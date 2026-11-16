//! The background thread that re-runs `gh` on an interval and publishes changes.

use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::{GithubState, GithubStatus, Issue, PullRequest, lock};

/// One refresh: the lists, or the status that explains why there are none.
pub type Fetched = Result<(Vec<PullRequest>, Vec<Issue>), GithubStatus>;

fn state_of(fetched: Fetched) -> GithubState {
    match fetched {
        Ok((prs, issues)) => GithubState {
            status: GithubStatus::Ok,
            prs,
            issues,
        },
        Err(status) => GithubState {
            status,
            prs: Vec::new(),
            issues: Vec::new(),
        },
    }
}

struct Shared {
    state: Mutex<GithubState>,
    stopped: Mutex<bool>,
    wake: Condvar,
}

impl Shared {
    fn is_stopped(&self) -> bool {
        *lock(&self.stopped)
    }

    /// Sleep for `delay` unless a stop request arrives first.
    fn sleep(&self, delay: Duration) {
        let guard = lock(&self.stopped);
        let _ = self
            .wake
            .wait_timeout_while(guard, delay, |stopped| !*stopped);
    }

    /// Store `next` and hand it to `sink`, only if it changed.
    fn publish(&self, next: GithubState, sink: &dyn Fn(GithubState)) {
        let changed = {
            let mut state = lock(&self.state);
            let changed = *state != next;
            if changed {
                *state = next.clone();
            }
            changed
        };
        if changed {
            sink(next);
        }
    }
}

fn work(
    shared: &Shared,
    fetch: &dyn Fn() -> Fetched,
    sink: &dyn Fn(GithubState),
    interval: Duration,
) {
    while !shared.is_stopped() {
        let next = state_of(fetch());
        // A stop that arrived during the fetch wins: nothing more is published.
        if shared.is_stopped() {
            return;
        }
        shared.publish(next, sink);
        shared.sleep(interval);
    }
}

