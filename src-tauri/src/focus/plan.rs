use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Interactive,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent_pid: u32,
    pub exe: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub hwnd: isize,
    pub pid: u32,
    pub tabs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusPlan {
    SelectTab { hwnd: isize, tab: usize },
    RaiseWindows { hwnds: Vec<isize> },
    OpenAttach { session_id: String },
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidSessionId;

impl fmt::Display for InvalidSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid session id")
    }
}

const TERMINAL_EXE: &str = "WindowsTerminal.exe";
const MAX_SESSION_ID_LEN: usize = 128;

/// The nearest Windows Terminal process at or above `pid`, walking parent links.
///
/// The walk stops at pid 0 and after one step per process in `procs`, so a
/// parent loop cannot spin.
pub fn terminal_ancestor(pid: u32, procs: &[ProcInfo]) -> Option<u32> {
    let by_pid: HashMap<u32, &ProcInfo> = procs.iter().map(|p| (p.pid, p)).collect();
    let mut current = pid;
    for _ in 0..procs.len() {
        if current == 0 {
            return None;
        }
        let proc = by_pid.get(&current)?;
        if proc.exe.eq_ignore_ascii_case(TERMINAL_EXE) {
            return Some(proc.pid);
        }
        current = proc.parent_pid;
    }
    None
}

/// Decide how to bring the session's terminal to the front.
pub fn plan_focus(
    pid: u32,
    session_id: &str,
    title: &str,
    kind: SessionKind,
    procs: &[ProcInfo],
    windows: &[WindowInfo],
) -> FocusPlan {
    let own: Vec<&WindowInfo> = match terminal_ancestor(pid, procs) {
        Some(terminal) => windows.iter().filter(|w| w.pid == terminal).collect(),
        None => Vec::new(),
    };
    if own.is_empty() {
        return match kind {
            SessionKind::Background => FocusPlan::OpenAttach {
                session_id: session_id.to_string(),
            },
            SessionKind::Interactive => FocusPlan::NotFound,
        };
    }
    let title = title.trim();
    if !title.is_empty() {
        for window in &own {
            // The tab is the title, with at most a status glyph before it.
            let matches = |t: &String| {
                t.strip_suffix(title)
                    .is_some_and(|rest| rest.trim().chars().count() <= 1)
            };
            if let Some(tab) = window.tabs.iter().position(matches) {
                return FocusPlan::SelectTab {
                    hwnd: window.hwnd,
                    tab,
                };
            }
        }
    }
    FocusPlan::RaiseWindows {
        hwnds: own.iter().map(|w| w.hwnd).collect(),
    }
}

/// Accept 1..=128 ASCII letters, digits, `-` and `_`; nothing a shell or
/// Windows Terminal (which splits commands on `;`) could interpret.
pub fn validate_session_id(id: &str) -> Result<&str, InvalidSessionId> {
    let allowed = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
    if (1..=MAX_SESSION_ID_LEN).contains(&id.len()) && id.bytes().all(allowed) {
        Ok(id)
    } else {
        Err(InvalidSessionId)
    }
}

