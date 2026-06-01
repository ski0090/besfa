mod codec;
mod command;
mod config;
mod error;
mod message;
mod payload;

pub use codec::{
    PROTOCOL_VERSION, command_message, decode_client_message, decode_runtime_message,
    empty_ok_response, encode_line, error_response, frame_stats_message, log_message, ok_response,
    preview_surface_ready_message, runtime_ready_message, scene_snapshot_message,
};
pub use command::{
    CreateEntityParams, CreateEntityResult, METHOD_CREATE_ENTITY, METHOD_OPEN_PROJECT,
    METHOD_RELOAD_SCENE, METHOD_SELECT_ENTITY, OpenProjectParams, RuntimeCommand,
    SelectEntityParams,
};
pub use config::RuntimeIpcConfig;
pub use error::IpcError;
pub use message::{ClientMessage, RuntimeEvent, RuntimeMessage};
pub use payload::{
    FrameStatsPayload, LogPayload, PreviewSurfacePayload, SceneEntityPayload, SceneSnapshotPayload,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    #[test]
    fn encodes_select_entity_command() {
        let line = encode_line(&command_message(
            7,
            RuntimeCommand::SelectEntity(SelectEntityParams {
                entity_id: "camera".to_string(),
            }),
        ))
        .unwrap();

        assert!(line.contains("\"method\":\"select_entity\""));
        assert!(line.contains("\"entity_id\":\"camera\""));
    }

    #[test]
    fn encodes_create_entity_command() {
        let line = encode_line(&command_message(
            8,
            RuntimeCommand::CreateEntity(CreateEntityParams {
                kind: "cube".to_string(),
                name: Some("Cube".to_string()),
                parent_entity_id: Some("world".to_string()),
            }),
        ))
        .unwrap();

        assert!(line.contains("\"method\":\"create_entity\""));
        assert!(line.contains("\"kind\":\"cube\""));
        assert!(line.contains("\"parent_entity_id\":\"world\""));
    }

    #[test]
    fn decodes_runtime_command() {
        let command = RuntimeCommand::from_method_params(
            METHOD_OPEN_PROJECT,
            json!({ "path": "C:/codes/besfa" }),
        )
        .unwrap();

        assert_eq!(
            command,
            RuntimeCommand::OpenProject(OpenProjectParams {
                path: "C:/codes/besfa".to_string(),
            })
        );
    }

    #[test]
    fn encodes_scene_snapshot_event() {
        let line = encode_line(&scene_snapshot_message(SceneSnapshotPayload {
            selected_entity_id: Some("camera".to_string()),
            root: SceneEntityPayload {
                id: "world".to_string(),
                name: "World".to_string(),
                kind: "world".to_string(),
                children: vec![],
            },
        }))
        .unwrap();

        assert!(line.contains("\"event\":\"scene_snapshot\""));
        assert!(line.contains("\"selected_entity_id\":\"camera\""));
    }

    #[test]
    fn encodes_preview_surface_ready_event() {
        let line = encode_line(&preview_surface_ready_message(PreviewSurfacePayload {
            shared_handle_name: "Local\\BesfaPreviewSurface-42".to_string(),
            width: 640,
            height: 360,
            format: "bgra8_unorm".to_string(),
        }))
        .unwrap();

        assert!(line.contains("\"event\":\"preview_surface_ready\""));
        assert!(line.contains("\"shared_handle_name\""));
    }
}
