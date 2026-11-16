//! Pure functions over `gh` output: the GraphQL snapshot and failure classes.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use super::{Checks, GithubStatus, Issue, PullRequest};

/// The GraphQL response could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError;

#[derive(Deserialize)]
struct Response {
    data: Option<Data>,
    errors: Option<Value>,
}

#[derive(Deserialize)]
struct Data {
    prs: Search,
    issues: Search,
}

/// Search results are a union; nodes of the other type arrive as `{}`.
#[derive(Deserialize)]
struct Search {
    nodes: Vec<Option<Map<String, Value>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    name_with_owner: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrNode {
    number: u64,
    title: String,
    url: String,
    is_draft: bool,
    repository: Repository,
    commits: Commits,
}

#[derive(Deserialize)]
struct Commits {
    nodes: Vec<CommitNode>,
}

#[derive(Deserialize)]
struct CommitNode {
    commit: Commit,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commit {
    status_check_rollup: Option<Rollup>,
}

#[derive(Deserialize)]
struct Rollup {
    state: String,
}

#[derive(Deserialize)]
struct IssueNode {
    number: u64,
    title: String,
    url: String,
    repository: Repository,
}

fn checks(commits: &Commits) -> Checks {
    let state = commits
        .nodes
        .last()
        .and_then(|node| node.commit.status_check_rollup.as_ref())
        .map(|rollup| rollup.state.as_str());
    match state {
        Some("SUCCESS") => Checks::Passing,
        Some("FAILURE" | "ERROR") => Checks::Failing,
        Some("PENDING" | "EXPECTED") => Checks::Pending,
        _ => Checks::None,
    }
}

/// The non-empty nodes of `search`, in order, as `T`.
fn nodes<T: DeserializeOwned>(search: Search) -> Result<Vec<T>, ParseError> {
    search
        .nodes
        .into_iter()
        .flatten()
        .filter(|node| !node.is_empty())
        .map(|node| serde_json::from_value(Value::Object(node)).map_err(|_| ParseError))
        .collect()
}

/// Parse `{"data":{"prs":{"nodes":[..]},"issues":{"nodes":[..]}}}`.
pub fn parse_snapshot(json: &str) -> Result<(Vec<PullRequest>, Vec<Issue>), ParseError> {
    let response: Response = serde_json::from_str(json).map_err(|_| ParseError)?;
    if response.errors.is_some() {
        return Err(ParseError);
    }
    let data = response.data.ok_or(ParseError)?;
    let prs = nodes::<PrNode>(data.prs)?
        .into_iter()
        .map(|node| PullRequest {
            checks: checks(&node.commits),
            repo: node.repository.name_with_owner,
            number: node.number,
            title: node.title,
            url: node.url,
            draft: node.is_draft,
        })
        .collect();
    let issues = nodes::<IssueNode>(data.issues)?
        .into_iter()
        .map(|node| Issue {
            repo: node.repository.name_with_owner,
            number: node.number,
            title: node.title,
            url: node.url,
        })
        .collect();
    Ok((prs, issues))
}

/// Map a failed `gh` run to the status the panel shows.
pub fn classify_failure(
    spawn_not_found: bool,
    exit_code: Option<i32>,
    stderr: &str,
) -> GithubStatus {
    if spawn_not_found {
        GithubStatus::GhMissing
    } else if exit_code == Some(4) || stderr.contains("gh auth login") {
        GithubStatus::SignedOut
    } else {
        GithubStatus::Error
    }
}

#[cfg(test)]
mod tests {
    use super::super::Checks;
    use super::*;

    fn pr_node(number: u64, repo: &str, rollup: &str) -> String {
        format!(
            r#"{{"number":{number},"title":"PR {number}","url":"https://example.test/{repo}/pull/{number}","isDraft":false,"repository":{{"nameWithOwner":"{repo}"}},"commits":{{"nodes":[{{"commit":{{"statusCheckRollup":{rollup}}}}}]}}}}"#
        )
    }

