//! Tests for `edge_deck_lib::agents` (`src-tauri/src/agents/`): the strate-core
//! engine run in-process and the `code_snapshot` data, against synthetic stores.
//!
//! The interface and document shape these tests pin:
//!
//! ```text
//! pub const POLL_EVERY: Duration            // == strate's engine default (1 s)
//! pub const CHANGED_EVENT: &str             // "code-changed"
//! pub fn store_path(app_data: &Path) -> PathBuf
//!     // edge-deck's own store file, under `app_data`; never `strate.db` /
//!     // `strate-dev.db`
//! pub fn config_root(claude_config_dir: Option<OsString>, home: Option<PathBuf>)
//!     -> Option<PathBuf>                    // the way strate resolves it
//! pub fn engine_config(root: PathBuf, app_data: &Path, claude: Option<PathBuf>)
//!     -> strate_core::engine::Config
//!     // root, db = store_path(app_data), poll.every = POLL_EVERY, registry =
//!     // cli_runner(claude) when given, github and hooks off, debounce default
//! pub fn start(config: Config, on_change: impl FnMut() + Send + 'static)
//!     -> Result<Engine, String>             // Err, never a panic
//! pub fn snapshot(conn: &rusqlite::Connection, now_ms: i64)
//!     -> Result<Snapshot, E: Debug>         // pure mapping of the store rows
//! pub struct Reader; Reader::new(db: PathBuf); Reader::read(&self, now_ms: i64)
//!     // -> Result<Snapshot, String>: opens `Store::open_reader(db)` on first
//!     // use, then `snapshot`; Err when the store is missing or unreadable,
//!     // and the next read opens it afresh (the command's error state)
//! pub fn wait_order(a: Option<i64>, b: Option<i64>) -> Ordering
//!     // longest wait first: smaller sinceMs first, None last
//! ```
//!
//! `Snapshot: Serialize`, camelCase:
//! `{ graphs, needsYou, header }`.
//! - `graphs[]`: `{ sessionId, startedAtMs, sessionCostUsd, agents[], edges[] }`,
//!   one per live root session, oldest root (smallest `startedAtMs`, null
//!   last, then sessionId) first.
//! - `agents[]`: `{ id, parent, kind, role, chargeKey, name, state, costUsd,
//!   contextPct, depth, focus }`; `depth` is 0 on the root orchestrator.
//! - `edges[]`: `{ parent, child, kind }`.
//! - `needsYou[]`: `{ label, waitingFor, sinceMs, focus }`, longest wait first.
//! - `header`: `{ startedAtMs, sessionCostUsd, todayUsd, liveAgents, needYou }`
//!   of the oldest live root; `todayUsd` is 0 with no spend.
//! - `focus`: `{ pid, sessionId, kind, title }` or null; `pid` is a u32 (0 when
//!   the session has no pid), `kind` is `interactive` or `background`
//!   (the registry's kind); a background session's `sessionId` is its bg id.
//!   A subagent or teammate carries its parent session's focus; the title is
//!   that session's orchestrator name (strate-core exposes no ai-title).

use std::cmp::Ordering;
use std::ffi::OsString;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::time::Duration;

use edge_deck_lib::agents;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use strate_core::discovery::{AgentRef, Graph, Link, SubagentMeta, SubagentNode};
use strate_core::engine::{DEBOUNCE, Engine};
use strate_core::hooks::{Hook, HookEvent};
use strate_core::registry::{PollOptions, RegistryEvent, parse_snapshot};
use strate_core::store::Store;
use strate_core::store::views;
use strate_core::tail::{Batch, TailEvent};

// 2026-10-08T12:00:00Z, the day of the fixture's records.
const NOW_MS: i64 = 1_791_460_800_000;
// 2026-01-01T12:00:00Z, a day with no spend.
const QUIET_DAY_MS: i64 = 1_767_268_800_000;

const PROJECT: &str = "/cfg/projects/-work-alpha";

/// A temp dir that is removed when it drops, so a run leaves nothing behind.
struct Scratch(PathBuf);

impl Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "edge-deck-code-{}-{}-{tag}",
        std::process::id(),
        NEXT.fetch_add(1, AtomicOrdering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    Scratch(dir)
}

/// One read on a fresh [`agents::Reader`].
fn read_snapshot(db: &Path, now_ms: i64) -> Result<agents::Snapshot, String> {
    agents::Reader::new(db.to_owned()).read(now_ms)
}

// ---- synthetic store, built through strate-core's public store API ----

fn batch(rel: &str, records: &[Value]) -> Batch {
    let path: Arc<Path> = Path::new(PROJECT).join(rel).into();
    let events = records
        .iter()
        .enumerate()
        .map(|(i, value)| TailEvent::Record {
            path: path.clone(),
            offset: i as u64 * 100,
            value: value.clone(),
        })
        .collect();
    Batch {
        path,
        events,
        checkpoint: None,
    }
}

fn title(name: &str) -> Value {
    json!({"type": "custom-title", "customTitle": name})
}

fn prompt(uuid: &str, at: &str) -> Value {
    json!({"type": "user", "uuid": uuid, "timestamp": at, "cwd": "/work/alpha",
           "message": {"role": "user", "content": "Lorem ipsum."}})
}

