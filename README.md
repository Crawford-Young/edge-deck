# edge-deck

A fullscreen touch app for a 2560x720 display. Built with Tauri v2, React, Vite and Tailwind, styled entirely with `@crawfordyoung/ui`.

Status: the window places itself on the 2560x720 display, the setup rail switches between Stream and Code, and the app follows OBS stream state. The Code setup shows live data: the header, "Needs you" and the "Agents" web come from the in-process agents engine, and the GitHub panel lists PRs and issues through `gh`; the Stream setup is a placeholder.

## Setup rail

A 112px rail at the left edge lists the setups (Stream, Code) as 96px touch targets, with a clock at the bottom. Tapping one switches setup, and the last tapped setup is restored on launch (kept in the webview's `localStorage`). When OBS starts streaming the app switches to Stream by itself, and when the stream ends it returns to the last tapped setup. A tap during a stream is respected until the stream state next changes. The rules are a pure reducer in `src/setups.ts`; `src/useSetups.ts` wires it to storage and the `obs-state` event.

## Code setup

`src/code/CodeSetup.tsx` lays out a 56px header (running time, session and daily cost, agent count, an "N need you" chip) above three panels: "Needs you" (580px, longest wait first, ages tick each second), "Agents" (fills the rest) and "GitHub" (640px). It is laid out to fit the 2448x720 main area; each panel body scrolls inside itself. `Panel` is the reusable labelled frame, and `useNow` is the one-second clock hook (each component that shows a ticking time calls it). A visually hidden "Code" `<h1>` names the page. Apart from the GitHub panel, which reads its own data (below), it is presentational: `App` passes it the `code_snapshot` document from `useCodeSnapshot` (`src/code/useCodeSnapshot.ts`), which subscribes to `code-changed` first and then reads, drops a read that a newer one has overtaken, and uses no timer. A failed read shows its message and keeps the last snapshot.

Each "Needs you" row that has a focus target is a 96px-tall button that calls `focusTerminal`; its keyboard focus ring uses library tokens. Shared types (`FocusTarget`, `SessionKind`, the snapshot shapes) live in `src/code/types.ts`; `src/api.ts` re-exports them.

## Agent web

The "Agents" panel (`src/code/AgentsPanel.tsx`) draws `src/code/graph/AgentWeb.tsx`: a React Flow canvas (`@xyflow/react`) of one island per live root session, each laid out by a deterministic d3-force simulation (`src/code/graph/layout.ts`, `d3-force`) and placed side by side with a gap between islands. Each node is the library's `AgentNodeRing` (`@crawfordyoung/ui` 0.34). A layout runs again only when membership, depth or edges change, so a state or cost update does not move nodes; a surviving node keeps its place and a new one starts next to its parent.

- Dense look: when the total agent count across islands is more than `DENSE_NODE_COUNT` (24), nodes shrink with depth (84px at the root, 12px less per step, never under 48px), only the root, hubs (an agent with at least 3 children) and agents that need you keep their label, and the panel count reads "N total · N working · N finished · N deep" instead of the live count.
- Touch rule: a node must be at least 96px on screen to be tapped. A tap on a smaller node centres and zooms to just past that size instead of acting; a tap on a tappable node calls `focusTerminal` with its focus target (nothing if it has none), and a failure shows "Could not bring that terminal forward".
- Each island sits on a cluster target; tapping it zooms to fit that island. The "Re-fit" button (library `touch` size) fits all agents.
- The rules (counts, sizes, tappable, hubs) are pure functions in `src/code/graph/rules.ts`.

## GitHub panel

`src/code/GitHubPanel.tsx` lists the signed-in user's open PRs in any repo, then the open issues labelled `in-progress` in repos they own. The title row counts them ("2 PRs · 1 issue"). Each PR row shows `owner/name#number`, its title, "Draft" for a draft, and the latest commit's check rollup: "Checks failing", "Checks passing" or "Checks pending", nothing when it has no checks. Each issue row shows `owner/name#number` and its title. Rows are not tappable yet.

A background thread (`src-tauri/src/github/`) runs the GitHub CLI, so `gh` must be installed and signed in. It reads the login once with `gh api user --jq .login` (validated as 1 to 39 letters, digits and `-`), then makes one `gh api graphql` request with two searches passed as GraphQL variables, never interpolated into the query or run through a shell: open PRs authored by that login, and open issues labelled `in-progress` in repos owned by it, both skipping archived repos, newest-updated first, up to 50 each. It runs with no console window, and a run that takes longer than 20 seconds is killed and counted as an error. It fetches at launch, then every `REFRESH_INTERVAL` (60 seconds), and emits a `github-state` event only when the result changes; the webview reads `{ status, prs, issues }` with the `github_state` command (`githubState()` and `onGithubState()` in `src/api.ts`, wrapped by `src/code/useGithub.ts`; subscribe first, then read).

`status` is "loading" until the first run returns, then "ok", or one of these, each with empty lists and its own message in the panel:

- "gh-missing": `gh` is neither on `PATH` nor at the default install location `%ProgramFiles%\GitHub CLI\gh.exe` ("GitHub CLI not found. Install gh to see PRs and issues.").
- "signed-out": `gh` exits with code 4 or asks for `gh auth login` ("Not signed in to GitHub. Run gh auth login.").
- "error": anything else, such as no network or an unreadable response ("Couldn't reach GitHub. Retrying.").

Every failure is retried on the next tick, so signing in or getting the network back recovers without a restart. Installing `gh` recovers without a restart only for the default install location (`%ProgramFiles%\GitHub CLI`): the app looks for `gh` on the `PATH` it started with, and falls back to that location; `gh` installed elsewhere is found after an app restart. The thread stops when the app exits, which can wait for a `gh` run in flight: on the first refresh that is about 2 x 20 seconds (the login run, then the search run), later about 20 seconds.

## Window placement

The window starts hidden and is placed before it is shown. If an attached display is exactly 2560x720 physical pixels (horizontal), the app opens borderless and fullscreen on it, wherever Windows positions that display; it is found by size only, never by index, name or position. If two displays match, the leftmost wins, then the topmost. With no such display it opens as a normal decorated window centred on the primary display. The choice is a pure function in `src-tauri/src/placement.rs`.

## OBS stream state

A background thread watches OBS over obs-websocket v5 on `127.0.0.1` so the shell can follow the stream. It takes the port and password from OBS's own settings file, `%APPDATA%\obs-studio\plugin_config\obs-websocket\config.json`, re-read before every connect attempt; edge-deck never stores, logs or emits the password. Turn the server on in OBS under Tools > WebSocket Server Settings. The webview reads `{ connection: "off" | "connecting" | "connected" | "auth-failed", streaming }` with the `obs_state` command and gets each change as an `obs-state` event (`obsState()` and `onObsState()` in `src/api.ts`; subscribe first, then read, so no change slips between). "off" means OBS is closed, its server is disabled or the file is missing; a dropped connection reads as not streaming. The watcher retries forever with a backoff capped at 10 seconds and stops when the app exits. The code is in `src-tauri/src/obs/`.

## Terminal focus

The `focus_terminal` command (`focusTerminal()` in `src/api.ts`) brings a Claude session's Windows Terminal to the front. It walks the session's process ancestry to the nearest `WindowsTerminal.exe`, lists that process's visible windows and their tabs through UI Automation, and then, in this order: selects the first tab whose name is the session's title, with at most a status glyph before it, and raises its window; raises all of that terminal's windows if no tab matches; for a background session with no terminal, opens a new window running `wt.exe -w new claude attach <id>`; otherwise it fails with "no terminal window found for this session". Session ids are validated first (1 to 128 ASCII letters, digits, `-` and `_`) and passed as an argument vector, never through a shell. The decisions are pure functions in `src-tauri/src/focus/plan.rs`; the Win32 side is `src-tauri/src/focus/windows.rs`.

## Agents engine

The Code setup's data comes from strate-core's ingest engine, which edge-deck runs in-process (`src-tauri/src/agents/`) into a store of its own, `<app_data_dir>/edge-deck-agents.db`; it never opens or writes strate's store. The engine reads the Claude config dir (`CLAUDE_CONFIG_DIR`, else `~/.claude`) and polls `claude agents --json --all` every second when a `claude` binary is found. After each store change (debounced by the engine) the host emits a `code-changed` event with no payload; the webview answers it with the `code_snapshot` command, which returns `{ graphs, needsYou, header }` read through one long-lived read-only connection to the store. A needs-you row for a parked background job focuses by its short background id, the `<id>` of `claude attach <id>`, not by its session id. If the engine cannot start, `code_snapshot` returns that error rather than the app failing.

strate-core is a git dependency in `src-tauri/Cargo.toml`, pinned by `rev`, so a strate-core change reaches edge-deck only by bumping that rev.

## Prerequisites

- Windows 11 with WebView2 (preinstalled on Windows 11)
- Rust stable, MSVC toolchain (`rust-toolchain.toml` pins the channel and components)
- Visual Studio Build Tools with the "Desktop development with C++" workload
- Bun 1.4.2+ (see `packageManager` in `package.json`); it is both package manager and runtime, no separate Node install needed

## Commands

```sh
bun install
bun run tauri dev       # run the app (Vite dev server on strict port 1421)
```

Gates (CI runs the same on `windows-latest`):

```sh
bun run typecheck
bun run lint
bun run test             # Vitest, 100% coverage thresholds
bun run build            # must run before cargo: Tauri embeds dist/
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bun run tauri build --debug --no-bundle   # proves the app compiles
```
