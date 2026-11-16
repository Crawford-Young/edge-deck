//! An in-process stand-in for obs-websocket, driven step by step by a test.

use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::protocol::CloseFrame;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Message, WebSocket};

const WAIT: Duration = Duration::from_secs(5);

pub struct FakeObs {
    listener: TcpListener,
    pub port: u16,
}

pub type Client = WebSocket<TcpStream>;

impl FakeObs {
    pub fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        Self { listener, port }
    }

    /// Accept the next WebSocket client, failing the test after five seconds.
    pub fn accept(&self) -> Client {
        let deadline = Instant::now() + WAIT;
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream.set_read_timeout(Some(WAIT)).unwrap();
                    return tungstenite::accept(stream).unwrap();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "no client connected");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("accept failed: {e}"),
            }
        }
    }
}

/// A port with nothing listening on it.
pub fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

pub fn send(client: &mut Client, msg: Value) {
    client.send(Message::text(msg.to_string())).unwrap();
}

/// The next text message from the client, as JSON.
pub fn recv(client: &mut Client) -> Value {
    loop {
        match client.read().unwrap() {
            Message::Text(text) => return serde_json::from_str(text.as_str()).unwrap(),
            Message::Ping(_) | Message::Pong(_) => {}
            other => panic!("unexpected message: {other:?}"),
        }
    }
}

pub fn hello(client: &mut Client, auth: Option<(&str, &str)>) {
    let mut d = json!({"obsWebSocketVersion":"5.0.0-synthetic","rpcVersion":1});
    if let Some((challenge, salt)) = auth {
        d["authentication"] = json!({"challenge":challenge,"salt":salt});
    }
    send(client, json!({"op":0,"d":d}));
}

pub fn identified(client: &mut Client) {
    send(client, json!({"op":2,"d":{"negotiatedRpcVersion":1}}));
}

pub fn stream_status(client: &mut Client, request: &Value, active: bool) {
    send(
        client,
        json!({"op":7,"d":{"requestType":"GetStreamStatus","requestId":request["d"]["requestId"],
            "requestStatus":{"result":true,"code":100},
            "responseData":{"outputActive":active,"outputReconnecting":false}}}),
    );
}

pub fn stream_changed(client: &mut Client, active: bool) {
    let state = if active {
        "OBS_WEBSOCKET_OUTPUT_STARTED"
    } else {
        "OBS_WEBSOCKET_OUTPUT_STOPPED"
    };
    send(
        client,
        json!({"op":5,"d":{"eventType":"StreamStateChanged","eventIntent":64,
            "eventData":{"outputActive":active,"outputState":state}}}),
    );
}

pub fn close(client: &mut Client, code: u16) {
    client
        .close(Some(CloseFrame {
            code: CloseCode::from(code),
            reason: "synthetic".into(),
        }))
        .unwrap();
    // Drain until the client's close reply (or the socket going away).
    while client.read().is_ok() {}
}