/// An assistant request on haiku ($1/M input, $0.10/M cache read, $5/M
/// output; 100 output tokens), with a `tool_use` block per dispatch id.
fn reply(uuid: &str, at: &str, input: u64, cached: u64, dispatches: &[&str]) -> Value {
    let content: Vec<Value> = dispatches
        .iter()
        .map(|id| json!({"type": "tool_use", "id": id, "name": "Agent", "input": {}}))
        .collect();
    json!({"type": "assistant", "uuid": uuid, "timestamp": at, "requestId": format!("req-{uuid}"),
           "message": {"id": format!("msg-{uuid}"), "model": "claude-haiku-4-5", "content": content,
                       "usage": {"input_tokens": input, "cache_read_input_tokens": cached,
                                 "output_tokens": 100}}})
}

fn dispatched(session: &str, agent: &str, tool_use_id: &str) -> SubagentNode {
    SubagentNode {
        session_id: session.into(),
        agent_id: agent.into(),
        path: Path::new(PROJECT).join(format!("{session}/subagents/agent-{agent}.jsonl")),
        meta: Some(SubagentMeta {
            agent_type: Some("implementer".into()),
            description: Some(format!("Lorem {agent}")),
            tool_use_id: Some(tool_use_id.into()),
            ..SubagentMeta::default()
        }),
        link: Link::Dispatch {
            parent: AgentRef::Session {
                session_id: session.into(),
            },
            tool_use_id: tool_use_id.into(),
        },
    }
}

fn appeared(json: &str) -> RegistryEvent {
    appeared_at(json, 5000)
}

fn appeared_at(json: &str, at_ms: i64) -> RegistryEvent {
    RegistryEvent::Appeared {
        entry: parse_snapshot(format!("[{json}]").as_bytes())
            .expect("entry")
            .remove(0),
        at_ms,
    }
}

/// Three live roots, ingested alpha, beta, gamma but started beta, alpha,
/// gamma (so neither insertion order nor strate's own tab order is "oldest
/// first"):
/// - `s-alpha`: interactive pid 4101, busy, two running subagents (a1; a2
///   asks permission by hook at 6000).
/// - `s-beta`: interactive pid 4102, started first, waits on `permission`
///   since 5000.
/// - `s-gamma`: a background session with no pid, running.
fn fill(store: &mut Store) {
    store
        .ingest(&batch(
            "s-alpha.jsonl",
            &[
                title("alpha-1"),
                prompt("u1", "2026-10-08T10:00:00.000Z"),
                // Latest depth 1000 + 49000 = 50000 of haiku's 200000: 25%.
                reply(
                    "u2",
                    "2026-10-08T10:00:10.000Z",
                    1000,
                    49_000,
                    &["toolu_1", "toolu_2"],
                ),
            ],
        ))
        .expect("alpha");
    for (agent, uuid, at) in [
        ("a1", "u4", "2026-10-08T10:00:20.000Z"),
        ("a2", "u5", "2026-10-08T10:00:21.000Z"),
    ] {
        store
            .ingest(&batch(
                &format!("s-alpha/subagents/agent-{agent}.jsonl"),
                &[reply(uuid, at, 2000, 0, &[])],
            ))
            .expect("subagent");
    }
    store
        .ingest(&batch(
            "s-beta.jsonl",
            &[
                title("beta-2"),
                prompt("b1", "2026-10-08T11:00:00.000Z"),
                reply("b2", "2026-10-08T11:00:05.000Z", 10, 0, &[]),
            ],
        ))
        .expect("beta");
    store
        .ingest(&batch(
            "s-gamma.jsonl",
            &[
                title("gamma-3"),
                prompt("g1", "2026-10-08T11:30:00.000Z"),
                reply("g2", "2026-10-08T11:30:05.000Z", 20, 0, &[]),
            ],
        ))
        .expect("gamma");
    store
        .ingest_graph(&Graph {
            subagents: vec![
                dispatched("s-alpha", "a1", "toolu_1"),
                dispatched("s-alpha", "a2", "toolu_2"),
            ],
            ..Graph::default()
        })
        .expect("graph");
    store
        .apply_registry(&[
            appeared(
                r#"{"pid":4101,"cwd":"/work/alpha","kind":"interactive","startedAt":1791000000000,
                    "sessionId":"s-alpha","name":"alpha-1","status":"busy"}"#,
            ),
            appeared(
                r#"{"pid":4102,"cwd":"/work/beta","kind":"interactive","startedAt":1790999000000,
                    "sessionId":"s-beta","name":"beta-2","status":"waiting","waitingFor":"permission"}"#,
            ),
            appeared(
                r#"{"id":"bg-3","cwd":"/work/gamma","kind":"background","startedAt":1791000200000,
                    "sessionId":"s-gamma","name":"gamma-3","state":"running"}"#,
            ),
        ])
        .expect("registry");
    store
        .apply_hook(&Hook {
            received_ms: 6000,
            event: HookEvent::PermissionRequest {
                session_id: "s-alpha".into(),
                agent_id: Some("a2".into()),
                tool_name: Some("Bash".into()),
            },
        })
        .expect("hook");
}

/// The fixture store's JSON snapshot, its db path and its dir.
fn fixture(tag: &str) -> (Value, PathBuf, Scratch) {
    let dir = scratch(tag);
    let db = dir.join("edge-deck-test.db");
    let mut store = Store::open(&db).expect("store");
    fill(&mut store);
    let conn = Store::open_reader(&db).expect("reader");
    let snap = agents::snapshot(&conn, NOW_MS).expect("snapshot");
    (serde_json::to_value(snap).expect("json"), db, dir)
}

fn graph_of<'a>(snap: &'a Value, session: &str) -> &'a Value {
    snap["graphs"]
        .as_array()
        .expect("graphs array")
        .iter()
        .find(|g| g["sessionId"] == session)
        .unwrap_or_else(|| panic!("no island for {session}: {snap}"))
}

