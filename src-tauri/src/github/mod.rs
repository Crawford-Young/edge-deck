//! Open PRs and in-progress issues from the GitHub CLI.

mod gh;
mod parse;
mod refresher;

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::Serialize;

pub use gh::{Gh, run_gh};
pub use parse::{classify_failure, parse_snapshot};
pub use refresher::Refresher;

/// Lock, recovering the data if a holder panicked: the values stay valid.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How often the refresher asks `gh` again.
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GithubStatus {
    Loading,
    Ok,
    GhMissing,
    SignedOut,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Checks {
    Passing,
    Failing,
    Pending,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub draft: bool,
    pub checks: Checks,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubState {
    pub status: GithubStatus,
    pub prs: Vec<PullRequest>,
    pub issues: Vec<Issue>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_interval_is_sixty_seconds() {
        assert_eq!(REFRESH_INTERVAL, Duration::from_secs(60));
    }

    #[test]
    fn state_serializes_camel_case_with_kebab_status_and_lowercase_checks() {
        let state = GithubState {
            status: GithubStatus::GhMissing,
            prs: vec![PullRequest {
                repo: "octo/widgets".into(),
                number: 7,
                title: "Add a gizmo".into(),
                url: "https://example.test/octo/widgets/pull/7".into(),
                draft: true,
                checks: Checks::None,
            }],
            issues: vec![Issue {
                repo: "octo/widgets".into(),
                number: 9,
                title: "Gizmo is slow".into(),
                url: "https://example.test/octo/widgets/issues/9".into(),
            }],
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["status"], "gh-missing");
        assert_eq!(json["prs"][0]["checks"], "none");
        assert_eq!(json["prs"][0]["draft"], true);
        assert_eq!(json["prs"][0]["repo"], "octo/widgets");
        assert_eq!(json["issues"][0]["number"], 9);
        for (status, text) in [
            (GithubStatus::Loading, "loading"),
            (GithubStatus::Ok, "ok"),
            (GithubStatus::SignedOut, "signed-out"),
            (GithubStatus::Error, "error"),
        ] {
            assert_eq!(serde_json::to_value(status).unwrap(), text);
        }
        for (checks, text) in [
            (Checks::Passing, "passing"),
            (Checks::Failing, "failing"),
            (Checks::Pending, "pending"),
        ] {
            assert_eq!(serde_json::to_value(checks).unwrap(), text);
        }
    }
}
