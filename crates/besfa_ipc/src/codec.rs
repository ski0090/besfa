use crate::{
    ClientMessage, EditorCameraStatePayload, FrameStatsPayload, IpcError, LogPayload,
    PreviewSurfacePayload, RuntimeCommand, RuntimeEvent, RuntimeMessage, SceneSnapshotPayload,
};
use serde::Serialize;
use serde_json::{Value, json};

/// Protocol version expected by both the editor and runtime.
pub const PROTOCOL_VERSION: u32 = 1;

/// Serializes a message as one newline-delimited JSON line.
pub fn encode_line<T: Serialize>(message: &T) -> serde_json::Result<String> {
    let mut line = serde_json::to_string(message)?;
    line.push('\n');
    Ok(line)
}

/// Decodes a client-to-runtime message from one JSON line.
pub fn decode_client_message(line: &str) -> serde_json::Result<ClientMessage> {
    serde_json::from_str(line.trim_end())
}

/// Decodes a runtime-to-client message from one JSON line.
pub fn decode_runtime_message(line: &str) -> serde_json::Result<RuntimeMessage> {
    serde_json::from_str(line.trim_end())
}

/// Wraps a typed runtime command in a protocol `command` message.
pub fn command_message(id: u64, command: RuntimeCommand) -> ClientMessage {
    ClientMessage::Command {
        id,
        method: command.method().to_string(),
        params: command.params(),
    }
}

/// Builds the event sent after a successful runtime handshake.
pub fn runtime_ready_message() -> RuntimeMessage {
    RuntimeMessage::Event {
        event: RuntimeEvent::RuntimeReady,
        payload: json!({
            "protocol_version": PROTOCOL_VERSION,
        }),
    }
}

/// Builds a runtime log event.
pub fn log_message(level: impl Into<String>, message: impl Into<String>) -> RuntimeMessage {
    RuntimeMessage::Event {
        event: RuntimeEvent::Log,
        payload: json!(LogPayload {
            level: level.into(),
            message: message.into(),
        }),
    }
}

/// Builds a scene hierarchy snapshot event.
pub fn scene_snapshot_message(payload: SceneSnapshotPayload) -> RuntimeMessage {
    RuntimeMessage::Event {
        event: RuntimeEvent::SceneSnapshot,
        payload: json!(payload),
    }
}

/// Builds a frame statistics event.
pub fn frame_stats_message(payload: FrameStatsPayload) -> RuntimeMessage {
    RuntimeMessage::Event {
        event: RuntimeEvent::FrameStats,
        payload: json!(payload),
    }
}

/// Builds a preview surface descriptor event.
pub fn preview_surface_ready_message(payload: PreviewSurfacePayload) -> RuntimeMessage {
    RuntimeMessage::Event {
        event: RuntimeEvent::PreviewSurfaceReady,
        payload: json!(payload),
    }
}

/// Builds a selected camera preview surface descriptor event.
pub fn camera_preview_surface_ready_message(payload: PreviewSurfacePayload) -> RuntimeMessage {
    RuntimeMessage::Event {
        event: RuntimeEvent::CameraPreviewSurfaceReady,
        payload: json!(payload),
    }
}

/// Builds an editor preview camera orientation event.
pub fn editor_camera_state_message(payload: EditorCameraStatePayload) -> RuntimeMessage {
    RuntimeMessage::Event {
        event: RuntimeEvent::EditorCameraState,
        payload: json!(payload),
    }
}

/// Builds a successful command response with a JSON result payload.
pub fn ok_response(id: u64, result: Value) -> RuntimeMessage {
    RuntimeMessage::Response {
        id,
        ok: true,
        result: Some(result),
        error: None,
    }
}

/// Builds a successful command response with an empty JSON object result.
pub fn empty_ok_response(id: u64) -> RuntimeMessage {
    ok_response(id, json!({}))
}

/// Builds a failed command response.
pub fn error_response(id: u64, error: IpcError) -> RuntimeMessage {
    RuntimeMessage::Response {
        id,
        ok: false,
        result: None,
        error: Some(error),
    }
}
