mod focus;
mod github;
mod obs;
mod placement;

use github::{Gh, GithubState, Refresher};
use obs::{ObsState, Timing, Watcher};
use placement::{MonitorInfo, Placement, choose_placement};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize};

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

fn place_main_window(app: &tauri::App) -> tauri::Result<()> {
    let window = app
        .get_webview_window("main")
        .expect("main window is declared in tauri.conf.json");
    let primary = window.primary_monitor()?;
    let monitors: Vec<MonitorInfo> = window
        .available_monitors()?
        .iter()
        .map(|m| describe(m, primary.as_ref()))
        .collect();
    match choose_placement(&monitors) {
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
    window.show()
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
            focus_terminal
        ])
        .setup(|app| {
            start_obs_watcher(app);
            start_github_refresher(app);
            Ok(place_main_window(app)?)
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
            }
        });
}
