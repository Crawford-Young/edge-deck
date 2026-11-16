//! Bring a Claude session's Windows Terminal tab or window to the front.
//!
//! Every decision lives in the pure `plan` module; this module wires it to the
//! desktop through the `Desktop` trait, which `windows.rs` implements with
//! Win32 and UI Automation.

// The shell is Windows-only; elsewhere only the tests use the dispatch.
#![cfg_attr(not(windows), allow(dead_code))]

mod plan;
#[cfg(windows)]
mod windows;

pub use plan::SessionKind;
use plan::{
    FocusPlan, ProcInfo, WindowInfo, attach_command, plan_focus, terminal_ancestor,
    validate_session_id,
};

pub const NOT_FOUND: &str = "no terminal window found for this session";

/// Focus the session's terminal on the real desktop. Blocking: call it off the
/// main thread.
#[cfg(windows)]
pub fn focus_terminal(
    pid: u32,
    session_id: &str,
    title: &str,
    kind: SessionKind,
) -> Result<(), String> {
    let desktop = windows::Win32Desktop::new()?;
    run(&desktop, pid, session_id, title, kind)
}

#[cfg(not(windows))]
pub fn focus_terminal(
    _pid: u32,
    _session_id: &str,
    _title: &str,
    _kind: SessionKind,
) -> Result<(), String> {
    Err("terminal focus needs Windows".to_string())
}

/// The desktop operations a focus request needs.
pub(crate) trait Desktop {
    fn processes(&self) -> Result<Vec<ProcInfo>, String>;
    /// Top-level windows of `pid`, each with its tab names in tab-strip order.
    fn windows_of(&self, pid: u32) -> Result<Vec<WindowInfo>, String>;
    fn select_tab(&self, hwnd: isize, tab: usize) -> Result<(), String>;
    fn raise_windows(&self, hwnds: &[isize]) -> Result<(), String>;
    fn spawn(&self, program: &str, args: &[String]) -> Result<(), String>;
    /// The claude executable to run: its full path when installed natively,
    /// else `claude` for a PATH lookup.
    fn claude_program(&self) -> String;
}

/// Validate the id, read the desktop, plan, and act on the plan.
pub(crate) fn run(
    desktop: &impl Desktop,
    pid: u32,
    session_id: &str,
    title: &str,
    kind: SessionKind,
) -> Result<(), String> {
    let session_id = validate_session_id(session_id).map_err(|e| e.to_string())?;
    let procs = desktop.processes()?;
    let windows = match terminal_ancestor(pid, &procs) {
        Some(terminal) => desktop.windows_of(terminal)?,
        None => Vec::new(),
    };
    match plan_focus(pid, session_id, title, kind, &procs, &windows) {
        FocusPlan::SelectTab { hwnd, tab } => desktop.select_tab(hwnd, tab),
        FocusPlan::RaiseWindows { hwnds } => desktop.raise_windows(&hwnds),
        FocusPlan::OpenAttach { session_id } => {
            let (program, args) = attach_command(&desktop.claude_program(), &session_id);
            desktop.spawn(program, &args)
        }
        FocusPlan::NotFound => Err(NOT_FOUND.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        procs: Vec<ProcInfo>,
        windows: Vec<WindowInfo>,
        calls: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new(procs: Vec<ProcInfo>, windows: Vec<WindowInfo>) -> Self {
            Fake {
                procs,
                windows,
                calls: RefCell::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }

        fn log(&self, call: String) {
            self.calls.borrow_mut().push(call);
        }
    }

    impl Desktop for Fake {
        fn processes(&self) -> Result<Vec<ProcInfo>, String> {
            self.log("processes".into());
            Ok(self.procs.clone())
        }
        fn windows_of(&self, pid: u32) -> Result<Vec<WindowInfo>, String> {
            self.log(format!("windows_of {pid}"));
            Ok(self
                .windows
                .iter()
                .filter(|w| w.pid == pid)
                .cloned()
                .collect())
        }
        fn select_tab(&self, hwnd: isize, tab: usize) -> Result<(), String> {
            self.log(format!("select_tab {hwnd} {tab}"));
            Ok(())
        }
        fn raise_windows(&self, hwnds: &[isize]) -> Result<(), String> {
            self.log(format!("raise_windows {hwnds:?}"));
            Ok(())
        }
        fn spawn(&self, program: &str, args: &[String]) -> Result<(), String> {
            self.log(format!("spawn {program} {}", args.join(" ")));
            Ok(())
        }
        fn claude_program(&self) -> String {
            r"C:\Users\someone\.local\bin\claude.exe".to_string()
        }
    }

    fn p(pid: u32, parent_pid: u32, exe: &str) -> ProcInfo {
        ProcInfo {
            pid,
            parent_pid,
            exe: exe.to_string(),
        }
    }

    // claude.exe(30) <- powershell.exe(20) <- WindowsTerminal.exe(10); claude.exe(40) has no terminal.
    fn procs() -> Vec<ProcInfo> {
        vec![
            p(10, 1, "WindowsTerminal.exe"),
            p(20, 10, "powershell.exe"),
            p(30, 20, "claude.exe"),
            p(40, 1, "claude.exe"),
        ]
    }

    fn windows() -> Vec<WindowInfo> {
        vec![
            WindowInfo {
                hwnd: 501,
                pid: 10,
                tabs: vec!["shell".into(), "\u{2733} fix the parser".into()],
            },
            WindowInfo {
                hwnd: 502,
                pid: 10,
                tabs: vec!["notes".into()],
            },
        ]
    }

    #[test]
    fn selects_the_matching_tab() {
        let fake = Fake::new(procs(), windows());
        let result = run(&fake, 30, "s-1", "fix the parser", SessionKind::Interactive);
        assert_eq!(result, Ok(()));
        assert_eq!(
            fake.calls(),
            ["processes", "windows_of 10", "select_tab 501 1"]
        );
    }

    #[test]
    fn raises_the_terminals_windows_when_no_tab_matches() {
        let fake = Fake::new(procs(), windows());
        let result = run(&fake, 30, "s-1", "other", SessionKind::Interactive);
        assert_eq!(result, Ok(()));
        assert_eq!(
            fake.calls(),
            ["processes", "windows_of 10", "raise_windows [501, 502]"]
        );
    }

    #[test]
    fn opens_an_attach_terminal_for_a_background_session_without_one() {
        let fake = Fake::new(procs(), windows());
        let result = run(&fake, 40, "bg_7", "t", SessionKind::Background);
        assert_eq!(result, Ok(()));
        assert_eq!(
            fake.calls(),
            [
                "processes",
                r"spawn wt.exe -w new C:\Users\someone\.local\bin\claude.exe attach bg_7"
            ]
        );
    }

    #[test]
    fn interactive_session_without_a_terminal_is_an_error() {
        let fake = Fake::new(procs(), windows());
        let result = run(&fake, 40, "s-1", "t", SessionKind::Interactive);
        assert_eq!(result, Err(NOT_FOUND.to_string()));
        assert_eq!(fake.calls(), ["processes"]);
    }

    #[test]
    fn invalid_id_is_rejected_before_touching_the_desktop() {
        let fake = Fake::new(procs(), windows());
        let result = run(&fake, 40, "x; calc", "t", SessionKind::Background);
        assert_eq!(result, Err("invalid session id".to_string()));
        assert!(fake.calls().is_empty());
    }
}