/// Owns the worker thread. Dropping it (or `stop`) ends the thread.
pub struct Refresher {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Refresher {
    /// Fetch at once, then every `interval`. `sink` gets each new state, only
    /// when it differs from the last one; a failed fetch sets its status and
    /// the next tick retries.
    pub fn spawn<F, S>(fetch: F, sink: S, interval: Duration) -> Self
    where
        F: Fn() -> Fetched + Send + 'static,
        S: Fn(GithubState) + Send + 'static,
    {
        let shared = Arc::new(Shared {
            state: Mutex::new(state_of(Err(GithubStatus::Loading))),
            stopped: Mutex::new(false),
            wake: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("github-refresher".into())
            .spawn(move || work(&worker_shared, &fetch, &sink, interval))
            .expect("spawn the GitHub refresher thread");
        Self {
            shared,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn current(&self) -> GithubState {
        lock(&self.shared.state).clone()
    }

    /// Ask the worker to stop and wait for it. Safe to call more than once.
    /// A `gh` run already in flight finishes (or times out) first.
    pub fn stop(&self) {
        *lock(&self.shared.stopped) = true;
        self.shared.wake.notify_all();
        if let Some(worker) = lock(&self.worker).take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Refresher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::super::Checks;
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
    use std::sync::{Arc, Mutex};

    const WAIT: Duration = Duration::from_secs(5);
    const QUIET: Duration = Duration::from_millis(200);
    const FAST: Duration = Duration::from_millis(10);

    fn pr(number: u64) -> PullRequest {
        PullRequest {
            repo: "octo/widgets".into(),
            number,
            title: format!("PR {number}"),
            url: format!("https://example.test/octo/widgets/pull/{number}"),
            draft: false,
            checks: Checks::Passing,
        }
    }

    fn issue(number: u64) -> Issue {
        Issue {
            repo: "octo/widgets".into(),
            number,
            title: format!("Issue {number}"),
            url: format!("https://example.test/octo/widgets/issues/{number}"),
        }
    }

    fn ok(prs: Vec<PullRequest>, issues: Vec<Issue>) -> Fetched {
        Ok((prs, issues))
    }

    fn state(status: GithubStatus, prs: Vec<PullRequest>, issues: Vec<Issue>) -> GithubState {
        GithubState {
            status,
            prs,
            issues,
        }
    }

    fn failed(status: GithubStatus) -> GithubState {
        state(status, vec![], vec![])
    }

    /// Serve `script` in order, repeating the last entry; count the calls.
    fn start(
        script: Vec<Fetched>,
        interval: Duration,
    ) -> (Refresher, Receiver<GithubState>, Arc<AtomicUsize>) {
        let queue = Arc::new(Mutex::new(VecDeque::from(script)));
        let last: Arc<Mutex<Option<Fetched>>> = Arc::new(Mutex::new(None));
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let (tx, rx) = mpsc::channel();
        let refresher = Refresher::spawn(
            move || {
                counted.fetch_add(1, Ordering::SeqCst);
                let mut queue = queue.lock().unwrap();
                let mut last = last.lock().unwrap();
                if let Some(next) = queue.pop_front() {
                    *last = Some(next);
                }
                last.clone().expect("script is not empty")
            },
            move |s| {
                let _ = tx.send(s);
            },
            interval,
        );
        (refresher, rx, calls)
    }

    fn next(rx: &Receiver<GithubState>) -> GithubState {
        rx.recv_timeout(WAIT).expect("expected a state change")
    }

    fn assert_quiet(rx: &Receiver<GithubState>) {
        match rx.recv_timeout(QUIET) {
            Err(RecvTimeoutError::Timeout) => {}
            other => panic!("expected no state change, got {other:?}"),
        }
    }

    fn wait_for_calls(calls: &AtomicUsize, at_least: usize) {
        let deadline = std::time::Instant::now() + WAIT;
        while calls.load(Ordering::SeqCst) < at_least {
            assert!(
                std::time::Instant::now() < deadline,
                "fetch was not called {at_least} times"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn is_loading_until_the_first_fetch_returns_then_publishes_it() {
        let (release, gate) = mpsc::channel::<()>();
        let gate = Mutex::new(gate);
        let (tx, rx) = mpsc::channel();
        let refresher = Refresher::spawn(
            move || {
                let _ = gate.lock().unwrap().recv_timeout(WAIT);
                ok(vec![pr(1)], vec![issue(2)])
            },
            move |s| {
                let _ = tx.send(s);
            },
            FAST,
        );
        assert_eq!(refresher.current(), failed(GithubStatus::Loading));
        assert_quiet(&rx);
        release.send(()).unwrap();
        let expected = state(GithubStatus::Ok, vec![pr(1)], vec![issue(2)]);
        assert_eq!(next(&rx), expected);
        assert_eq!(refresher.current(), expected);
    }

    #[test]
    fn fetches_again_every_interval() {
        let (_refresher, _rx, calls) = start(vec![ok(vec![], vec![])], FAST);
        wait_for_calls(&calls, 4);
    }

    #[test]
    fn waits_for_the_interval_between_fetches() {
        let (_refresher, rx, calls) = start(vec![ok(vec![], vec![])], Duration::from_secs(60));
        assert_eq!(next(&rx).status, GithubStatus::Ok);
        assert_quiet(&rx);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn emits_only_when_the_state_changes() {
        let (refresher, rx, calls) = start(
            vec![
                ok(vec![pr(1)], vec![]),
                ok(vec![pr(1)], vec![]),
                ok(vec![pr(1)], vec![]),
                ok(vec![pr(1), pr(2)], vec![]),
            ],
            FAST,
        );
        assert_eq!(next(&rx), state(GithubStatus::Ok, vec![pr(1)], vec![]));
        assert_eq!(
            next(&rx),
            state(GithubStatus::Ok, vec![pr(1), pr(2)], vec![])
        );
        wait_for_calls(&calls, 6);
        assert_quiet(&rx);
        assert_eq!(refresher.current().prs.len(), 2);
    }

    #[test]
    fn a_repeated_failure_emits_once() {
        let (refresher, rx, calls) = start(vec![Err(GithubStatus::GhMissing)], FAST);
        assert_eq!(next(&rx), failed(GithubStatus::GhMissing));
        wait_for_calls(&calls, 4);
        assert_quiet(&rx);
        assert_eq!(refresher.current(), failed(GithubStatus::GhMissing));
    }

    #[test]
    fn recovers_after_failed_fetches_without_a_restart() {
        let (refresher, rx, _calls) = start(
            vec![
                Err(GithubStatus::GhMissing),
                Err(GithubStatus::SignedOut),
                Err(GithubStatus::Error),
                ok(vec![pr(5)], vec![issue(6)]),
            ],
            FAST,
        );
        assert_eq!(next(&rx), failed(GithubStatus::GhMissing));
        assert_eq!(next(&rx), failed(GithubStatus::SignedOut));
        assert_eq!(next(&rx), failed(GithubStatus::Error));
        assert_eq!(
            next(&rx),
            state(GithubStatus::Ok, vec![pr(5)], vec![issue(6)])
        );
        assert_eq!(refresher.current().status, GithubStatus::Ok);
    }

    #[test]
    fn a_failure_after_success_drops_the_lists() {
        let (refresher, rx, _calls) = start(
            vec![ok(vec![pr(1)], vec![issue(1)]), Err(GithubStatus::Error)],
            FAST,
        );
        assert_eq!(next(&rx).status, GithubStatus::Ok);
        assert_eq!(next(&rx), failed(GithubStatus::Error));
        assert_eq!(refresher.current(), failed(GithubStatus::Error));
    }

    #[test]
    fn stop_ends_fetching_and_emitting_and_is_idempotent() {
        let (refresher, rx, calls) = start(vec![ok(vec![], vec![])], FAST);
        next(&rx);
        wait_for_calls(&calls, 2);
        refresher.stop();
        refresher.stop();
        let after = calls.load(Ordering::SeqCst);
        std::thread::sleep(QUIET);
        assert_eq!(calls.load(Ordering::SeqCst), after);
        // Quiet either way: the worker's sink may be dropped with it (Disconnected).
        assert!(matches!(
            rx.recv_timeout(QUIET),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn stop_returns_promptly_even_with_a_long_interval() {
        let (refresher, rx, _calls) = start(vec![ok(vec![], vec![])], Duration::from_secs(60));
        next(&rx);
        let started = std::time::Instant::now();
        refresher.stop();
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn dropping_the_refresher_stops_the_thread() {
        let (refresher, rx, calls) = start(vec![ok(vec![], vec![])], FAST);
        next(&rx);
        drop(refresher);
        let after = calls.load(Ordering::SeqCst);
        std::thread::sleep(QUIET);
        assert_eq!(calls.load(Ordering::SeqCst), after);
    }
}
