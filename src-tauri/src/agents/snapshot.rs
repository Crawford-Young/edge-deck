//! The pure mapping from edge-deck's store to the Code setup's snapshot:
//! strate-core's live views in, one camelCase document out.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use rusqlite::Connection;
use serde::Serialize;
use strate_core::store::Store;
use strate_core::store::views::{self, AgentState, GraphAgent, LiveGraph, NeedsYou, Workstream};

use crate::focus::SessionKind;

/// The label of a wait with no session to name it by.
const UNKNOWN: &str = "unknown session";

/// Everything the Code setup's graph, needs-you panel and header show.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// One island per live root session, oldest root first.
    pub graphs: Vec<Island>,
    /// Every wait on the user, longest first.
    pub needs_you: Vec<Wait>,
    pub header: Header,
}

/// One live root session's running tree.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Island {
    pub session_id: String,
    pub started_at_ms: Option<i64>,
    pub session_cost_usd: Option<f64>,
    pub agents: Vec<Agent>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: i64,
    pub parent: Option<i64>,
    /// orchestrator | subagent | teammate.
    pub kind: String,
    pub role: Option<String>,
    pub charge_key: Option<String>,
    pub name: Option<String>,
    pub state: AgentState,
    pub cost_usd: Option<f64>,
    pub context_pct: Option<f64>,
    /// Steps from the island's root orchestrator, which is 0.
    pub depth: u32,
    pub focus: Option<Focus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub parent: i64,
    pub child: i64,
    pub kind: String,
}

/// One wait on the user.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Wait {
    /// The waiting agent's role or name, else its session's name.
    pub label: String,
    pub waiting_for: Option<String>,
    pub since_ms: Option<i64>,
    pub focus: Option<Focus>,
}

/// The oldest live root's start and $, plus totals.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub started_at_ms: Option<i64>,
    pub session_cost_usd: Option<f64>,
    /// Today's spend; 0 with none.
    pub today_usd: f64,
    /// Agents working or needing you, across every island.
    pub live_agents: usize,
    /// The number of waits in `needsYou`.
    pub need_you: usize,
}

/// The arguments of the `focus_terminal` command for the session whose
/// terminal an agent runs in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Focus {
    /// The session's process id; 0 with none.
    pub pid: u32,
    pub session_id: String,
    /// `background` when the session has no pid.
    pub kind: SessionKind,
    /// The session orchestrator's name as strate-core gives it; strate-core
    /// exposes no ai-title.
    pub title: String,
}

/// Longest wait first: a smaller `sinceMs` first, a wait with none last.
pub fn wait_order(a: Option<i64>, b: Option<i64>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// One long-lived reader on the store at `db`, opened on first use and kept
/// so strate-core's prepared statements stay cached between reads.
pub struct Reader {
    db: PathBuf,
    conn: Mutex<Option<Connection>>,
}

impl Reader {
    pub fn new(db: PathBuf) -> Self {
        Self {
            db,
            conn: Mutex::new(None),
        }
    }

    /// Reads the store's [`snapshot`]. A store that is missing or unreadable
    /// is an error message; the next read opens it afresh.
    pub fn read(&self, now_ms: i64) -> Result<Snapshot, String> {
        let mut slot = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
            let conn = Store::open_reader(&self.db)
                .map_err(|e| format!("the Code store did not open: {e}"))?;
            *slot = Some(conn);
        }
        let conn = slot.as_ref().expect("the reader was just opened");
        snapshot(conn, now_ms).map_err(|e| {
            *slot = None;
            format!("the Code store did not read: {e}")
        })
    }
}

/// Opens a reader on the store at `db` for one [`snapshot`] read.
pub fn read_snapshot(db: &Path, now_ms: i64) -> Result<Snapshot, String> {
    Reader::new(db.to_owned()).read(now_ms)
}

