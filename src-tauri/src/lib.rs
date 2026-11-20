pub mod agents;
mod cursor;
mod focus;
mod github;
mod obs;
mod placement;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use github::{Gh, GithubState, Refresher};
use obs::{ObsState, Timing, Watcher};
use placement::{MonitorInfo, Placement, choose_placement};
use strate_core::engine::Engine;
use strate_core::registry::resolve_claude;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize};

/// The Code engine's store, or why it did not start, and the engine itself,
/// taken and stopped on exit.
struct Code {
    reader: Result<Arc<agents::Reader>, String>,
    engine: Mutex<Option<Engine>>,
}

fn describe(monitor: &tauri::Monitor, primary: Option<&tauri::Monitor>) -> MonitorInfo {
    MonitorInfo {
        width: monitor.size().width,
        height: monitor.size().height,
        x: monitor.position().x,
        y: monitor.position().y,
        scale_factor: monitor.scale_factor(),
        is_primary: primary
            .is_some_and(|p| p.position() == monitor.position() && p.size() == monitor.size()),
    }
}

/// Place the main window and show it; returns the placement chosen.
fn place_main_window(app: &tauri::App) -> tauri::Result<Placement> {
    let window = app
        .get_webview_window("main")
        .expect("main window is declared in tauri.conf.json");
    let primary = window.primary_monitor()?;
    let monitors: Vec<MonitorInfo> = window
        .available_monitors()?
        .iter()
        .map(|m| describe(m, primary.as_ref()))
        .collect();
    let placement = choose_placement(&monitors);
    match placement {
        Placement::Fullscreen {
            x,
            y,
            width,
            height,
        } => {
            window.set_decorations(false)?;
            window.set_position(PhysicalPosition::new(x, y))?;
            window.set_size(PhysicalSize::new(width, height))?;
            window.set_fullscreen(true)?;
        }
        Placement::Windowed {
            x,
            y,
            width,
            height,
        } => {
            window.set_size(PhysicalSize::new(width, height))?;
            window.set_position(PhysicalPosition::new(x, y))?;
        }
        Placement::Default => {}
    }
    window.show()?;
    Ok(placement)
}

/// On the Edge, start putting the cursor back after each touch there. A hook
/// that cannot be installed leaves the cursor where Windows puts it.
fn start_cursor_restore(app: &tauri::App, placement: &Placement) {
    if let Some(edge) = cursor::edge_rect(placement)
        && let Ok(restorer) = cursor::Restorer::start(edge)
    {
        app.manage(restorer);
    }
}

/// Current OBS state, for a webview that mounts after the last `obs-state` event.
#[tauri::command]
fn obs_state(watcher: tauri::State<'_, Watcher>) -> ObsState {
    watcher.current()
}

/// Bring a session's terminal tab or window to the front, or open a terminal
/// attached to a background session. COM/UIA work runs on a blocking thread.
#[tauri::command]
async fn focus_terminal(
    pid: u32,
    session_id: String,
    title: String,
    kind: focus::SessionKind,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        focus::focus_terminal(pid, &session_id, &title, kind)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Current GitHub state, for a webview that mounts after the last `github-state` event.
#[tauri::command]
fn github_state(refresher: tauri::State<'_, Refresher>) -> GithubState {
    refresher.current()
}

/// Start the GitHub refresher: `gh` now, then every `REFRESH_INTERVAL`.
fn start_github_refresher(app: &tauri::App) {
    let handle = app.handle().clone();
    let gh = Gh::new(github::run_gh);
    let refresher = Refresher::spawn(
        move || gh.fetch(),
        move |state| {
            let _ = handle.emit("github-state", state);
        },
        github::REFRESH_INTERVAL,
    );
    app.manage(refresher);
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The Code setup's graph, needs-you and header, read from edge-deck's own
/// store on a blocking thread. An error when the engine did not start or
/// the store cannot be read.
#[tauri::command]
async fn code_snapshot(code: tauri::State<'_, Code>) -> Result<agents::Snapshot, String> {
    let reader = code.reader.clone()?;
    tauri::async_runtime::spawn_blocking(move || reader.read(now_ms()))
        .await
        .map_err(|e| e.to_string())?
}

/// Start strate-core's engine on the real Claude config dir into edge-deck's
/// own store; each store change emits `code-changed`.
fn start_code_engine(app: &tauri::App) -> Result<(PathBuf, Engine), String> {
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let home = app.path().home_dir().ok();
    let root = agents::config_root(std::env::var_os("CLAUDE_CONFIG_DIR"), home.clone())
        .ok_or("no home dir to find ~/.claude in, and CLAUDE_CONFIG_DIR is unset")?;
    let claude = resolve_claude(None, std::env::var_os("PATH"), home);
    let config = agents::engine_config(root, &data, claude);
    let db = config.db.clone();
    let handle = app.handle().clone();
    let engine = agents::start(config, move || {
        let _ = handle.emit(agents::CHANGED_EVENT, ());
    })?;
    Ok((db, engine))
}

/// Manage the Code engine, or its start error for `code_snapshot` to report.
fn manage_code_engine(app: &tauri::App) {
    let (db, engine) = match start_code_engine(app) {
        Ok((db, engine)) => (Ok(Arc::new(agents::Reader::new(db))), Some(engine)),
        Err(error) => (Err(error), None),
    };
    app.manage(Code {
        reader: db,
        engine: Mutex::new(engine),
    });
}

/// Start the OBS watcher; it re-reads OBS's own config file on every attempt.
fn start_obs_watcher(app: &tauri::App) {
    let handle = app.handle().clone();
    let watcher = Watcher::spawn(
        || obs::default_config_path().and_then(|path| obs::read_config(&path)),
        move |state| {
            let _ = handle.emit("obs-state", state);
        },
        Timing::default(),
    );
    app.manage(watcher);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            obs_state,
            github_state,
            focus_terminal,
            code_snapshot
        ])
        .setup(|app| {
            start_obs_watcher(app);
            start_github_refresher(app);
            manage_code_engine(app);
            let placement = place_main_window(app)?;
            start_cursor_restore(app, &placement);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building edge-deck")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(watcher) = app.try_state::<Watcher>() {
                    watcher.stop();
                }
                if let Some(refresher) = app.try_state::<Refresher>() {
                    refresher.stop();
                }
                if let Some(restorer) = app.try_state::<cursor::Restorer>() {
                    restorer.stop();
                }
                if let Some(code) = app.try_state::<Code>()
                    && let Some(engine) = code
                        .engine
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take()
                {
                    engine.stop();
                }
            }
        });
}