/// The program and argument vector that open a new terminal window attached
/// to a background session. Pass only an id that `validate_session_id` accepted.
pub fn attach_command(session_id: &str) -> (&'static str, Vec<String>) {
    let args = ["-w", "new", "claude", "attach", session_id];
    ("wt.exe", args.iter().map(|a| a.to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent_pid: u32, exe: &str) -> ProcInfo {
        ProcInfo {
            pid,
            parent_pid,
            exe: exe.to_string(),
        }
    }

    fn w(hwnd: isize, pid: u32, tabs: &[&str]) -> WindowInfo {
        WindowInfo {
            hwnd,
            pid,
            tabs: tabs.iter().map(|t| t.to_string()).collect(),
        }
    }

    // claude.exe(300) <- powershell.exe(200) <- WindowsTerminal.exe(100) <- explorer.exe(50)
    fn tree() -> Vec<ProcInfo> {
        vec![
            p(50, 1, "explorer.exe"),
            p(100, 50, "WindowsTerminal.exe"),
            p(200, 100, "powershell.exe"),
            p(300, 200, "claude.exe"),
        ]
    }

    fn two_windows() -> Vec<WindowInfo> {
        vec![
            w(1001, 100, &["\u{2733} fix the parser", "shell"]),
            w(1002, 100, &["notes", "\u{2733} write the docs"]),
        ]
    }

    // ---- terminal_ancestor ----

    #[test]
    fn ancestor_found_through_the_parent_chain() {
        assert_eq!(terminal_ancestor(300, &tree()), Some(100));
    }

    #[test]
    fn ancestor_walk_is_inclusive_of_the_start_pid() {
        assert_eq!(terminal_ancestor(100, &tree()), Some(100));
    }

    #[test]
    fn ancestor_exe_match_ignores_case() {
        let procs = vec![p(10, 1, "windowsterminal.EXE"), p(20, 10, "claude.exe")];
        assert_eq!(terminal_ancestor(20, &procs), Some(10));
    }

    #[test]
    fn ancestor_none_when_chain_has_no_terminal() {
        let procs = vec![p(10, 1, "explorer.exe"), p(20, 10, "claude.exe")];
        assert_eq!(terminal_ancestor(20, &procs), None);
    }

    #[test]
    fn ancestor_none_for_missing_pid() {
        assert_eq!(terminal_ancestor(999, &tree()), None);
    }

    #[test]
    fn ancestor_none_for_empty_table() {
        assert_eq!(terminal_ancestor(300, &[]), None);
    }

    #[test]
    fn ancestor_walk_survives_a_two_node_cycle() {
        let procs = vec![p(10, 20, "a.exe"), p(20, 10, "b.exe")];
        assert_eq!(terminal_ancestor(10, &procs), None);
    }

    #[test]
    fn ancestor_walk_survives_a_self_parent() {
        let procs = vec![p(10, 10, "a.exe")];
        assert_eq!(terminal_ancestor(10, &procs), None);
    }

    #[test]
    fn ancestor_walk_stops_at_parent_pid_zero() {
        let procs = vec![p(10, 0, "claude.exe"), p(0, 10, "WindowsTerminal.exe")];
        assert_eq!(terminal_ancestor(10, &procs), None);
    }

    #[test]
    fn ancestor_walk_is_depth_safe_on_a_very_long_chain() {
        let mut procs: Vec<ProcInfo> = (2..5000u32).map(|i| p(i, i + 1, "x.exe")).collect();
        procs.push(p(5000, 0, "x.exe"));
        // Must terminate; the terminal is absent, so there is no ancestor.
        assert_eq!(terminal_ancestor(2, &procs), None);
    }

    #[test]
    fn ancestor_picks_the_nearest_terminal() {
        let procs = vec![
            p(1, 0, "WindowsTerminal.exe"),
            p(2, 1, "WindowsTerminal.exe"),
            p(3, 2, "claude.exe"),
        ];
        assert_eq!(terminal_ancestor(3, &procs), Some(2));
    }

    // ---- plan_focus ----

    #[test]
    fn selects_the_tab_whose_name_ends_with_the_title() {
        let plan = plan_focus(
            300,
            "sess-1",
            "fix the parser",
            SessionKind::Interactive,
            &tree(),
            &two_windows(),
        );
        assert_eq!(plan, FocusPlan::SelectTab { hwnd: 1001, tab: 0 });
    }

    #[test]
    fn selects_a_tab_in_the_second_window() {
        let plan = plan_focus(
            300,
            "sess-1",
            "write the docs",
            SessionKind::Interactive,
            &tree(),
            &two_windows(),
        );
        assert_eq!(plan, FocusPlan::SelectTab { hwnd: 1002, tab: 1 });
    }

    #[test]
    fn same_title_in_two_tabs_picks_first_by_window_then_tab_order() {
        let windows = vec![
            w(1, 100, &["shell", "\u{2733} dup"]),
            w(2, 100, &["\u{2733} dup"]),
        ];
        let plan = plan_focus(300, "s", "dup", SessionKind::Interactive, &tree(), &windows);
        assert_eq!(plan, FocusPlan::SelectTab { hwnd: 1, tab: 1 });
    }

    #[test]
    fn title_is_trimmed_before_matching() {
        let plan = plan_focus(
            300,
            "s",
            "  fix the parser \n",
            SessionKind::Interactive,
            &tree(),
            &two_windows(),
        );
        assert_eq!(plan, FocusPlan::SelectTab { hwnd: 1001, tab: 0 });
    }

    #[test]
    fn title_must_match_at_the_end_not_the_start_or_middle() {
        let windows = vec![w(1, 100, &["fix the parser later", "x fix the parser y"])];
        let plan = plan_focus(
            300,
            "s",
            "fix the parser",
            SessionKind::Interactive,
            &tree(),
            &windows,
        );
        assert_eq!(plan, FocusPlan::RaiseWindows { hwnds: vec![1] });
    }

    #[test]
    fn a_tab_with_a_longer_prefix_than_one_glyph_does_not_match() {
        let windows = vec![w(
            1,
            100,
            &["refix the parser", "\u{2733} please fix the parser"],
        )];
        let plan = plan_focus(
            300,
            "s",
            "fix the parser",
            SessionKind::Interactive,
            &tree(),
            &windows,
        );
        assert_eq!(plan, FocusPlan::RaiseWindows { hwnds: vec![1] });
    }

    #[test]
    fn a_bare_title_tab_and_a_glyph_prefixed_tab_both_match() {
        let bare = vec![w(1, 100, &["shell", "fix the parser"])];
        let plan = plan_focus(
            300,
            "s",
            "fix the parser",
            SessionKind::Interactive,
            &tree(),
            &bare,
        );
        assert_eq!(plan, FocusPlan::SelectTab { hwnd: 1, tab: 1 });
        let glyph = vec![w(1, 100, &["shell", "\u{2733} fix the parser"])];
        let plan = plan_focus(
            300,
            "s",
            "fix the parser",
            SessionKind::Interactive,
            &tree(),
            &glyph,
        );
        assert_eq!(plan, FocusPlan::SelectTab { hwnd: 1, tab: 1 });
    }

    #[test]
    fn only_windows_of_the_ancestor_process_are_considered() {
        let windows = vec![
            w(7, 555, &["\u{2733} fix the parser"]),
            w(8, 100, &["shell"]),
        ];
        let plan = plan_focus(
            300,
            "s",
            "fix the parser",
            SessionKind::Interactive,
            &tree(),
            &windows,
        );
        assert_eq!(plan, FocusPlan::RaiseWindows { hwnds: vec![8] });
    }

    #[test]
    fn no_tab_match_raises_all_of_the_terminals_windows_in_order() {
        let plan = plan_focus(
            300,
            "s",
            "unrelated title",
            SessionKind::Interactive,
            &tree(),
            &two_windows(),
        );
        assert_eq!(
            plan,
            FocusPlan::RaiseWindows {
                hwnds: vec![1001, 1002]
            }
        );
    }

    #[test]
    fn empty_title_matches_nothing_and_raises_windows() {
        let plan = plan_focus(
            300,
            "s",
            "",
            SessionKind::Interactive,
            &tree(),
            &two_windows(),
        );
        assert_eq!(
            plan,
            FocusPlan::RaiseWindows {
                hwnds: vec![1001, 1002]
            }
        );
    }

    #[test]
    fn whitespace_only_title_matches_nothing() {
        let plan = plan_focus(
            300,
            "s",
            "   ",
            SessionKind::Interactive,
            &tree(),
            &two_windows(),
        );
        assert_eq!(
            plan,
            FocusPlan::RaiseWindows {
                hwnds: vec![1001, 1002]
            }
        );
    }

    #[test]
    fn interactive_without_a_terminal_ancestor_is_not_found() {
        let procs = vec![p(10, 1, "explorer.exe"), p(20, 10, "claude.exe")];
        let plan = plan_focus(
            20,
            "s",
            "t",
            SessionKind::Interactive,
            &procs,
            &two_windows(),
        );
        assert_eq!(plan, FocusPlan::NotFound);
    }

    #[test]
    fn background_without_a_terminal_ancestor_opens_attach() {
        let procs = vec![p(10, 1, "explorer.exe"), p(20, 10, "claude.exe")];
        let plan = plan_focus(
            20,
            "sess-9",
            "t",
            SessionKind::Background,
            &procs,
            &two_windows(),
        );
        assert_eq!(
            plan,
            FocusPlan::OpenAttach {
                session_id: "sess-9".to_string()
            }
        );
    }

    #[test]
    fn missing_pid_follows_the_no_ancestor_rule() {
        let interactive = plan_focus(
            999,
            "s",
            "t",
            SessionKind::Interactive,
            &tree(),
            &two_windows(),
        );
        let background = plan_focus(
            999,
            "s",
            "t",
            SessionKind::Background,
            &tree(),
            &two_windows(),
        );
        assert_eq!(interactive, FocusPlan::NotFound);
        assert_eq!(
            background,
            FocusPlan::OpenAttach {
                session_id: "s".to_string()
            }
        );
    }

    #[test]
    fn ancestor_with_no_windows_falls_through_by_kind() {
        let interactive = plan_focus(300, "s", "t", SessionKind::Interactive, &tree(), &[]);
        let background = plan_focus(300, "s", "t", SessionKind::Background, &tree(), &[]);
        assert_eq!(interactive, FocusPlan::NotFound);
        assert_eq!(
            background,
            FocusPlan::OpenAttach {
                session_id: "s".to_string()
            }
        );
    }

    #[test]
    fn background_with_a_matching_tab_still_selects_it() {
        let plan = plan_focus(
            300,
            "s",
            "fix the parser",
            SessionKind::Background,
            &tree(),
            &two_windows(),
        );
        assert_eq!(plan, FocusPlan::SelectTab { hwnd: 1001, tab: 0 });
    }

    #[test]
    fn background_with_windows_but_no_match_raises_rather_than_attaches() {
        let plan = plan_focus(
            300,
            "s",
            "nope",
            SessionKind::Background,
            &tree(),
            &two_windows(),
        );
        assert_eq!(
            plan,
            FocusPlan::RaiseWindows {
                hwnds: vec![1001, 1002]
            }
        );
    }

    #[test]
    fn session_kind_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&SessionKind::Interactive).unwrap(),
            "\"interactive\""
        );
        assert_eq!(
            serde_json::from_str::<SessionKind>("\"background\"").unwrap(),
            SessionKind::Background
        );
    }

    // ---- validate_session_id ----

    #[test]
    fn accepts_letters_digits_dash_underscore() {
        assert_eq!(validate_session_id("abc-DEF_123"), Ok("abc-DEF_123"));
    }

    #[test]
    fn accepts_a_uuid_shaped_id() {
        let id = "0b9f3c1e-7a52-4d0e-9a41-123456789abc";
        assert_eq!(validate_session_id(id), Ok(id));
    }

    #[test]
    fn accepts_length_bounds_1_and_128() {
        assert!(validate_session_id("a").is_ok());
        assert!(validate_session_id(&"a".repeat(128)).is_ok());
    }

    #[test]
    fn rejects_empty_and_over_long() {
        assert!(validate_session_id("").is_err());
        assert!(validate_session_id(&"a".repeat(129)).is_err());
    }

    #[test]
    fn rejects_shell_and_wt_metacharacters() {
        for bad in [
            "a b", "a;b", "a\"b", "a'b", "a&b", "a|b", "a`b", "a\nb", "a\tb", "a/b", "a\\b", "a.b",
            "a$b", "a%b", "a<b", "a>b", "a(b", "a^b",
        ] {
            assert!(validate_session_id(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn rejects_non_ascii_letters() {
        assert!(validate_session_id("caf\u{e9}").is_err());
        assert!(validate_session_id("\u{2733}").is_err());
    }

    // ---- attach_command ----

    #[test]
    fn attach_command_is_an_argument_vector_for_wt() {
        let (program, args) = attach_command("sess-1_x");
        assert_eq!(program, "wt.exe");
        assert_eq!(args, vec!["-w", "new", "claude", "attach", "sess-1_x"]);
    }
}
