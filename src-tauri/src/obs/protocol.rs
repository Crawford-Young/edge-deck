//! obs-websocket v5 message shapes: the few the watcher sends and reads.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// `EventSubscription::Outputs`: carries `StreamStateChanged`.
pub const OUTPUTS_SUBSCRIPTION: u64 = 1 << 6;
/// `WebSocketCloseCode::AuthenticationFailed`.
pub const AUTH_FAILED_CLOSE: u16 = 4009;
/// The only RPC version obs-websocket v5 speaks.
pub const RPC_VERSION: u64 = 1;

/// The `authentication` object of a Hello.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub challenge: String,
    pub salt: String,
}

/// A server message, reduced to what the watcher acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    /// OpCode 0; `Some` when the server requires authentication.
    Hello(Option<Challenge>),
    /// OpCode 2.
    Identified,
    /// A `GetStreamStatus` response or a `StreamStateChanged` event.
    Streaming(bool),
    /// Anything else, including malformed text.
    Ignored,
}

/// `base64(sha256(base64(sha256(password + salt)) + challenge))`.
pub fn auth_string(password: &str, salt: &str, challenge: &str) -> String {
    let hash = |text: String| STANDARD.encode(Sha256::digest(text.as_bytes()));
    hash(hash(format!("{password}{salt}")) + challenge)
}

pub fn parse_incoming(text: &str) -> Incoming {
    let Ok(msg) = serde_json::from_str::<Value>(text) else {
        return Incoming::Ignored;
    };
    let d = &msg["d"];
    let streaming = |active: &Value| {
        active
            .as_bool()
            .map_or(Incoming::Ignored, Incoming::Streaming)
    };
    match msg["op"].as_u64() {
        Some(0) => {
            let auth = &d["authentication"];
            let challenge = auth["challenge"].as_str().zip(auth["salt"].as_str());
            Incoming::Hello(challenge.map(|(challenge, salt)| Challenge {
                challenge: challenge.to_owned(),
                salt: salt.to_owned(),
            }))
        }
        Some(2) => Incoming::Identified,
        Some(5) if d["eventType"] == "StreamStateChanged" => {
            streaming(&d["eventData"]["outputActive"])
        }
        Some(7)
            if d["requestType"] == "GetStreamStatus" && d["requestStatus"]["result"] == true =>
        {
            streaming(&d["responseData"]["outputActive"])
        }
        _ => Incoming::Ignored,
    }
}

/// The Identify (OpCode 1) message. `authentication` is included only when the
/// server asked for it and a password is known.
pub fn identify(password: Option<&str>, challenge: Option<&Challenge>) -> String {
    let mut d = json!({"rpcVersion": RPC_VERSION, "eventSubscriptions": OUTPUTS_SUBSCRIPTION});
    if let (Some(password), Some(c)) = (password, challenge) {
        d["authentication"] = auth_string(password, &c.salt, &c.challenge).into();
    }
    json!({"op": 1, "d": d}).to_string()
}

