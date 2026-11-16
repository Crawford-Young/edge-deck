//! Running `gh`: the login once, then one GraphQL search per refresh.

use std::io::{ErrorKind, Read};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::refresher::Fetched;
use super::{GithubStatus, classify_failure, lock, parse_snapshot};

/// Both searches in one request. The search strings are variables, so a login
/// is never interpolated into the query text.
pub const QUERY: &str = "query($prQuery:String!,$issueQuery:String!){prs:search(query:$prQuery,type:ISSUE,first:50){nodes{...on PullRequest{number title url isDraft repository{nameWithOwner} commits(last:1){nodes{commit{statusCheckRollup{state}}}}}}} issues:search(query:$issueQuery,type:ISSUE,first:50){nodes{...on Issue{number title url repository{nameWithOwner}}}}}";

/// The longest a single `gh` run may take before it is killed and counted as an
/// error, so a hung network call can't stall the refresher forever. App exit
/// joins the worker (`Refresher::stop`), so it can wait up to this long per run:
/// about twice this on the first refresh (the login run, then the search run).
const TIMEOUT: Duration = Duration::from_secs(20);

/// How often a running `gh` is checked for exit.
const POLL: Duration = Duration::from_millis(25);

/// A GitHub login: 1 to 39 ASCII letters, digits and `-`.
pub fn valid_login(login: &str) -> bool {
    (1..=39).contains(&login.len())
        && login
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn login_args() -> Vec<String> {
    ["api", "user", "--jq", ".login"].map(String::from).to_vec()
}

fn snapshot_args(login: &str) -> Vec<String> {
    vec![
        "api".into(),
        "graphql".into(),
        "-f".into(),
        format!("query={QUERY}"),
        "-f".into(),
        format!("prQuery=is:pr is:open author:{login} archived:false sort:updated-desc"),
        "-f".into(),
        format!(
            "issueQuery=is:issue is:open label:in-progress user:{login} archived:false sort:updated-desc"
        ),
    ]
}

/// One refresh through `run` (a `gh` invocation: args in, stdout or the
/// failure's status out). The login is cached after its first success.
pub struct Gh<R> {
    run: R,
    login: Mutex<Option<String>>,
}

impl<R: Fn(&[String]) -> Result<String, GithubStatus>> Gh<R> {
    pub fn new(run: R) -> Self {
        Self {
            run,
            login: Mutex::new(None),
        }
    }

    pub fn fetch(&self) -> Fetched {
        let login = self.login()?;
        let json = (self.run)(&snapshot_args(&login))?;
        parse_snapshot(&json).map_err(|_| GithubStatus::Error)
    }

    fn login(&self) -> Result<String, GithubStatus> {
        let mut cached = lock(&self.login);
        if let Some(login) = cached.as_ref() {
            return Ok(login.clone());
        }
        let stdout = (self.run)(&login_args())?;
        let login = stdout.trim();
        if !valid_login(login) {
            return Err(GithubStatus::Error);
        }
        *cached = Some(login.to_string());
        Ok(login.to_string())
    }
}

/// Run the real `gh` with `args`.
/// `gh` is looked up on the `PATH` the app started with, so a fresh install is
/// not seen there until restart; when it is not found, the default install
/// location (`%ProgramFiles%\GitHub CLI`) is tried once. Only an install there
/// is picked up without a restart.
pub fn run_gh(args: &[String]) -> Result<String, GithubStatus> {
    let first = run("gh", args, TIMEOUT);
    if first != Err(GithubStatus::GhMissing) {
        return first;
    }
    match gh_fallback(std::env::var_os("ProgramFiles").as_deref()) {
        Some(path) if path.is_file() => run(&path, args, TIMEOUT),
        _ => first,
    }
}

/// Where the default gh install lives under `program_files`, if that is known.
fn gh_fallback(program_files: Option<&std::ffi::OsStr>) -> Option<std::path::PathBuf> {
    program_files.map(|dir| std::path::Path::new(dir).join("GitHub CLI").join("gh.exe"))
}

/// Read `pipe` to its end on its own thread, so a full pipe never blocks the child.
fn drain(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// Run `program` with `args` (no shell), without a console window on Windows.
/// Stdout on success; otherwise the failure's status. Killed after `timeout`.
fn run(
    program: impl AsRef<std::ffi::OsStr>,
    args: &[String],
    timeout: Duration,
) -> Result<String, GithubStatus> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|e| classify_failure(e.kind() == ErrorKind::NotFound, None, ""))?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            // Timed out, or the wait itself failed. The drain threads end when
            // the pipes close; nothing waits for them.
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GithubStatus::Error);
            }
        }
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    if status.success() {
        String::from_utf8(stdout).map_err(|_| GithubStatus::Error)
    } else {
        let stderr = String::from_utf8_lossy(&stderr);
        Err(classify_failure(false, status.code(), &stderr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn gh_fallback_is_the_default_install_under_program_files() {
        let got = gh_fallback(Some(std::ffi::OsStr::new("C:\\Program Files")));
        let want = std::path::Path::new("C:\\Program Files")
            .join("GitHub CLI")
            .join("gh.exe");
        assert_eq!(got, Some(want));
    }

    #[test]
    fn gh_fallback_is_none_without_program_files() {
        assert_eq!(gh_fallback(None), None);
    }

    type Reply = Result<String, GithubStatus>;
    type Calls = Arc<Mutex<Vec<Vec<String>>>>;

    const EMPTY: &str = r#"{"data":{"prs":{"nodes":[]},"issues":{"nodes":[]}}}"#;

    fn reply(stdout: &str) -> Reply {
        Ok(stdout.to_string())
    }

    /// A `Gh` over a fake runner serving `replies` in order; records each call.
    fn fake(replies: Vec<Reply>) -> (Gh<impl Fn(&[String]) -> Reply>, Calls) {
        let queue = Mutex::new(VecDeque::from(replies));
        let calls: Calls = Arc::default();
        let seen = Arc::clone(&calls);
        let gh = Gh::new(move |args: &[String]| {
            seen.lock().unwrap().push(args.to_vec());
            queue
                .lock()
                .unwrap()
                .pop_front()
                .expect("no more scripted replies")
        });
        (gh, calls)
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn valid_login_accepts_github_logins_only() {
        for ok in ["a", "octo-cat", "Octo123", &"x".repeat(39)] {
            assert!(valid_login(ok), "{ok:?} should be valid");
        }
        for bad in [
            "",
            &"x".repeat(40),
            "octo cat",
            "octo;cat",
            "octo_cat",
            "öcto",
            "-x\n",
        ] {
            assert!(!valid_login(bad), "{bad:?} should be invalid");
        }
    }

    #[test]
    fn login_is_read_with_gh_api_user() {
        assert_eq!(login_args(), strings(&["api", "user", "--jq", ".login"]));
    }

    #[test]
    fn searches_are_passed_as_graphql_variables() {
        assert_eq!(
            snapshot_args("octo-cat"),
            strings(&[
                "api",
                "graphql",
                "-f",
                &format!("query={QUERY}"),
                "-f",
                "prQuery=is:pr is:open author:octo-cat archived:false sort:updated-desc",
                "-f",
                "issueQuery=is:issue is:open label:in-progress user:octo-cat archived:false sort:updated-desc",
            ])
        );
        assert!(QUERY.contains("search(query:$prQuery"));
        assert!(QUERY.contains("search(query:$issueQuery"));
    }

    #[test]
    fn reads_the_login_once_then_searches_with_it() {
        let (gh, calls) = fake(vec![reply("octo-cat\n"), reply(EMPTY), reply(EMPTY)]);
        assert_eq!(gh.fetch(), Ok((vec![], vec![])));
        assert_eq!(gh.fetch(), Ok((vec![], vec![])));
        assert_eq!(
            *calls.lock().unwrap(),
            [
                login_args(),
                snapshot_args("octo-cat"),
                snapshot_args("octo-cat")
            ]
        );
    }

    #[test]
    fn a_failed_login_is_its_status_and_is_retried_next_time() {
        let (gh, calls) = fake(vec![
            Err(GithubStatus::SignedOut),
            reply("octo-cat"),
            reply(EMPTY),
        ]);
        assert_eq!(gh.fetch(), Err(GithubStatus::SignedOut));
        assert_eq!(gh.fetch(), Ok((vec![], vec![])));
        assert_eq!(
            *calls.lock().unwrap(),
            [login_args(), login_args(), snapshot_args("octo-cat")]
        );
    }

    #[test]
    fn an_invalid_login_is_an_error_and_is_never_searched() {
        let (gh, calls) = fake(vec![reply("octo cat; rm -rf\n")]);
        assert_eq!(gh.fetch(), Err(GithubStatus::Error));
        assert_eq!(*calls.lock().unwrap(), [login_args()]);
    }

    #[test]
    fn a_failed_search_is_its_status_and_an_unparsable_one_is_an_error() {
        let (gh, _calls) = fake(vec![
            reply("octo-cat"),
            Err(GithubStatus::SignedOut),
            reply("not json"),
        ]);
        assert_eq!(gh.fetch(), Err(GithubStatus::SignedOut));
        assert_eq!(gh.fetch(), Err(GithubStatus::Error));
    }

    #[test]
    fn a_missing_program_is_gh_missing() {
        assert_eq!(
            run(
                "edge-deck-test-no-such-program",
                &[],
                Duration::from_secs(5)
            ),
            Err(GithubStatus::GhMissing)
        );
    }

    #[cfg(windows)]
    fn cmd(script: &str, timeout: Duration) -> Reply {
        run("cmd", &strings(&["/C", script]), timeout)
    }

    #[cfg(windows)]
    #[test]
    fn success_returns_stdout_even_when_it_outgrows_a_pipe_buffer() {
        assert_eq!(
            cmd("echo octo", Duration::from_secs(10)).map(|s| s.trim().to_string()),
            Ok("octo".to_string())
        );
        let long = cmd(
            "for /L %i in (1,1,3000) do @echo line%i",
            Duration::from_secs(10),
        )
        .expect("the loop succeeds");
        assert_eq!(long.lines().count(), 3000);
        assert_eq!(long.lines().last(), Some("line3000"));
    }

    #[cfg(windows)]
    #[test]
    fn a_failed_run_is_classified_by_exit_code() {
        assert_eq!(
            cmd("exit 4", Duration::from_secs(10)),
            Err(GithubStatus::SignedOut)
        );
        assert_eq!(
            cmd("exit 1", Duration::from_secs(10)),
            Err(GithubStatus::Error)
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_run_past_the_timeout_is_killed_and_is_an_error() {
        let started = Instant::now();
        assert_eq!(
            cmd("ping -n 30 127.0.0.1", Duration::from_millis(300)),
            Err(GithubStatus::Error)
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
