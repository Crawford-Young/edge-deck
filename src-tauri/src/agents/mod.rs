//! The Code setup's data: strate-core's ingest engine, run into edge-deck's
//! own store, and the snapshot the webview reads from it.
//!
//! The engine thread owns the store writer and calls `on_change` (already
//! throttled to [`DEBOUNCE`](strate_core::engine::DEBOUNCE)) after each
//! commit; the host turns that into one [`CHANGED_EVENT`], and the webview
//! answers it with the `code_snapshot` command ([`read_snapshot`]).

mod snapshot;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use strate_core::discovery::resolve_config_dir;
use strate_core::engine::{Config, Engine};
use strate_core::registry::{PollOptions, cli_runner};

pub use snapshot::{
    Agent, Edge, Focus, Header, Island, Reader, Snapshot, Wait, read_snapshot, snapshot, wait_order,
};

/// How often the engine polls `claude agents --json --all`. Left at the
/// engine's default (1 s) until latency is measured on the Edge.
pub const POLL_EVERY: Duration = Duration::from_secs(1);

/// The event emitted, with no payload, each time the store changed.
pub const CHANGED_EVENT: &str = "code-changed";

/// edge-deck's own store file; never strate's.
const STORE_FILE: &str = "edge-deck-agents.db";

/// Where edge-deck's store lives: a file of its own under `app_data`.
pub fn store_path(app_data: &Path) -> PathBuf {
    app_data.join(STORE_FILE)
}

/// The Claude Code config dir, resolved as strate resolves it: a non-empty
/// `CLAUDE_CONFIG_DIR`, else `~/.claude`; `None` with neither.
pub fn config_root(claude_config_dir: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    resolve_config_dir(claude_config_dir, home)
}

/// The engine config: the config dir at `root`, the store at
/// [`store_path`], the registry on when a `claude` binary is given, polled
/// every [`POLL_EVERY`]. GitHub and hooks stay off; debounce and tail keep
/// the engine defaults.
pub fn engine_config(root: PathBuf, app_data: &Path, claude: Option<PathBuf>) -> Config {
    Config {
        registry: claude.map(cli_runner),
        poll: PollOptions {
            every: POLL_EVERY,
            ..PollOptions::default()
        },
        ..Config::new(root, store_path(app_data))
    }
}

/// Starts the engine; a store that cannot open or a source that cannot
/// start is an error message, never a panic.
pub fn start(config: Config, on_change: impl FnMut() + Send + 'static) -> Result<Engine, String> {
    Engine::start(config, on_change).map_err(|e| format!("the Code engine did not start: {e}"))
}
