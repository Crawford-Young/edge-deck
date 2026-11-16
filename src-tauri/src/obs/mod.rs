//! Watches OBS over obs-websocket v5 and reports whether it is streaming.
//!
//! Pure pieces (config parsing, auth, message handling, backoff) are plain
//! functions; `session` drives one WebSocket connection and `watcher` owns the
//! reconnecting worker thread. The Tauri glue lives in `lib.rs`.

mod config;
mod protocol;
mod session;
mod watcher;

#[cfg(test)]
mod fake_server;

use serde::Serialize;

pub use config::{default_config_path, read_config};
pub use watcher::{Timing, Watcher};

/// How far the watcher has got with OBS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Connection {
    /// OBS closed, its WebSocket server disabled, or its settings file missing.
    #[default]
    Off,
    /// A socket is open and the Hello/Identify handshake is in progress.
    Connecting,
    /// Identified; `streaming` is live.
    Connected,
    /// OBS rejected the password from its own settings file.
    AuthFailed,
}

/// What the webview sees: the `obs_state` command result and `obs-state` event payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct ObsState {
    pub connection: Connection,
    pub streaming: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_off_and_not_streaming() {
        assert_eq!(
            ObsState::default(),
            ObsState {
                connection: Connection::Off,
                streaming: false
            }
        );
    }

    #[test]
    fn serializes_to_the_webview_shape() {
        let cases = [
            (
                Connection::Off,
                false,
                r#"{"connection":"off","streaming":false}"#,
            ),
            (
                Connection::Connecting,
                false,
                r#"{"connection":"connecting","streaming":false}"#,
            ),
            (
                Connection::Connected,
                true,
                r#"{"connection":"connected","streaming":true}"#,
            ),
            (
                Connection::AuthFailed,
                false,
                r#"{"connection":"auth-failed","streaming":false}"#,
            ),
        ];
        for (connection, streaming, json) in cases {
            let state = ObsState {
                connection,
                streaming,
            };
            assert_eq!(serde_json::to_string(&state).unwrap(), json);
        }
    }
}