    fn rollup(state: &str) -> String {
        format!(r#"{{"state":"{state}"}}"#)
    }

    fn snapshot(prs: &[String], issues: &[String]) -> String {
        format!(
            r#"{{"data":{{"prs":{{"nodes":[{}]}},"issues":{{"nodes":[{}]}}}}}}"#,
            prs.join(","),
            issues.join(",")
        )
    }

    fn issue_node(number: u64, repo: &str) -> String {
        format!(
            r#"{{"number":{number},"title":"Issue {number}","url":"https://example.test/{repo}/issues/{number}","repository":{{"nameWithOwner":"{repo}"}}}}"#
        )
    }

    fn checks_of(state: &str) -> Checks {
        let json = snapshot(&[pr_node(1, "octo/widgets", &rollup(state))], &[]);
        parse_snapshot(&json).unwrap().0[0].checks
    }

    #[test]
    fn maps_every_field_of_a_pr_and_an_issue() {
        let pr = r#"{"number":42,"title":"Add a gizmo","url":"https://example.test/octo/widgets/pull/42","isDraft":true,"repository":{"nameWithOwner":"octo/widgets"},"commits":{"nodes":[{"commit":{"statusCheckRollup":{"state":"SUCCESS"}}}]}}"#;
        let json = snapshot(&[pr.to_string()], &[issue_node(9, "octo/gadgets")]);
        let (prs, issues) = parse_snapshot(&json).unwrap();
        assert_eq!(
            prs,
            vec![PullRequest {
                repo: "octo/widgets".into(),
                number: 42,
                title: "Add a gizmo".into(),
                url: "https://example.test/octo/widgets/pull/42".into(),
                draft: true,
                checks: Checks::Passing,
            }]
        );
        assert_eq!(
            issues,
            vec![Issue {
                repo: "octo/gadgets".into(),
                number: 9,
                title: "Issue 9".into(),
                url: "https://example.test/octo/gadgets/issues/9".into(),
            }]
        );
    }

    #[test]
    fn rollup_states_map_to_checks() {
        assert_eq!(checks_of("SUCCESS"), Checks::Passing);
        assert_eq!(checks_of("FAILURE"), Checks::Failing);
        assert_eq!(checks_of("ERROR"), Checks::Failing);
        assert_eq!(checks_of("PENDING"), Checks::Pending);
        assert_eq!(checks_of("EXPECTED"), Checks::Pending);
    }

    #[test]
    fn null_rollup_and_empty_commits_are_none() {
        let json = snapshot(&[pr_node(1, "octo/widgets", "null")], &[]);
        assert_eq!(parse_snapshot(&json).unwrap().0[0].checks, Checks::None);
        let no_commits = r#"{"number":2,"title":"t","url":"u","isDraft":false,"repository":{"nameWithOwner":"octo/widgets"},"commits":{"nodes":[]}}"#;
        let json = snapshot(&[no_commits.to_string()], &[]);
        assert_eq!(parse_snapshot(&json).unwrap().0[0].checks, Checks::None);
    }

    #[test]
    fn skips_empty_union_nodes_and_keeps_order() {
        let json = snapshot(
            &[
                pr_node(3, "octo/widgets", &rollup("SUCCESS")),
                "{}".into(),
                pr_node(1, "octo/widgets", &rollup("FAILURE")),
            ],
            &[
                "{}".into(),
                issue_node(8, "octo/widgets"),
                issue_node(2, "octo/gadgets"),
            ],
        );
        let (prs, issues) = parse_snapshot(&json).unwrap();
        assert_eq!(prs.iter().map(|p| p.number).collect::<Vec<_>>(), [3, 1]);
        assert_eq!(issues.iter().map(|i| i.number).collect::<Vec<_>>(), [8, 2]);
    }

    #[test]
    fn empty_result_is_ok_and_empty() {
        let (prs, issues) = parse_snapshot(&snapshot(&[], &[])).unwrap();
        assert!(prs.is_empty());
        assert!(issues.is_empty());
    }

    #[test]
    fn a_top_level_errors_array_is_an_error() {
        let json =
            r#"{"errors":[{"message":"boom"}],"data":{"prs":{"nodes":[]},"issues":{"nodes":[]}}}"#;
        assert_eq!(parse_snapshot(json), Err(ParseError));
    }

    #[test]
    fn missing_data_is_an_error() {
        assert_eq!(parse_snapshot("{}"), Err(ParseError));
        assert_eq!(parse_snapshot(r#"{"data":null}"#), Err(ParseError));
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert_eq!(parse_snapshot("not json"), Err(ParseError));
        assert_eq!(parse_snapshot(""), Err(ParseError));
        assert_eq!(parse_snapshot(r#"{"data":{"prs":"#), Err(ParseError));
    }

    #[test]
    fn spawn_not_found_is_gh_missing_whatever_else_is_set() {
        assert_eq!(classify_failure(true, None, ""), GithubStatus::GhMissing);
        assert_eq!(
            classify_failure(true, Some(4), "gh auth login"),
            GithubStatus::GhMissing
        );
    }

    #[test]
    fn exit_code_four_is_signed_out() {
        assert_eq!(
            classify_failure(false, Some(4), ""),
            GithubStatus::SignedOut
        );
    }

    #[test]
    fn stderr_mentioning_gh_auth_login_is_signed_out() {
        let stderr = "To get started with GitHub CLI, please run:  gh auth login";
        assert_eq!(
            classify_failure(false, Some(1), stderr),
            GithubStatus::SignedOut
        );
    }

    #[test]
    fn anything_else_is_an_error() {
        assert_eq!(classify_failure(false, Some(1), ""), GithubStatus::Error);
        assert_eq!(
            classify_failure(false, Some(1), "HTTP 502: Bad Gateway"),
            GithubStatus::Error
        );
        assert_eq!(classify_failure(false, None, ""), GithubStatus::Error);
    }
}