fn agents_of(graph: &Value) -> &Vec<Value> {
    graph["agents"].as_array().expect("agents array")
}

fn root_of(graph: &Value) -> &Value {
    agents_of(graph)
        .iter()
        .find(|a| a["kind"] == "orchestrator")
        .expect("a root orchestrator")
}

fn close(v: &Value, want: f64) -> bool {
    v.as_f64().is_some_and(|got| (got - want).abs() < 1e-9)
}

// ---- islands ----

#[test]
fn one_island_per_live_root_oldest_root_first() {
    let (snap, _db, _dir) = fixture("islands");
    let sessions: Vec<&str> = snap["graphs"]
        .as_array()
        .expect("graphs")
        .iter()
        .map(|g| g["sessionId"].as_str().expect("sessionId"))
        .collect();
    // beta started 1790999000000, alpha 1791000000000, gamma 1791000200000.
    assert_eq!(sessions, ["s-beta", "s-alpha", "s-gamma"]);
    assert_eq!(snap["graphs"][0]["startedAtMs"], 1_790_999_000_000_i64);
    assert_eq!(snap["graphs"][1]["startedAtMs"], 1_791_000_000_000_i64);
}

#[test]
fn an_island_carries_its_agents_edges_cost_and_depth() {
    let (snap, _db, _dir) = fixture("agents");
    let alpha = graph_of(&snap, "s-alpha");
    // haiku: root request (1000 + 49000 cached + 100 out), two subagent
    // requests (2000 + 100 out each).
    let root_usd = (1000.0 + 49_000.0 * 0.1 + 500.0) / 1e6;
    let sub_usd = (2000.0 + 500.0) / 1e6;
    assert!(close(&alpha["sessionCostUsd"], root_usd + 2.0 * sub_usd));

    let all = agents_of(alpha);
    assert_eq!(all.len(), 3, "root and two running subagents: {alpha}");
    let root = root_of(alpha);
    assert_eq!(root["depth"], 0);
    assert_eq!(root["parent"], Value::Null);
    assert_eq!(root["state"], "working");
    assert_eq!(root["name"], "alpha-1");
    assert!(close(&root["costUsd"], root_usd));
    assert!(close(&root["contextPct"], 25.0));

    let subs: Vec<&Value> = all.iter().filter(|a| a["kind"] == "subagent").collect();
    assert_eq!(subs.len(), 2);
    for sub in &subs {
        assert_eq!(sub["depth"], 1);
        assert_eq!(sub["parent"], root["id"]);
        assert!(close(&sub["costUsd"], sub_usd));
    }
    let states: Vec<&str> = {
        let mut s: Vec<&str> = subs.iter().map(|a| a["state"].as_str().unwrap()).collect();
        s.sort_unstable();
        s
    };
    assert_eq!(states, ["needs_you", "working"]);

    let edges = alpha["edges"].as_array().expect("edges");
    assert_eq!(edges.len(), 2);
    for edge in edges {
        assert_eq!(edge["kind"], "dispatch");
        assert_eq!(edge["parent"], root["id"]);
        assert!(subs.iter().any(|s| s["id"] == edge["child"]));
    }
}

#[test]
fn every_agent_field_matches_the_live_graph_view_in_camel_case() {
    let (snap, db, _dir) = fixture("fields");
    let conn = Store::open_reader(&db).expect("reader");
    let tabs = views::workstreams(&conn).expect("tabs");
    let alpha_tab = tabs
        .iter()
        .find(|w| w.session_id.as_deref() == Some("s-alpha"))
        .expect("alpha tab");
    let view = views::live_graph(&conn, alpha_tab.id)
        .expect("query")
        .expect("graph");
    let graph = graph_of(&snap, "s-alpha");
    assert_eq!(agents_of(graph).len(), view.agents.len());
    for want in &view.agents {
        let got = agents_of(graph)
            .iter()
            .find(|a| a["id"] == want.id)
            .unwrap_or_else(|| panic!("agent {} missing", want.id));
        assert_eq!(got["kind"], want.kind.as_str());
        assert_eq!(got["role"], json!(want.role));
        assert_eq!(got["chargeKey"], json!(want.charge_key));
        assert_eq!(got["name"], json!(want.name));
        assert_eq!(got["costUsd"], json!(want.cost_usd));
        assert_eq!(got["contextPct"], json!(want.context_pct));
        assert_eq!(got["parent"], json!(want.parent));
    }
    for key in ["charge_key", "cost_usd", "context_pct", "session_id"] {
        assert!(
            snap.to_string().find(&format!("\"{key}\"")).is_none(),
            "snake_case key {key} leaked into the JSON"
        );
    }
}

// ---- needs you ----

#[test]
fn needs_you_lists_every_wait_longest_first_with_labels_and_wait_text() {
    let (snap, _db, _dir) = fixture("needs");
    let rows = snap["needsYou"].as_array().expect("needsYou");
    assert_eq!(rows.len(), 2, "{rows:?}");
    // beta's registry wait started at 5000, alpha a2's hook wait at 6000.
    assert_eq!(rows[0]["sinceMs"], 5000);
    assert_eq!(rows[0]["waitingFor"], "permission");
    assert_eq!(rows[1]["sinceMs"], 6000);
    assert_eq!(rows[1]["waitingFor"], "Bash");
    let label0 = rows[0]["label"].as_str().expect("label");
    let label1 = rows[1]["label"].as_str().expect("label");
    assert!(label0.contains("beta-2"), "{label0}");
    // a2 has no name or role of its own, so its session's name labels it.
    assert!(label1.contains("alpha-1"), "{label1}");
}

