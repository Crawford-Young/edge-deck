//! One connection to OBS: TCP, WebSocket handshake, Hello/Identify, then
//! stream-state updates until the socket ends or the watcher stops.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Instant;

use tungstenite::{Message, WebSocket};

use super::config::Settings;
use super::protocol::{self, Incoming};
use super::watcher::Timing;

/// Progress reported while the session runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    /// The TCP socket is open; the handshake is in progress.
    Connecting,
    /// OBS sent Identified.
    Connected,
    /// Stream status, only ever reported after `Connected`.
    Streaming(bool),
}

/// Why the session returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Refused, timed out, closed or dropped.
    Ended,
    /// OBS closed the socket with `AuthenticationFailed`.
    AuthFailed,
    /// The watcher asked to stop.
    Stopped,
}

pub fn run(
    settings: &Settings,
    timing: &Timing,
    stop: &dyn Fn() -> bool,
    report: &mut dyn FnMut(Update),
) -> Outcome {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, settings.port));
    let Ok(stream) = TcpStream::connect_timeout(&addr, timing.connect_timeout) else {
        return Outcome::Ended;
    };
    report(Update::Connecting);
    let deadline = Instant::now() + timing.identify_timeout;
    let Some(mut ws) = open(stream, settings.port, timing) else {
        return Outcome::Ended;
    };
    let mut identified = false;
    loop {
        if stop() {
            return Outcome::Stopped;
        }
        if !identified && Instant::now() >= deadline {
            return Outcome::Ended;
        }
        let reply = match ws.read() {
            Ok(Message::Text(text)) => match protocol::parse_incoming(text.as_str()) {
                Incoming::Hello(challenge) => Some(protocol::identify(
                    settings.password.as_deref(),
                    challenge.as_ref(),
                )),
                Incoming::Identified => {
                    identified = true;
                    report(Update::Connected);
                    Some(protocol::stream_status_request())
                }
                Incoming::Streaming(active) if identified => {
                    report(Update::Streaming(active));
                    None
                }
                Incoming::Streaming(_) | Incoming::Ignored => None,
            },
            Ok(Message::Close(frame)) => {
                let auth_failed =
                    frame.is_some_and(|f| u16::from(f.code) == protocol::AUTH_FAILED_CLOSE);
                return if auth_failed {
                    Outcome::AuthFailed
                } else {
                    Outcome::Ended
                };
            }
            Ok(_) => None,
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                None
            }
            Err(_) => return Outcome::Ended,
        };
        if let Some(text) = reply
            && ws.send(Message::text(text)).is_err()
        {
            return Outcome::Ended;
        }
    }
}

/// WebSocket handshake under the identify timeout, then switch reads to the
/// short poll interval so a stop request is noticed promptly.
fn open(stream: TcpStream, port: u16, timing: &Timing) -> Option<WebSocket<TcpStream>> {
    stream
        .set_read_timeout(Some(timing.identify_timeout))
        .ok()?;
    stream
        .set_write_timeout(Some(timing.identify_timeout))
        .ok()?;
    let (ws, _) = tungstenite::client(format!("ws://127.0.0.1:{port}"), stream).ok()?;
    ws.get_ref().set_read_timeout(Some(timing.poll)).ok()?;
    Some(ws)
}
