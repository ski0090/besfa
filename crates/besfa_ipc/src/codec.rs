use crate::{
    ClientMessage, FrameStatsPayload, IpcError, LogPayload, RuntimeCommand, RuntimeEvent,
    RuntimeMessage, SceneSnapshotPayload,
};
use serde::Serialize;
use serde_json::{Value, json};

pub const PROTOCOL_VERSION: u32 = 1;

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

pub fn command_message(id: u64, command: RuntimeCommand) -> ClientMessage {
    ClientMessage::Command {
        id,
        method: command.method().to_string(),
        params: command.params(),
    }
}

pub fn runtime_ready_message() -> RuntimeMessage {
    RuntimeMessage::Event {
        event: RuntimeEvent::RuntimeReady,
        payload: json!({
            "protocol_version": PROTOCOL_VERSION,
        }),
    }
}

pub fn log_message(level: impl Into<String>, message: impl Into<String>) -> RuntimeMessage {
    RuntimeMessage::Event {
        event: RuntimeEvent::Log,
        payload: json!(LogPayload {
            level: level.into(),
            message: message.into(),
        }),
    }
}

pub fn scene_snapshot_message(payload: SceneSnapshotPayload) -> RuntimeMessage {
    RuntimeMessage::Event {
        event: RuntimeEvent::SceneSnapshot,
        payload: json!(payload),
    }
}

pub fn frame_stats_message(payload: FrameStatsPayload) -> RuntimeMessage {
    RuntimeMessage::Event {
        event: RuntimeEvent::FrameStats,
        payload: json!(payload),
    }
}

pub fn ok_response(id: u64, result: Value) -> RuntimeMessage {
    RuntimeMessage::Response {
        id,
        ok: true,
        result: Some(result),
        error: None,
    }
}

pub fn empty_ok_response(id: u64) -> RuntimeMessage {
    ok_response(id, json!({}))
}

pub fn error_response(id: u64, error: IpcError) -> RuntimeMessage {
    RuntimeMessage::Response {
        id,
        ok: false,
        result: None,
        error: Some(error),
    }
}