#[test]
fn a_wait_with_no_start_sorts_after_every_dated_wait() {
    let mut waits = [Some(9), None, Some(3), None, Some(5)];
    waits.sort_by(|a, b| agents::wait_order(*a, *b));
    assert_eq!(waits, [Some(3), Some(5), Some(9), None, None]);
    assert_eq!(agents::wait_order(Some(1), Some(1)), Ordering::Equal);
    assert_eq!(agents::wait_order(None, None), Ordering::Equal);
    assert_eq!(agents::wait_order(Some(1), None), Ordering::Less);
    assert_eq!(agents::wait_order(None, Some(1)), Ordering::Greater);
}

// ---- header ----

#[test]
fn the_header_is_the_oldest_root_plus_totals() {
    let (snap, db, _dir) = fixture("header");
    let conn = Store::open_reader(&db).expect("reader");
    let header = &snap["header"];
    // The oldest root is beta: one request, 10 in + 100 out on haiku.
    assert_eq!(header["startedAtMs"], 1_790_999_000_000_i64);
    assert!(close(&header["sessionCostUsd"], (10.0 + 500.0) / 1e6));
    // alpha 3 (root, a1 working; a2 needs you) + beta 1 + gamma 1.
    assert_eq!(header["liveAgents"], 5);
    assert_eq!(header["needYou"], 2);
    assert_eq!(
        header["needYou"],
        snap["needsYou"].as_array().expect("rows").len()
    );
    let today = views::spend_today(&conn, NOW_MS)
        .expect("spend")
        .cost_usd
        .unwrap_or(0.0);
    assert!(close(&header["todayUsd"], today), "{header}");
    assert!(close(&header["todayUsd"], FIXTURE_DAY_USD), "{header}");
    assert!(header["todayUsd"].as_f64().is_some_and(|v| v > 0.0));
}

/// The fixture's five requests on haiku, summed by hand: alpha's root
/// (1000 in + 49000 cached + 100 out), its two subagents (2000 in + 100 out
/// each), beta (10 in + 100 out) and gamma (20 in + 100 out).
const FIXTURE_DAY_USD: f64 =
    ((1000.0 + 49_000.0 * 0.1 + 500.0) + 2.0 * (2000.0 + 500.0) + (10.0 + 500.0) + (20.0 + 500.0))
        / 1e6;

#[test]
fn today_usd_follows_the_clock_it_is_given() {
    let (_snap, db, _dir) = fixture("today");
    let conn = Store::open_reader(&db).expect("reader");
    let busy =
        serde_json::to_value(agents::snapshot(&conn, NOW_MS).expect("snapshot")).expect("json");
    let quiet = serde_json::to_value(agents::snapshot(&conn, QUIET_DAY_MS).expect("snapshot"))
        .expect("json");
    // The fixture does have spend on its own day, so the quiet 0 is the
    // clock's doing, not an empty store.
    assert!(close(&busy["header"]["todayUsd"], FIXTURE_DAY_USD));
    assert!(busy["header"]["todayUsd"].as_f64().is_some_and(|v| v > 0.0));
    assert_eq!(quiet["header"]["todayUsd"], 0.0);
}

#[test]
fn an_empty_store_reads_as_an_empty_snapshot() {
    let dir = scratch("empty");
    let db = dir.join("empty.db");
    drop(Store::open(&db).expect("store"));
    let snap = serde_json::to_value(read_snapshot(&db, NOW_MS).expect("snapshot")).expect("json");
    assert_eq!(snap["graphs"], json!([]));
    assert_eq!(snap["needsYou"], json!([]));
    assert_eq!(snap["header"]["liveAgents"], 0);
    assert_eq!(snap["header"]["needYou"], 0);
    assert_eq!(snap["header"]["startedAtMs"], Value::Null);
    assert_eq!(snap["header"]["sessionCostUsd"], Value::Null);
    assert_eq!(snap["header"]["todayUsd"], 0.0);
}

#[test]
fn a_missing_store_is_an_error_not_a_panic() {
    let dir = scratch("missing");
    let missing = dir.join("no-such.db");
    assert!(read_snapshot(&missing, NOW_MS).is_err());
}

#[test]
fn a_failed_open_is_retried_on_the_same_reader() {
    let dir = scratch("retry-open");
    let db = dir.join("late.db");
    let reader = agents::Reader::new(db.clone());
    let err = reader.read(NOW_MS).expect_err("no store yet");
    assert!(err.contains("did not open"), "{err}");
    drop(Store::open(&db).expect("store"));
    let snap = reader.read(NOW_MS).expect("the store now exists");
    assert_eq!(
        serde_json::to_value(snap).expect("json")["graphs"],
        json!([])
    );
}

#[test]
fn a_failed_read_drops_the_connection_and_the_next_read_reopens() {
    let dir = scratch("drop-conn");
    let db = dir.join("broken.db");
    drop(Store::open(&db).expect("store"));
    let reader = agents::Reader::new(db.clone());
    reader.read(NOW_MS).expect("a healthy first read");
    // Break the schema under the open reader.
    let writer = Connection::open(&db).expect("writer");
    writer
        .execute_batch("DROP TABLE registry")
        .expect("drop a table the views read");
    drop(writer);
    let err = reader.read(NOW_MS).expect_err("the read fails");
    assert!(err.contains("did not read"), "{err}");
    // The failed read released the file: it can be removed (Windows refuses
    // while a connection holds it), and the next read tries to open afresh
    // rather than reading on the stale connection.
    std::fs::remove_file(&db).expect("no connection holds the store");
    let err = reader.read(NOW_MS).expect_err("the store is gone");
    assert!(err.contains("did not open"), "{err}");
    drop(Store::open(&db).expect("store"));
    reader.read(NOW_MS).expect("a rebuilt store reads again");
}