/// The snapshot of the store on `conn`, with today's spend for the local
/// day `now_ms` falls on.
pub fn snapshot(conn: &Connection, now_ms: i64) -> strate_core::store::Result<Snapshot> {
    let tabs = views::workstreams(conn)?;
    let mut graphs = Vec::new();
    for tab in tabs.iter().filter(|t| t.live && t.director.is_none()) {
        if let Some(graph) = views::live_graph(conn, tab.id)? {
            graphs.push(graph);
        }
    }
    graphs.sort_by(|a, b| {
        wait_order(a.started_at_ms, b.started_at_ms).then_with(|| a.session_id.cmp(&b.session_id))
    });
    let focus = focus_by_session(&tabs, &graphs);
    let mut waits = views::needs_you(conn)?;
    waits.sort_by(|a, b| wait_order(a.since_ms, b.since_ms));
    let needs_you: Vec<Wait> = waits.iter().map(|w| wait(w, &graphs, &focus)).collect();
    let graphs: Vec<Island> = graphs.iter().map(|g| island(g, &focus)).collect();
    let live_agents = graphs
        .iter()
        .flat_map(|g| &g.agents)
        .filter(|a| matches!(a.state, AgentState::Working | AgentState::NeedsYou))
        .count();
    let today_usd = views::spend_today(conn, now_ms)?.cost_usd.unwrap_or(0.0);
    let oldest = graphs.first();
    let header = Header {
        started_at_ms: oldest.and_then(|g| g.started_at_ms),
        session_cost_usd: oldest.and_then(|g| g.session_cost_usd),
        today_usd,
        live_agents,
        need_you: needs_you.len(),
    };
    Ok(Snapshot {
        graphs,
        needs_you,
        header,
    })
}

/// Each drawn session's terminal: its tab's pid, titled by its
/// orchestrator's name, else the tab's name.
fn focus_by_session(tabs: &[Workstream], graphs: &[LiveGraph]) -> HashMap<String, Focus> {
    // A session outside every live graph (a parked background job) has no
    // graph agent, so it falls back to the tab's name.
    let names: HashMap<String, String> = graphs
        .iter()
        .flat_map(|g| &g.agents)
        .filter(|a| a.agent_id.is_none())
        .filter_map(|a| Some((a.session_id.clone(), a.name.clone()?)))
        .collect();
    tabs.iter()
        .filter_map(|tab| {
            let session = tab.session_id.as_deref()?;
            let pid = tab.pid.and_then(|p| u32::try_from(p).ok());
            let focus = Focus {
                pid: pid.unwrap_or(0),
                session_id: session.to_owned(),
                kind: if pid.is_some() {
                    SessionKind::Interactive
                } else {
                    SessionKind::Background
                },
                title: names.get(session).unwrap_or(&tab.name).clone(),
            };
            Some((session.to_owned(), focus))
        })
        .collect()
}

fn island(graph: &LiveGraph, focus: &HashMap<String, Focus>) -> Island {
    let parents: HashMap<i64, Option<i64>> =
        graph.agents.iter().map(|a| (a.id, a.parent)).collect();
    Island {
        session_id: graph.session_id.clone(),
        started_at_ms: graph.started_at_ms,
        session_cost_usd: graph.session_cost_usd,
        agents: graph
            .agents
            .iter()
            .map(|a| agent(a, depth(a.id, &parents), focus))
            .collect(),
        edges: graph
            .edges
            .iter()
            .map(|e| Edge {
                parent: e.parent,
                child: e.child,
                kind: e.kind.clone(),
            })
            .collect(),
    }
}

fn agent(a: &GraphAgent, depth: u32, focus: &HashMap<String, Focus>) -> Agent {
    Agent {
        id: a.id,
        parent: a.parent,
        kind: a.kind.clone(),
        role: a.role.clone(),
        charge_key: a.charge_key.clone(),
        name: a.name.clone(),
        state: a.state,
        cost_usd: a.cost_usd,
        context_pct: a.context_pct,
        depth,
        focus: focus.get(&a.session_id).cloned(),
    }
}

/// Steps up `parents` from `id` to the island's root; a cycle stops after
/// one lap.
fn depth(id: i64, parents: &HashMap<i64, Option<i64>>) -> u32 {
    let mut steps = 0;
    let mut at = id;
    while let Some(Some(parent)) = parents.get(&at) {
        steps += 1;
        if steps as usize > parents.len() {
            break;
        }
        at = *parent;
    }
    steps
}

fn wait(row: &NeedsYou, graphs: &[LiveGraph], focus: &HashMap<String, Focus>) -> Wait {
    let session = row.session_id.as_deref();
    let mut focus = session.and_then(|s| focus.get(s)).cloned();
    // `claude attach` takes a parked job's short id, not its session id.
    if let (Some(f), Some(bg)) = (focus.as_mut(), row.bg_id.as_ref()) {
        f.session_id.clone_from(bg);
    }
    let waiting = session.and_then(|s| {
        graphs
            .iter()
            .flat_map(|g| &g.agents)
            .find(|a| a.session_id == s && a.agent_id == row.agent_id)
    });
    let label = waiting
        .and_then(|a| a.role.clone().or_else(|| a.name.clone()))
        .or_else(|| focus.as_ref().map(|f| f.title.clone()))
        .or_else(|| row.session_id.clone())
        .unwrap_or_else(|| UNKNOWN.to_owned());
    Wait {
        label,
        waiting_for: row.waiting_for.clone(),
        since_ms: row.since_ms,
        focus,
    }
}
