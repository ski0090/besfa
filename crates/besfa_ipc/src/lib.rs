mod codec;
mod command;
mod config;
mod error;
mod message;
mod payload;

pub use codec::{
    PROTOCOL_VERSION, command_message, decode_client_message, decode_runtime_message,
    editor_camera_state_message, empty_ok_response, encode_line, error_response,
    frame_stats_message, log_message, ok_response, preview_surface_ready_message,
    runtime_ready_message, scene_snapshot_message,
};
pub use command::{
    CreateEntityParams, CreateEntityResult, EditorCameraInputParams, METHOD_CREATE_ENTITY,
    METHOD_EDITOR_CAMERA_INPUT, METHOD_OPEN_PROJECT, METHOD_PICK_ENTITY, METHOD_RELOAD_SCENE,
    METHOD_SELECT_ENTITY, METHOD_SET_TRANSFORM, OpenProjectParams, PickEntityParams,
    PickEntityResult, RuntimeCommand, SelectEntityParams, SetTransformParams,
};
pub use config::RuntimeIpcConfig;
pub use error::IpcError;
pub use message::{ClientMessage, RuntimeEvent, RuntimeMessage};
pub use payload::{
    EditorCameraStatePayload, FrameStatsPayload, LogPayload, PreviewSurfacePayload,
    SceneEntityPayload, SceneSnapshotPayload, SceneTransformPayload, Vec3Payload,
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
    fn encodes_pick_entity_command() {
        let line = encode_line(&command_message(
            8,
            RuntimeCommand::PickEntity(PickEntityParams {
                viewport_x: 0.5,
                viewport_y: 0.25,
            }),
        ))
        .unwrap();

        assert!(line.contains("\"method\":\"pick_entity\""));
        assert!(line.contains("\"viewport_x\":0.5"));
        assert!(line.contains("\"viewport_y\":0.25"));
    }

    #[test]
    fn encodes_create_entity_command() {
        let line = encode_line(&command_message(
            9,
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
    fn encodes_set_transform_command() {
        let line = encode_line(&command_message(
            10,
            RuntimeCommand::SetTransform(SetTransformParams {
                entity_id: "cube_1".to_string(),
                translation: Vec3Payload {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
            }),
        ))
        .unwrap();

        assert!(line.contains("\"method\":\"set_transform\""));
        assert!(line.contains("\"entity_id\":\"cube_1\""));
        assert!(line.contains("\"translation\":{\"x\":1.0,\"y\":2.0,\"z\":3.0}"));
    }

    #[test]
    fn encodes_editor_camera_input_command() {
        let line = encode_line(&command_message(
            11,
            RuntimeCommand::EditorCameraInput(EditorCameraInputParams {
                rotate_delta_x: 4.0,
                rotate_delta_y: -2.0,
                move_forward: 1.0,
                move_right: -1.0,
                move_up: 0.0,
                speed_multiplier: 4.0,
                delta_seconds: 0.016,
            }),
        ))
        .unwrap();

        assert!(line.contains("\"method\":\"editor_camera_input\""));
        assert!(line.contains("\"rotate_delta_x\":4.0"));
        assert!(line.contains("\"speed_multiplier\":4.0"));
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
    fn decodes_pick_entity_command() {
        let command = RuntimeCommand::from_method_params(
            METHOD_PICK_ENTITY,
            json!({ "viewport_x": 0.25, "viewport_y": 0.75 }),
        )
        .unwrap();

        assert_eq!(
            command,
            RuntimeCommand::PickEntity(PickEntityParams {
                viewport_x: 0.25,
                viewport_y: 0.75,
            })
        );
    }

    #[test]
    fn decodes_editor_camera_input_command_with_defaults() {
        let command = RuntimeCommand::from_method_params(
            METHOD_EDITOR_CAMERA_INPUT,
            json!({ "move_forward": 1.0 }),
        )
        .unwrap();

        assert_eq!(
            command,
            RuntimeCommand::EditorCameraInput(EditorCameraInputParams {
                rotate_delta_x: 0.0,
                rotate_delta_y: 0.0,
                move_forward: 1.0,
                move_right: 0.0,
                move_up: 0.0,
                speed_multiplier: 1.0,
                delta_seconds: 0.0,
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
                transform: Some(SceneTransformPayload {
                    translation: Vec3Payload {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    },
                }),
                children: vec![],
            },
        }))
        .unwrap();

        assert!(line.contains("\"event\":\"scene_snapshot\""));
        assert!(line.contains("\"selected_entity_id\":\"camera\""));
        assert!(line.contains("\"translation\""));
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

    #[test]
    fn encodes_editor_camera_state_event() {
        let line = encode_line(&editor_camera_state_message(EditorCameraStatePayload {
            right: Vec3Payload {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            up: Vec3Payload {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
            forward: Vec3Payload {
                x: 0.0,
                y: 0.0,
                z: -1.0,
            },
        }))
        .unwrap();

        assert!(line.contains("\"event\":\"editor_camera_state\""));
        assert!(line.contains("\"forward\""));
    }
}