// ---- focus ----

#[test]
fn focus_names_the_session_terminal_and_a_subagent_shares_its_parents() {
    let (snap, _db, _dir) = fixture("focus");
    let alpha = graph_of(&snap, "s-alpha");
    let root = root_of(alpha);
    let root_focus = &root["focus"];
    assert_eq!(root_focus["pid"], 4101);
    assert_eq!(root_focus["sessionId"], "s-alpha");
    assert_eq!(root_focus["kind"], "interactive");
    // strate-core's name for the orchestrator; no ai-title exists.
    assert_eq!(root_focus["title"], "alpha-1");
    for sub in agents_of(alpha).iter().filter(|a| a["kind"] == "subagent") {
        assert_eq!(
            sub["focus"], *root_focus,
            "a subagent runs in its parent's terminal"
        );
    }
    // The waiting subagent's needs-you row focuses that same terminal.
    let a2_wait = snap["needsYou"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["waitingFor"] == "Bash")
        .expect("a2's wait");
    assert_eq!(a2_wait["focus"], *root_focus);
    // beta's own session wait focuses beta's terminal.
    let beta_wait = &snap["needsYou"][0];
    assert_eq!(beta_wait["focus"]["pid"], 4102);
    assert_eq!(beta_wait["focus"]["sessionId"], "s-beta");
    assert_eq!(beta_wait["focus"]["kind"], "interactive");
}

#[test]
fn a_session_with_no_pid_focuses_as_background() {
    let (snap, _db, _dir) = fixture("background");
    let gamma = graph_of(&snap, "s-gamma");
    let focus = &root_of(gamma)["focus"];
    assert_eq!(focus["kind"], "background");
    // `claude attach` takes the registry's short id (bg-3), not the session id.
    assert_eq!(focus["sessionId"], "bg-3");
    assert_eq!(focus["pid"], 0);
    assert_eq!(focus["title"], "gamma-3");
}

/// Four live roots, each a different registry shape:
/// - `s-run`: a running background session with pid 4401 and bg id
///   `abcd1234`, with one subagent;
/// - `s-live`: interactive, pid 4402, no bg id;
/// - `s-nobg`: background with pid 4403 and no bg id;
/// - `s-norow`: a transcript with no registry row (never live, so never drawn).
fn kinds_fixture(tag: &str) -> (Value, Scratch) {
    let dir = scratch(tag);
    let db = dir.join("kinds.db");
    let mut store = Store::open(&db).expect("store");
    for (session, uuid, replies) in [
        ("s-run", "r", vec!["toolu_9"]),
        ("s-live", "l", vec![]),
        ("s-nobg", "n", vec![]),
        ("s-norow", "z", vec![]),
    ] {
        store
            .ingest(&batch(
                &format!("{session}.jsonl"),
                &[
                    title(session),
                    prompt(&format!("{uuid}1"), "2026-10-08T10:00:00.000Z"),
                    reply(
                        &format!("{uuid}2"),
                        "2026-10-08T10:00:10.000Z",
                        10,
                        0,
                        &replies,
                    ),
                ],
            ))
            .expect("session");
    }
    store
        .ingest(&batch(
            "s-run/subagents/agent-r1.jsonl",
            &[reply("r4", "2026-10-08T10:00:20.000Z", 10, 0, &[])],
        ))
        .expect("subagent");
    store
        .ingest_graph(&Graph {
            subagents: vec![dispatched("s-run", "r1", "toolu_9")],
            ..Graph::default()
        })
        .expect("graph");
    store
        .apply_registry(&[
            appeared(
                r#"{"pid":4401,"id":"abcd1234","cwd":"/work/run","kind":"background",
                    "startedAt":1791000000000,"sessionId":"s-run","name":"s-run","state":"running"}"#,
            ),
            appeared(
                r#"{"pid":4402,"cwd":"/work/live","kind":"interactive","startedAt":1791000100000,
                    "sessionId":"s-live","name":"s-live","status":"busy"}"#,
            ),
            appeared(
                r#"{"pid":4403,"cwd":"/work/nobg","kind":"background","startedAt":1791000200000,
                    "sessionId":"s-nobg","name":"s-nobg","state":"running"}"#,
            ),
        ])
        .expect("registry");
    let conn = Store::open_reader(&db).expect("reader");
    let snap = agents::snapshot(&conn, NOW_MS).expect("snapshot");
    (serde_json::to_value(snap).expect("json"), dir)
}

#[test]
fn a_running_background_session_with_a_pid_focuses_by_its_bg_id() {
    let (snap, _dir) = kinds_fixture("running-bg");
    let run = graph_of(&snap, "s-run");
    let focus = &root_of(run)["focus"];
    assert_eq!(focus["kind"], "background");
    assert_eq!(focus["sessionId"], "abcd1234");
    assert_eq!(focus["pid"], 4401);
    let sub = agents_of(run)
        .iter()
        .find(|a| a["kind"] == "subagent")
        .expect("its subagent");
    assert_eq!(sub["focus"], *focus, "the subagent follows its session");
}

#[test]
fn an_interactive_session_keeps_its_pid_session_id_and_kind() {
    let (snap, _dir) = kinds_fixture("interactive");
    let focus = &root_of(graph_of(&snap, "s-live"))["focus"];
    assert_eq!(focus["kind"], "interactive");
    assert_eq!(focus["sessionId"], "s-live");
    assert_eq!(focus["pid"], 4402);
}