/// A `GetStreamStatus` request (OpCode 6).
pub fn stream_status_request() -> String {
    json!({"op": 6, "d": {"requestType": "GetStreamStatus", "requestId": "edge-deck-stream-status"}})
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn challenge() -> Challenge {
        Challenge {
            challenge: "c3ludGhldGljLWNoYWxsZW5nZQ==".into(),
            salt: "c3ludGhldGljLXNhbHQ=".into(),
        }
    }

    fn parse(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn auth_string_matches_synthetic_vectors() {
        // Expected values computed independently with Node's crypto module.
        assert_eq!(
            auth_string(
                "synthetic-password",
                "c3ludGhldGljLXNhbHQ=",
                "c3ludGhldGljLWNoYWxsZW5nZQ=="
            ),
            "/Sx6qoKgtsNiZXK0LdvlJ/1JoH0HCWJh68/9dARmv6I="
        );
        assert_eq!(
            auth_string("another-placeholder", "AAAAsaltAAAA", "BBBBchallengeBBBB"),
            "nYjDvTr0gVfIb399515x2mOfDXCcMbVXYStrVV6Ub+M="
        );
    }

    #[test]
    fn hello_with_and_without_authentication() {
        let with = json!({"op":0,"d":{"obsWebSocketVersion":"5.0.0","rpcVersion":1,
            "authentication":{"challenge":"c3ludGhldGljLWNoYWxsZW5nZQ==","salt":"c3ludGhldGljLXNhbHQ="}}});
        assert_eq!(
            parse_incoming(&with.to_string()),
            Incoming::Hello(Some(challenge()))
        );
        let without = json!({"op":0,"d":{"obsWebSocketVersion":"5.0.0","rpcVersion":1}});
        assert_eq!(parse_incoming(&without.to_string()), Incoming::Hello(None));
        let partial = json!({"op":0,"d":{"rpcVersion":1,"authentication":{"salt":"s"}}});
        assert_eq!(parse_incoming(&partial.to_string()), Incoming::Hello(None));
    }

    #[test]
    fn identified() {
        let msg = json!({"op":2,"d":{"negotiatedRpcVersion":1}});
        assert_eq!(parse_incoming(&msg.to_string()), Incoming::Identified);
    }

    #[test]
    fn stream_state_changed_events() {
        for active in [true, false] {
            let msg = json!({"op":5,"d":{"eventType":"StreamStateChanged","eventIntent":64,
                "eventData":{"outputActive":active,"outputState":"OBS_WEBSOCKET_OUTPUT_STARTED"}}});
            assert_eq!(
                parse_incoming(&msg.to_string()),
                Incoming::Streaming(active)
            );
        }
        let other = json!({"op":5,"d":{"eventType":"RecordStateChanged","eventIntent":64,
            "eventData":{"outputActive":true}}});
        assert_eq!(parse_incoming(&other.to_string()), Incoming::Ignored);
    }

    #[test]
    fn stream_status_responses() {
        for active in [true, false] {
            let msg = json!({"op":7,"d":{"requestType":"GetStreamStatus","requestId":"x",
                "requestStatus":{"result":true,"code":100},
                "responseData":{"outputActive":active,"outputReconnecting":false}}});
            assert_eq!(
                parse_incoming(&msg.to_string()),
                Incoming::Streaming(active)
            );
        }
        let failed = json!({"op":7,"d":{"requestType":"GetStreamStatus","requestId":"x",
            "requestStatus":{"result":false,"code":204}}});
        assert_eq!(parse_incoming(&failed.to_string()), Incoming::Ignored);
        let other = json!({"op":7,"d":{"requestType":"GetVersion","requestId":"x",
            "requestStatus":{"result":true,"code":100},"responseData":{"outputActive":true}}});
        assert_eq!(parse_incoming(&other.to_string()), Incoming::Ignored);
    }

    #[test]
    fn junk_is_ignored() {
        for text in ["", "not json", "[]", r#"{"op":"0"}"#, r#"{"op":9,"d":{}}"#] {
            assert_eq!(parse_incoming(text), Incoming::Ignored, "{text}");
        }
    }

    #[test]
    fn identify_without_authentication() {
        let msg = parse(&identify(None, None));
        assert_eq!(msg["op"], 1);
        assert_eq!(msg["d"]["rpcVersion"], RPC_VERSION);
        assert_eq!(msg["d"]["eventSubscriptions"], OUTPUTS_SUBSCRIPTION);
        assert!(msg["d"].get("authentication").is_none());
        // A password the server did not ask for is not sent.
        let unasked = parse(&identify(Some("synthetic-password"), None));
        assert!(unasked["d"].get("authentication").is_none());
    }

    #[test]
    fn identify_with_authentication() {
        let msg = parse(&identify(Some("synthetic-password"), Some(&challenge())));
        assert_eq!(msg["op"], 1);
        assert_eq!(msg["d"]["eventSubscriptions"], OUTPUTS_SUBSCRIPTION);
        assert_eq!(
            msg["d"]["authentication"],
            "/Sx6qoKgtsNiZXK0LdvlJ/1JoH0HCWJh68/9dARmv6I="
        );
    }

    #[test]
    fn identify_when_asked_but_no_password_is_known() {
        let msg = parse(&identify(None, Some(&challenge())));
        assert!(msg["d"].get("authentication").is_none());
    }

    #[test]
    fn stream_status_request_shape() {
        let msg = parse(&stream_status_request());
        assert_eq!(msg["op"], 6);
        assert_eq!(msg["d"]["requestType"], "GetStreamStatus");
        assert!(
            msg["d"]["requestId"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
    }
}
