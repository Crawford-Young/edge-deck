mod placement;

use placement::{MonitorInfo, Placement, choose_placement};
use tauri::{Manager, PhysicalPosition, PhysicalSize};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| Ok(place_main_window(app)?))
        .run(tauri::generate_context!())
        .expect("error while running edge-deck");
}