#[test]
fn a_background_session_with_no_bg_id_keeps_its_session_id() {
    let (snap, _dir) = kinds_fixture("bg-no-id");
    let focus = &root_of(graph_of(&snap, "s-nobg"))["focus"];
    assert_eq!(focus["kind"], "background");
    assert_eq!(focus["sessionId"], "s-nobg");
    assert_eq!(focus["pid"], 4403);
    // No registry row, no live tab: nothing is drawn to focus.
    assert!(
        snap["graphs"]
            .as_array()
            .expect("graphs")
            .iter()
            .all(|g| g["sessionId"] != "s-norow")
    );
}

// ---- waits: bg ids, null sinceMs, label fallbacks ----

/// Four waits, inserted out of order, each a different kind of row:
/// - `s-delta`: a background job parked on the user (state blocked), registry
///   short id `bg-4`, since 7000;
/// - `s-eps`: the same, but the registry gave it no id, since 6000;
/// - `s-ghost`: an interactive session with no transcript or name waiting on
///   a permission, since 8000;
/// - a blocked job with no session at all, whose wait start is then lost.
fn waits_fixture(tag: &str) -> (Value, Scratch) {
    let dir = scratch(tag);
    let db = dir.join("waits.db");
    let mut store = Store::open(&db).expect("store");
    for (session, name, uuid) in [("s-delta", "delta-4", "d"), ("s-eps", "eps-5", "e")] {
        store
            .ingest(&batch(
                &format!("{session}.jsonl"),
                &[
                    title(name),
                    prompt(&format!("{uuid}1"), "2026-10-08T09:00:00.000Z"),
                    reply(&format!("{uuid}2"), "2026-10-08T09:00:05.000Z", 10, 0, &[]),
                ],
            ))
            .expect("session");
    }
    store
        .apply_registry(&[
            appeared_at(
                r#"{"id":"bg-4","cwd":"/work/delta","kind":"background","startedAt":1791000000000,
                    "sessionId":"s-delta","name":"delta-4","state":"blocked"}"#,
                7000,
            ),
            appeared_at(
                r#"{"id":"bg-7","cwd":"/work/none","kind":"background","startedAt":1791000300000,
                    "state":"blocked"}"#,
                9000,
            ),
            appeared_at(
                r#"{"pid":4301,"cwd":"/work/ghost","kind":"interactive","startedAt":1791000400000,
                    "sessionId":"s-ghost","status":"waiting","waitingFor":"permission"}"#,
                8000,
            ),
            appeared_at(
                r#"{"id":"","cwd":"/work/eps","kind":"background","startedAt":1791000100000,
                    "sessionId":"s-eps","name":"eps-5","state":"blocked"}"#,
                6000,
            ),
        ])
        .expect("registry");
    drop(store);
    Connection::open(&db)
        .expect("raw")
        .execute(
            "UPDATE registry SET waiting_since_ms = NULL WHERE key = 'id:bg-7'",
            [],
        )
        .expect("lose the wait start");
    let conn = Store::open_reader(&db).expect("reader");
    let snap = agents::snapshot(&conn, NOW_MS).expect("snapshot");
    (serde_json::to_value(snap).expect("json"), dir)
}

fn wait_since(snap: &Value, since: i64) -> &Value {
    snap["needsYou"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["sinceMs"] == since)
        .unwrap_or_else(|| panic!("no wait since {since}: {}", snap["needsYou"]))
}

#[test]
fn waits_with_no_start_come_last_through_the_whole_snapshot() {
    let (snap, _dir) = waits_fixture("null-since");
    let since: Vec<&Value> = snap["needsYou"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| &r["sinceMs"])
        .collect();
    assert_eq!(
        since,
        [&json!(6000), &json!(7000), &json!(8000), &Value::Null]
    );
    assert_eq!(snap["header"]["needYou"], 4);
}

#[test]
fn a_parked_background_wait_focuses_by_its_bg_id_not_its_session_id() {
    let (snap, _dir) = waits_fixture("bg-id");
    let focus = &wait_since(&snap, 7000)["focus"];
    // `claude attach` takes the registry's short id.
    assert_eq!(focus["sessionId"], "bg-4");
    assert_eq!(focus["kind"], "background");
    assert_eq!(focus["pid"], 0);
    // No live graph names the parked orchestrator, so the title is the tab's
    // name: the charge key strate-core's views give the bare name `delta-4`.
    assert_eq!(focus["title"], "issue_delta-4");
}

#[test]
fn a_wait_with_no_bg_id_keeps_its_session_id_to_focus_by() {
    let (snap, _dir) = waits_fixture("no-bg-id");
    let parked = &wait_since(&snap, 6000)["focus"];
    assert_eq!(parked["sessionId"], "s-eps");
    assert_eq!(parked["kind"], "background");
    // A session with no transcript has no tab, so nothing is known to focus.
    assert_eq!(wait_since(&snap, 8000)["focus"], Value::Null);
}

#[test]
fn a_background_wait_with_a_bg_id_and_no_reason_reads_blocked_in_the_background() {
    let (snap, _dir) = waits_fixture("bg-reason");
    // bg-4 (since 7000) has a bg id and the registry gave it no waitingFor.
    assert_eq!(
        wait_since(&snap, 7000)["waitingFor"],
        "blocked in the background"
    );
    // bg-7 has a bg id too, though its wait start was lost (sorts last).
    let lost = &snap["needsYou"][3];
    assert_eq!(lost["waitingFor"], "blocked in the background");
}

