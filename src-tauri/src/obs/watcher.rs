//! The reconnecting worker thread and the shared state it publishes.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::config::Settings;
use super::session::{self, Outcome, Update};
use super::{Connection, ObsState};

/// Timeouts and backoff for the worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Socket read timeout: how often the worker checks for a stop request.
    pub poll: Duration,
    /// TCP connect timeout.
    pub connect_timeout: Duration,
    /// Time allowed from TCP connect to Identified.
    pub identify_timeout: Duration,
    /// First retry delay; doubles per consecutive failure.
    pub backoff_base: Duration,
    /// Retry delay cap.
    pub backoff_max: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(200),
            connect_timeout: Duration::from_secs(1),
            identify_timeout: Duration::from_secs(5),
            backoff_base: Duration::from_millis(500),
            backoff_max: Duration::from_secs(10),
        }
    }
}

/// Delay before the next attempt after `failures` consecutive failed attempts.
pub fn backoff(timing: &Timing, failures: u32) -> Duration {
    timing
        .backoff_base
        .saturating_mul(2u32.saturating_pow(failures))
        .min(timing.backoff_max)
}

/// Lock, recovering the data if a holder panicked: the values stay valid.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Shared {
    state: Mutex<ObsState>,
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

    fn connection(&self) -> Connection {
        lock(&self.state).connection
    }

    /// Store `next` and hand it to `sink`, only if it changed.
    fn publish(&self, next: ObsState, sink: &dyn Fn(ObsState)) {
        let changed = {
            let mut state = lock(&self.state);
            let changed = *state != next;
            *state = next;
            changed
        };
        if changed {
            sink(next);
        }
    }
}

fn work(
    shared: &Shared,
    config: &dyn Fn() -> Option<Settings>,
    sink: &dyn Fn(ObsState),
    timing: &Timing,
) {
    let state = |connection, streaming| ObsState {
        connection,
        streaming,
    };
    let mut failures = 0u32;
    while !shared.is_stopped() {
        let outcome = match config() {
            None => Outcome::Ended,
            Some(settings) => {
                session::run(&settings, timing, &|| shared.is_stopped(), &mut |update| {
                    match update {
                        // A retry after a rejected password stays "auth-failed"
                        // until it resolves, instead of flickering.
                        Update::Connecting if shared.connection() == Connection::AuthFailed => {}
                        Update::Connecting => {
                            shared.publish(state(Connection::Connecting, false), sink)
                        }
                        Update::Connected => {
                            failures = 0;
                            shared.publish(state(Connection::Connected, false), sink);
                        }
                        Update::Streaming(active) => {
                            shared.publish(state(Connection::Connected, active), sink);
                        }
                    }
                })
            }
        };
        match outcome {
            Outcome::Stopped => return,
            Outcome::AuthFailed => shared.publish(state(Connection::AuthFailed, false), sink),
            Outcome::Ended => shared.publish(ObsState::default(), sink),
        }
        shared.sleep(backoff(timing, failures));
        failures = failures.saturating_add(1);
    }
}

