use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello {
        protocol_version: u32,
        token: u64,
    },
    Command {
        id: u64,
        method: String,
        #[serde(default)]
        params: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeMessage {
    Event {
        event: RuntimeEvent,
        #[serde(default)]
        payload: Value,
    },
    Response {
        id: u64,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<IpcError>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEvent {
    RuntimeReady,
    Log,
    SceneSnapshot,
    FrameStats,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeIpcConfig {
    pub port: u16,
    pub token: u64,
}

impl RuntimeIpcConfig {
    pub const fn new(port: u16, token: u64) -> Self {
        Self { port, token }
    }
}

pub fn encode_line<T: Serialize>(message: &T) -> serde_json::Result<String> {
    let mut line = serde_json::to_string(message)?;
    line.push('\n');
    Ok(line)
}

pub fn decode_client_message(line: &str) -> serde_json::Result<ClientMessage> {
    serde_json::from_str(line.trim_end())
}

pub fn decode_runtime_message(line: &str) -> serde_json::Result<RuntimeMessage> {
    serde_json::from_str(line.trim_end())
}

pub fn runtime_ready_message() -> RuntimeMessage {
    RuntimeMessage::Event {
        event: RuntimeEvent::RuntimeReady,
        payload: serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_runtime_ready_as_json_line() {
        let line = encode_line(&runtime_ready_message()).unwrap();

        assert!(line.ends_with('\n'));
        assert!(line.contains("\"type\":\"event\""));
        assert!(line.contains("\"event\":\"runtime_ready\""));
    }

    #[test]
    fn decodes_client_hello() {
        let message =
            decode_client_message(r#"{"type":"hello","protocol_version":1,"token":42}"#).unwrap();

        assert_eq!(
            message,
            ClientMessage::Hello {
                protocol_version: PROTOCOL_VERSION,
                token: 42
            }
        );
    }
}