#[test]
fn a_wait_with_neither_a_bg_id_nor_a_reason_keeps_no_reason() {
    let (snap, _dir) = waits_fixture("no-reason");
    // s-eps (since 6000): the registry gave it an empty id and no waitingFor.
    assert_eq!(wait_since(&snap, 6000)["waitingFor"], Value::Null);
    // A stated reason is untouched.
    assert_eq!(wait_since(&snap, 8000)["waitingFor"], "permission");
}

#[test]
fn a_wait_is_labelled_by_its_session_id_then_as_unknown() {
    let (snap, _dir) = waits_fixture("labels");
    // No role, no name and no tab name to go by: the raw session id.
    assert_eq!(wait_since(&snap, 8000)["label"], "s-ghost");
    // No session either.
    let lost = &snap["needsYou"][3];
    assert_eq!(lost["label"], "unknown session");
    assert_eq!(lost["focus"], Value::Null);
}

// ---- a nested live chain and a teammate ----

/// `s-dir` (interactive pid 4201) dispatches `w1` and has teammate `m1`;
/// `w1` spawned the live session `s-unit` (interactive pid 4202), a
/// `directs` edge, which dispatches `u1`. `m1` and `u1` each wait on a tool.
fn company(tag: &str) -> (Value, Scratch) {
    let dir = scratch(tag);
    let db = dir.join("company.db");
    let mut store = Store::open(&db).expect("store");
    store
        .ingest(&batch(
            "s-dir.jsonl",
            &[
                title("dir-1"),
                prompt("r1", "2026-10-08T10:00:00.000Z"),
                reply("r2", "2026-10-08T10:00:10.000Z", 1000, 0, &["toolu_1"]),
            ],
        ))
        .expect("director");
    store
        .ingest(&batch(
            "s-unit.jsonl",
            &[
                title("unit-2"),
                prompt("n1", "2026-10-08T10:05:00.000Z"),
                reply("n2", "2026-10-08T10:05:10.000Z", 1000, 0, &["toolu_9"]),
            ],
        ))
        .expect("unit");
    for (rel, uuid, at) in [
        ("s-dir/subagents/agent-w1.jsonl", "r3", "10:00:20"),
        ("s-dir/subagents/agent-m1.jsonl", "r4", "10:00:21"),
        ("s-unit/subagents/agent-u1.jsonl", "n3", "10:05:20"),
    ] {
        store
            .ingest(&batch(
                rel,
                &[reply(uuid, &format!("2026-10-08T{at}.000Z"), 2000, 0, &[])],
            ))
            .expect("worker");
    }
    let mate = SubagentNode {
        session_id: "s-dir".into(),
        agent_id: "m1".into(),
        path: Path::new(PROJECT).join("s-dir/subagents/agent-m1.jsonl"),
        meta: Some(SubagentMeta {
            name: Some("mate-1".into()),
            team_name: Some("team-lorem".into()),
            ..SubagentMeta::default()
        }),
        link: Link::Teammate {
            team_name: "team-lorem".into(),
            team_dir: None,
        },
    };
    store
        .ingest_graph(&Graph {
            subagents: vec![
                dispatched("s-dir", "w1", "toolu_1"),
                mate,
                dispatched("s-unit", "u1", "toolu_9"),
            ],
            ..Graph::default()
        })
        .expect("graph");
    store
        .apply_registry(&[
            appeared(
                r#"{"pid":4201,"cwd":"/work/dir","kind":"interactive","startedAt":1791000000000,
                    "sessionId":"s-dir","name":"dir-1","status":"busy"}"#,
            ),
            appeared(
                r#"{"pid":4202,"cwd":"/work/unit","kind":"interactive","startedAt":1791000300000,
                    "sessionId":"s-unit","name":"unit-2","status":"busy"}"#,
            ),
        ])
        .expect("registry");
    for (session, agent, tool, at) in [
        ("s-dir", "m1", "Bash", 6000),
        ("s-unit", "u1", "Edit", 7000),
    ] {
        store
            .apply_hook(&Hook {
                received_ms: at,
                event: HookEvent::PermissionRequest {
                    session_id: session.into(),
                    agent_id: Some(agent.into()),
                    tool_name: Some(tool.into()),
                },
            })
            .expect("hook");
    }
    drop(store);
    // A spawn of one session by another worker is a `message` edge, which
    // the graph draws as `directs`; the store API has no way to write one.
    let raw = Connection::open(&db).expect("raw");
    let row = |session: &str, agent: Option<&str>| -> i64 {
        raw.query_row(
            "SELECT id FROM agents WHERE session_id = ?1 AND agent_id IS ?2",
            params![session, agent],
            |r| r.get(0),
        )
        .expect("agent row")
    };
    raw.execute(
        "INSERT INTO edges (kind, parent, child, tool_use_id) VALUES ('message', ?1, ?2, NULL)",
        params![row("s-dir", Some("w1")), row("s-unit", None)],
    )
    .expect("directs edge");
    drop(raw);
    let conn = Store::open_reader(&db).expect("reader");
    let snap = agents::snapshot(&conn, NOW_MS).expect("snapshot");
    (serde_json::to_value(snap).expect("json"), dir)
}

fn agent_where<'a>(island: &'a Value, kind: &str, depth: u64) -> &'a Value {
    agents_of(island)
        .iter()
        .find(|a| a["kind"] == kind && a["depth"] == depth)
        .unwrap_or_else(|| panic!("no {kind} at depth {depth}: {island}"))
}