/// Owns the worker thread. Dropping it (or `stop`) ends the thread.
pub struct Watcher {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Watcher {
    /// Start the worker. `config` is called before every connect attempt, so a
    /// changed port or password is picked up without a restart. `sink` gets
    /// each new state, only when it differs from the last one.
    pub fn spawn<C, S>(config: C, sink: S, timing: Timing) -> Self
    where
        C: Fn() -> Option<Settings> + Send + 'static,
        S: Fn(ObsState) + Send + 'static,
    {
        let shared = Arc::new(Shared {
            state: Mutex::new(ObsState::default()),
            stopped: Mutex::new(false),
            wake: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("obs-watcher".into())
            .spawn(move || work(&worker_shared, &config, &sink, &timing))
            .expect("spawn the OBS watcher thread");
        Self {
            shared,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn current(&self) -> ObsState {
        *lock(&self.shared.state)
    }

    /// Ask the worker to stop and wait for it. Safe to call more than once.
    pub fn stop(&self) {
        *lock(&self.shared.stopped) = true;
        self.shared.wake.notify_all();
        if let Some(worker) = lock(&self.worker).take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::super::fake_server::{self as fake, FakeObs};
    use super::*;
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
    use std::time::Instant;
    use tungstenite::Message;

    const PASSWORD: &str = "synthetic-password";
    const SALT: &str = "c3ludGhldGljLXNhbHQ=";
    const CHALLENGE: &str = "c3ludGhldGljLWNoYWxsZW5nZQ==";
    // base64(sha256(base64(sha256(PASSWORD + SALT)) + CHALLENGE)), computed with Node.
    const AUTH: &str = "/Sx6qoKgtsNiZXK0LdvlJ/1JoH0HCWJh68/9dARmv6I=";
    const WAIT: Duration = Duration::from_secs(5);
    const QUIET: Duration = Duration::from_millis(200);

    fn fast() -> Timing {
        Timing {
            poll: Duration::from_millis(10),
            connect_timeout: Duration::from_millis(500),
            identify_timeout: Duration::from_secs(5),
            backoff_base: Duration::from_millis(10),
            backoff_max: Duration::from_millis(50),
        }
    }

    fn st(connection: Connection, streaming: bool) -> ObsState {
        ObsState {
            connection,
            streaming,
        }
    }

    type Config = Arc<Mutex<Option<Settings>>>;

    fn start(initial: Option<Settings>, timing: Timing) -> (Watcher, Receiver<ObsState>, Config) {
        let config: Config = Arc::new(Mutex::new(initial));
        let read = Arc::clone(&config);
        let (tx, rx) = mpsc::channel();
        let watcher = Watcher::spawn(
            move || read.lock().unwrap().clone(),
            move |state| {
                let _ = tx.send(state);
            },
            timing,
        );
        (watcher, rx, config)
    }

    fn next(rx: &Receiver<ObsState>) -> ObsState {
        rx.recv_timeout(WAIT).expect("expected a state change")
    }

    fn assert_quiet(rx: &Receiver<ObsState>) {
        match rx.recv_timeout(QUIET) {
            Err(RecvTimeoutError::Timeout) => {}
            other => panic!("expected no state change, got {other:?}"),
        }
    }

    #[test]
    fn backoff_doubles_up_to_the_cap() {
        let timing = Timing::default();
        let delays: Vec<u64> = (0..7)
            .map(|n| backoff(&timing, n).as_millis() as u64)
            .collect();
        assert_eq!(delays, [500, 1000, 2000, 4000, 8000, 10000, 10000]);
        assert_eq!(backoff(&timing, u32::MAX), Duration::from_secs(10));
    }

    #[test]
    fn hello_without_auth_then_follows_the_stream_and_reconnects_after_a_drop() {
        let obs = FakeObs::new();
        let (watcher, rx, _config) = start(Some(Settings::new(obs.port, None)), fast());
        assert_eq!(watcher.current(), st(Connection::Off, false));

        let mut c = obs.accept();
        assert_eq!(next(&rx), st(Connection::Connecting, false));
        // Stream news before Identified is not trusted.
        fake::stream_changed(&mut c, true);
        fake::hello(&mut c, None);
        let identify = fake::recv(&mut c);
        assert_eq!(identify["op"], 1);
        assert_eq!(identify["d"]["rpcVersion"], 1);
        assert_eq!(identify["d"]["eventSubscriptions"], 64);
        assert!(identify["d"].get("authentication").is_none());

        fake::identified(&mut c);
        assert_eq!(next(&rx), st(Connection::Connected, false));
        let request = fake::recv(&mut c);
        assert_eq!(request["op"], 6);
        assert_eq!(request["d"]["requestType"], "GetStreamStatus");
        fake::stream_status(&mut c, &request, true);
        assert_eq!(next(&rx), st(Connection::Connected, true));

        c.send(Message::binary(vec![1, 2, 3])).unwrap();
        c.send(Message::text("not json")).unwrap();
        fake::stream_changed(&mut c, true);
        fake::stream_changed(&mut c, false);
        assert_eq!(next(&rx), st(Connection::Connected, false));
        fake::stream_changed(&mut c, true);
        assert_eq!(next(&rx), st(Connection::Connected, true));
        assert_eq!(watcher.current(), st(Connection::Connected, true));

        // OBS goes away mid-stream: off, and no longer streaming.
        drop(c);
        assert_eq!(next(&rx), st(Connection::Off, false));

        let mut c = obs.accept();
        assert_eq!(next(&rx), st(Connection::Connecting, false));
        fake::hello(&mut c, None);
        fake::recv(&mut c);
        fake::identified(&mut c);
        assert_eq!(next(&rx), st(Connection::Connected, false));
    }

    #[test]
    fn hello_with_auth_sends_the_challenge_response() {
        let obs = FakeObs::new();
        let (_watcher, rx, _config) = start(Some(Settings::new(obs.port, Some(PASSWORD))), fast());
        let mut c = obs.accept();
        assert_eq!(next(&rx), st(Connection::Connecting, false));
        fake::hello(&mut c, Some((CHALLENGE, SALT)));
        let identify = fake::recv(&mut c);
        assert_eq!(identify["d"]["authentication"], AUTH);
        assert_eq!(identify["d"]["eventSubscriptions"], 64);
        fake::identified(&mut c);
        assert_eq!(next(&rx), st(Connection::Connected, false));
    }

    #[test]
    fn rejected_password_holds_auth_failed_until_a_reread_config_fixes_it() {
        let obs = FakeObs::new();
        let (watcher, rx, config) = start(
            Some(Settings::new(obs.port, Some("wrong-placeholder"))),
            fast(),
        );
        let mut c = obs.accept();
        assert_eq!(next(&rx), st(Connection::Connecting, false));
        fake::hello(&mut c, Some((CHALLENGE, SALT)));
        assert_ne!(fake::recv(&mut c)["d"]["authentication"], AUTH);
        fake::close(&mut c, 4009);
        assert_eq!(next(&rx), st(Connection::AuthFailed, false));

        // Still wrong on the retry: no flicker through "connecting".
        let mut c = obs.accept();
        fake::hello(&mut c, Some((CHALLENGE, SALT)));
        assert_ne!(fake::recv(&mut c)["d"]["authentication"], AUTH);
        // The user fixes the password in OBS; the next attempt re-reads it.
        *config.lock().unwrap() = Some(Settings::new(obs.port, Some(PASSWORD)));
        fake::close(&mut c, 4009);
        assert_eq!(watcher.current(), st(Connection::AuthFailed, false));

        let mut c = obs.accept();
        fake::hello(&mut c, Some((CHALLENGE, SALT)));
        assert_eq!(fake::recv(&mut c)["d"]["authentication"], AUTH);
        fake::identified(&mut c);
        assert_eq!(next(&rx), st(Connection::Connected, false));
    }

    #[test]
    fn hello_needing_auth_without_a_known_password_is_auth_failed() {
        let obs = FakeObs::new();
        let (_watcher, rx, _config) = start(Some(Settings::new(obs.port, None)), fast());
        let mut c = obs.accept();
        assert_eq!(next(&rx), st(Connection::Connecting, false));
        fake::hello(&mut c, Some((CHALLENGE, SALT)));
        assert!(fake::recv(&mut c)["d"].get("authentication").is_none());
        fake::close(&mut c, 4009);
        assert_eq!(next(&rx), st(Connection::AuthFailed, false));
    }

    #[test]
    fn other_close_codes_are_off() {
        let obs = FakeObs::new();
        let (_watcher, rx, _config) = start(Some(Settings::new(obs.port, None)), fast());
        let mut c = obs.accept();
        assert_eq!(next(&rx), st(Connection::Connecting, false));
        fake::hello(&mut c, None);
        fake::recv(&mut c);
        fake::close(&mut c, 4010);
        assert_eq!(next(&rx), st(Connection::Off, false));
    }

    #[test]
    fn closing_while_streaming_is_off_and_not_streaming() {
        let obs = FakeObs::new();
        let (_watcher, rx, _config) = start(Some(Settings::new(obs.port, None)), fast());
        let mut c = obs.accept();
        next(&rx);
        fake::hello(&mut c, None);
        fake::recv(&mut c);
        fake::identified(&mut c);
        next(&rx);
        let request = fake::recv(&mut c);
        fake::stream_status(&mut c, &request, true);
        assert_eq!(next(&rx), st(Connection::Connected, true));
        fake::close(&mut c, 1001);
        assert_eq!(next(&rx), st(Connection::Off, false));
    }

    #[test]
    fn missing_or_disabled_config_stays_off_and_is_reread() {
        let obs = FakeObs::new();
        let (watcher, rx, config) = start(None, fast());
        assert_quiet(&rx);
        assert_eq!(watcher.current(), st(Connection::Off, false));
        *config.lock().unwrap() = Some(Settings::new(obs.port, None));
        let _c = obs.accept();
        assert_eq!(next(&rx), st(Connection::Connecting, false));
    }

    #[test]
    fn closed_port_stays_off_and_keeps_retrying() {
        let (watcher, rx, config) = start(Some(Settings::new(fake::closed_port(), None)), fast());
        assert_quiet(&rx);
        assert_eq!(watcher.current(), st(Connection::Off, false));
        let obs = FakeObs::new();
        *config.lock().unwrap() = Some(Settings::new(obs.port, None));
        let _c = obs.accept();
        assert_eq!(next(&rx), st(Connection::Connecting, false));
    }

    #[test]
    fn silent_server_times_out_then_retries() {
        let obs = FakeObs::new();
        let timing = Timing {
            identify_timeout: Duration::from_millis(200),
            ..fast()
        };
        let (_watcher, rx, _config) = start(Some(Settings::new(obs.port, None)), timing);
        let _c = obs.accept();
        assert_eq!(next(&rx), st(Connection::Connecting, false));
        assert_eq!(next(&rx), st(Connection::Off, false));
        let _c2 = obs.accept();
        assert_eq!(next(&rx), st(Connection::Connecting, false));
    }

    #[test]
    fn stop_ends_a_connected_worker_promptly() {
        let obs = FakeObs::new();
        let (watcher, rx, _config) = start(Some(Settings::new(obs.port, None)), Timing::default());
        let mut c = obs.accept();
        next(&rx);
        fake::hello(&mut c, None);
        fake::recv(&mut c);
        fake::identified(&mut c);
        assert_eq!(next(&rx), st(Connection::Connected, false));

        let began = Instant::now();
        watcher.stop();
        assert!(
            began.elapsed() < Duration::from_secs(2),
            "{:?}",
            began.elapsed()
        );
        // The worker is gone, so its sink is dropped and nothing more arrives.
        assert_eq!(rx.recv_timeout(WAIT), Err(RecvTimeoutError::Disconnected));
        watcher.stop();
    }

    #[test]
    fn dropping_the_watcher_interrupts_a_long_backoff() {
        let timing = Timing {
            backoff_base: Duration::from_secs(60),
            backoff_max: Duration::from_secs(60),
            ..fast()
        };
        let (watcher, rx, _config) = start(None, timing);
        assert_quiet(&rx);
        let began = Instant::now();
        drop(watcher);
        assert!(
            began.elapsed() < Duration::from_secs(2),
            "{:?}",
            began.elapsed()
        );
        assert_eq!(rx.recv_timeout(WAIT), Err(RecvTimeoutError::Disconnected));
    }

    /// Live check against the OBS on this machine, reading OBS's own settings
    /// file. Run by hand: .
    /// Prints only the connection state, never the settings.
    #[test]
    #[ignore = "needs a running OBS with its WebSocket server enabled"]
    fn live_obs_connects_with_the_local_settings() {
        let watcher = Watcher::spawn(
            || crate::obs::default_config_path().and_then(|path| crate::obs::read_config(&path)),
            |_| {},
            Timing::default(),
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        while watcher.current().connection != crate::obs::Connection::Connected
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(100));
        }
        let state = watcher.current();
        watcher.stop();
        println!("live OBS state: {state:?}");
        assert_eq!(state.connection, crate::obs::Connection::Connected);
    }
}