#[test]
fn a_nested_live_chain_is_one_island_with_directs_and_its_own_focus() {
    let (snap, _dir) = company("company");
    assert_eq!(
        snap["graphs"].as_array().expect("graphs").len(),
        1,
        "the spawned unit is drawn inside its director's island: {snap}"
    );
    let island = graph_of(&snap, "s-dir");
    let root = agent_where(island, "orchestrator", 0);
    let worker = agent_where(island, "subagent", 1);
    let unit = agent_where(island, "orchestrator", 2);
    let unit_worker = agent_where(island, "subagent", 3);
    assert_eq!(unit["name"], "unit-2");
    assert_eq!(unit["parent"], worker["id"]);
    assert_eq!(unit_worker["parent"], unit["id"]);

    let edges = island["edges"].as_array().expect("edges");
    assert!(
        edges.iter().any(|e| e["kind"] == "directs"
            && e["parent"] == worker["id"]
            && e["child"] == unit["id"]),
        "{edges:?}"
    );

    // The director's terminal for its own tree, the unit's for the unit's.
    assert_eq!(root["focus"]["pid"], 4201);
    assert_eq!(root["focus"]["sessionId"], "s-dir");
    assert_eq!(worker["focus"], root["focus"]);
    assert_eq!(unit["focus"]["pid"], 4202);
    assert_eq!(unit["focus"]["sessionId"], "s-unit");
    assert_eq!(unit["focus"]["kind"], "interactive");
    assert_eq!(unit["focus"]["title"], "unit-2");
    assert_eq!(unit_worker["focus"], unit["focus"]);
}

#[test]
fn a_teammate_carries_its_leads_focus() {
    let (snap, _dir) = company("teammate");
    let island = graph_of(&snap, "s-dir");
    let root = agent_where(island, "orchestrator", 0);
    let mate = agent_where(island, "teammate", 1);
    assert_eq!(mate["name"], "mate-1");
    assert_eq!(mate["parent"], root["id"]);
    assert_eq!(mate["focus"], root["focus"]);
    assert_eq!(mate["focus"]["pid"], 4201);

    // Its wait, and the unit worker's, focus the terminal each runs in.
    let mate_wait = wait_since(&snap, 6000);
    assert!(
        mate_wait["label"]
            .as_str()
            .is_some_and(|l| l.contains("mate-1"))
    );
    assert_eq!(mate_wait["focus"], root["focus"]);
    let unit_wait = wait_since(&snap, 7000);
    assert_eq!(unit_wait["focus"]["pid"], 4202);
    assert_eq!(unit_wait["focus"]["sessionId"], "s-unit");
}

// ---- engine wiring ----

#[test]
fn the_store_lives_under_the_app_data_dir_and_is_never_strates() {
    let one = Path::new("/app-data/edge-deck");
    let two = Path::new("/elsewhere/edge-deck");
    let path = agents::store_path(one);
    assert!(path.starts_with(one), "{}", path.display());
    assert_ne!(path, one, "a file inside the dir, not the dir");
    assert!(agents::store_path(two).starts_with(two));
    let name = path.file_name().expect("file name").to_string_lossy();
    assert!(
        name != "strate.db" && name != "strate-dev.db" && !name.contains("strate"),
        "edge-deck's own store, not strate's: {name}"
    );
}

#[test]
fn the_poll_interval_is_the_engine_default_until_measured() {
    assert_eq!(agents::POLL_EVERY, PollOptions::default().every);
    assert_eq!(agents::POLL_EVERY, Duration::from_secs(1));
    assert_eq!(agents::CHANGED_EVENT, "code-changed");
}

#[test]
fn the_config_dir_resolves_like_strate_does() {
    let home = Some(PathBuf::from("/home/u"));
    assert_eq!(
        agents::config_root(Some(OsString::from("/cfg/alt")), home.clone()),
        Some(PathBuf::from("/cfg/alt"))
    );
    assert_eq!(
        agents::config_root(None, home.clone()),
        Some(PathBuf::from("/home/u/.claude"))
    );
    assert_eq!(
        agents::config_root(Some(OsString::new()), home),
        Some(PathBuf::from("/home/u/.claude")),
        "an empty CLAUDE_CONFIG_DIR is unset"
    );
    assert_eq!(agents::config_root(None, None), None);
}

#[test]
fn the_engine_config_puts_the_store_in_app_data_and_keeps_default_timing() {
    let app_data = Path::new("/app-data/edge-deck");
    let config = agents::engine_config(PathBuf::from("/cfg/root"), app_data, None);
    assert_eq!(config.root, PathBuf::from("/cfg/root"));
    assert_eq!(config.db, agents::store_path(app_data));
    assert_eq!(config.poll.every, agents::POLL_EVERY);
    assert_eq!(config.debounce, DEBOUNCE);
    assert!(config.registry.is_none(), "no claude binary, registry off");
    assert!(config.hooks.is_none());
    assert!(config.github.is_none());

    let with_claude = agents::engine_config(
        PathBuf::from("/cfg/root"),
        app_data,
        Some(PathBuf::from("/bin/claude")),
    );
    assert!(with_claude.registry.is_some());
}

#[test]
fn the_engine_starts_into_its_own_store_and_stops() {
    let root = scratch("engine-root");
    let app_data = scratch("engine-data");
    let config = agents::engine_config(root.to_path_buf(), &app_data, None);
    let engine: Engine = agents::start(config, || {}).expect("engine starts");
    assert!(agents::store_path(&app_data).is_file());
    engine.stop();
}

#[test]
fn a_store_that_cannot_open_is_an_error_the_app_survives() {
    let root = scratch("bad-root");
    let data = scratch("bad-data");
    let app_data = data.join("no-such-dir");
    let config = agents::engine_config(root.to_path_buf(), &app_data, None);
    let started = agents::start(config, || {});
    assert!(started.is_err());
    assert!(!started.err().expect("error").is_empty());
}
