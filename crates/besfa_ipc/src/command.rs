use crate::IpcError;
use crate::Vec3Payload;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Runtime command method name for opening a project.
pub const METHOD_OPEN_PROJECT: &str = "open_project";
/// Runtime command method name for reloading the active scene.
pub const METHOD_RELOAD_SCENE: &str = "reload_scene";
/// Runtime command method name for selecting an entity.
pub const METHOD_SELECT_ENTITY: &str = "select_entity";
/// Runtime command method name for picking an entity from viewport coordinates.
pub const METHOD_PICK_ENTITY: &str = "pick_entity";
/// Runtime command method name for creating an entity.
pub const METHOD_CREATE_ENTITY: &str = "create_entity";
/// Runtime command method name for updating an entity transform.
pub const METHOD_SET_TRANSFORM: &str = "set_transform";
/// Runtime command method name for moving the editor preview camera.
pub const METHOD_EDITOR_CAMERA_INPUT: &str = "editor_camera_input";
/// Runtime command method name for aligning the selected camera to the editor camera.
pub const METHOD_ALIGN_SELECTED_CAMERA_TO_EDITOR: &str = "align_selected_camera_to_editor";
/// Runtime command method name for starting a selected transform axis drag.
pub const METHOD_BEGIN_TRANSFORM_AXIS_DRAG: &str = "begin_transform_axis_drag";
/// Runtime command method name for updating a selected transform axis drag.
pub const METHOD_UPDATE_TRANSFORM_AXIS_DRAG: &str = "update_transform_axis_drag";
/// Runtime command method name for ending a selected transform axis drag.
pub const METHOD_END_TRANSFORM_AXIS_DRAG: &str = "end_transform_axis_drag";

/// Typed editor-to-runtime command set.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeCommand {
    /// Open or switch the runtime to a project path.
    OpenProject(OpenProjectParams),
    /// Reload the current runtime scene.
    ReloadScene,
    /// Select one runtime entity by id.
    SelectEntity(SelectEntityParams),
    /// Pick and select a runtime entity from normalized viewport coordinates.
    PickEntity(PickEntityParams),
    /// Create a runtime entity in the active scene.
    CreateEntity(CreateEntityParams),
    /// Update a runtime entity transform.
    SetTransform(SetTransformParams),
    /// Apply editor-only camera navigation input to the preview camera.
    EditorCameraInput(EditorCameraInputParams),
    /// Copy the editor preview camera transform into the selected scene camera.
    AlignSelectedCameraToEditor,
    /// Start dragging the selected entity along one local transform axis.
    BeginTransformAxisDrag(TransformAxisDragViewportParams),
    /// Update the active selected entity transform axis drag.
    UpdateTransformAxisDrag(TransformAxisDragViewportParams),
    /// End the active selected entity transform axis drag.
    EndTransformAxisDrag,
}

impl RuntimeCommand {
    /// Returns the wire method name for this command.
    pub fn method(&self) -> &'static str {
        match self {
            RuntimeCommand::OpenProject(_) => METHOD_OPEN_PROJECT,
            RuntimeCommand::ReloadScene => METHOD_RELOAD_SCENE,
            RuntimeCommand::SelectEntity(_) => METHOD_SELECT_ENTITY,
            RuntimeCommand::PickEntity(_) => METHOD_PICK_ENTITY,
            RuntimeCommand::CreateEntity(_) => METHOD_CREATE_ENTITY,
            RuntimeCommand::SetTransform(_) => METHOD_SET_TRANSFORM,
            RuntimeCommand::EditorCameraInput(_) => METHOD_EDITOR_CAMERA_INPUT,
            RuntimeCommand::AlignSelectedCameraToEditor => METHOD_ALIGN_SELECTED_CAMERA_TO_EDITOR,
            RuntimeCommand::BeginTransformAxisDrag(_) => METHOD_BEGIN_TRANSFORM_AXIS_DRAG,
            RuntimeCommand::UpdateTransformAxisDrag(_) => METHOD_UPDATE_TRANSFORM_AXIS_DRAG,
            RuntimeCommand::EndTransformAxisDrag => METHOD_END_TRANSFORM_AXIS_DRAG,
        }
    }

    /// Converts command parameters into their JSON wire representation.
    pub fn params(&self) -> Value {
        match self {
            RuntimeCommand::OpenProject(params) => json!(params),
            RuntimeCommand::ReloadScene => json!({}),
            RuntimeCommand::SelectEntity(params) => json!(params),
            RuntimeCommand::PickEntity(params) => json!(params),
            RuntimeCommand::CreateEntity(params) => json!(params),
            RuntimeCommand::SetTransform(params) => json!(params),
            RuntimeCommand::EditorCameraInput(params) => json!(params),
            RuntimeCommand::AlignSelectedCameraToEditor => json!({}),
            RuntimeCommand::BeginTransformAxisDrag(params) => json!(params),
            RuntimeCommand::UpdateTransformAxisDrag(params) => json!(params),
            RuntimeCommand::EndTransformAxisDrag => json!({}),
        }
    }

    /// Decodes a typed command from a wire method and JSON params value.
    pub fn from_method_params(method: &str, params: Value) -> Result<Self, IpcError> {
        match method {
            METHOD_OPEN_PROJECT => serde_json::from_value(params)
                .map(RuntimeCommand::OpenProject)
                .map_err(|error| IpcError::invalid_params(method, error)),
            METHOD_RELOAD_SCENE => Ok(RuntimeCommand::ReloadScene),
            METHOD_SELECT_ENTITY => serde_json::from_value(params)
                .map(RuntimeCommand::SelectEntity)
                .map_err(|error| IpcError::invalid_params(method, error)),
            METHOD_PICK_ENTITY => serde_json::from_value(params)
                .map(RuntimeCommand::PickEntity)
                .map_err(|error| IpcError::invalid_params(method, error)),
            METHOD_CREATE_ENTITY => serde_json::from_value(params)
                .map(RuntimeCommand::CreateEntity)
                .map_err(|error| IpcError::invalid_params(method, error)),
            METHOD_SET_TRANSFORM => serde_json::from_value(params)
                .map(RuntimeCommand::SetTransform)
                .map_err(|error| IpcError::invalid_params(method, error)),
            METHOD_EDITOR_CAMERA_INPUT => serde_json::from_value(params)
                .map(RuntimeCommand::EditorCameraInput)
                .map_err(|error| IpcError::invalid_params(method, error)),
            METHOD_ALIGN_SELECTED_CAMERA_TO_EDITOR => {
                Ok(RuntimeCommand::AlignSelectedCameraToEditor)
            }
            METHOD_BEGIN_TRANSFORM_AXIS_DRAG => serde_json::from_value(params)
                .map(RuntimeCommand::BeginTransformAxisDrag)
                .map_err(|error| IpcError::invalid_params(method, error)),
            METHOD_UPDATE_TRANSFORM_AXIS_DRAG => serde_json::from_value(params)
                .map(RuntimeCommand::UpdateTransformAxisDrag)
                .map_err(|error| IpcError::invalid_params(method, error)),
            METHOD_END_TRANSFORM_AXIS_DRAG => Ok(RuntimeCommand::EndTransformAxisDrag),
            _ => Err(IpcError::unsupported_command(method)),
        }
    }
}

/// Parameters for the `open_project` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenProjectParams {
    /// Project root path as understood by the editor.
    pub path: String,
}

/// Parameters for the `select_entity` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectEntityParams {
    /// Stable runtime entity id to select.
    pub entity_id: String,
}

/// Parameters for the `pick_entity` command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PickEntityParams {
    /// Horizontal viewport coordinate normalized from left `0.0` to right `1.0`.
    pub viewport_x: f32,
    /// Vertical viewport coordinate normalized from top `0.0` to bottom `1.0`.
    pub viewport_y: f32,
}

/// Result payload returned by a successful `pick_entity` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickEntityResult {
    /// Stable runtime entity id selected by the pick, or `None` when nothing was hit.
    pub entity_id: Option<String>,
}

/// Parameters for the `create_entity` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEntityParams {
    /// Runtime-supported entity kind, such as `cube`.
    pub kind: String,
    /// Optional display name. The runtime generates one when omitted.
    #[serde(default)]
    pub name: Option<String>,
    /// Optional parent entity id. The runtime uses the world root when omitted.
    #[serde(default)]
    pub parent_entity_id: Option<String>,
}

/// Result payload returned by a successful `create_entity` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEntityResult {
    /// Stable runtime entity id assigned by the runtime.
    pub entity_id: String,
}

/// Parameters for the `set_transform` command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetTransformParams {
    /// Stable runtime entity id to update.
    pub entity_id: String,
    /// New local translation in runtime world units.
    pub translation: Vec3Payload,
}

/// Parameters for the `editor_camera_input` command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorCameraInputParams {
    /// Horizontal pointer delta in editor logical pixels.
    #[serde(default)]
    pub rotate_delta_x: f32,
    /// Vertical pointer delta in editor logical pixels.
    #[serde(default)]
    pub rotate_delta_y: f32,
    /// Local forward movement intent, usually in the `-1.0..=1.0` range.
    #[serde(default)]
    pub move_forward: f32,
    /// Local right movement intent, usually in the `-1.0..=1.0` range.
    #[serde(default)]
    pub move_right: f32,
    /// World-up movement intent, usually in the `-1.0..=1.0` range.
    #[serde(default)]
    pub move_up: f32,
    /// Movement speed multiplier used for accelerated flythrough controls.
    #[serde(default = "default_editor_camera_speed_multiplier")]
    pub speed_multiplier: f32,
    /// Editor-side elapsed time for movement input, in seconds.
    #[serde(default)]
    pub delta_seconds: f32,
}

fn default_editor_camera_speed_multiplier() -> f32 {
    1.0
}

/// Local transform axis selected for viewport gizmo dragging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransformAxis {
    /// Local X axis.
    X,
    /// Local Y axis.
    Y,
    /// Local Z axis.
    Z,
}

/// Viewport coordinates for transform axis drag commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformAxisDragViewportParams {
    /// Horizontal viewport coordinate normalized from left `0.0` to right `1.0`.
    pub viewport_x: f32,
    /// Vertical viewport coordinate normalized from top `0.0` to bottom `1.0`.
    pub viewport_y: f32,
}

/// Result payload returned when transform axis dragging begins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformAxisDragStartResult {
    /// Axis that was hit by the pointer, or `None` when no axis was close enough.
    pub axis: Option<TransformAxis>,
}

/// Result payload returned when transform axis dragging updates the entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformAxisDragUpdateResult {
    /// Updated local translation in runtime world units.
    pub translation: Vec3Payload,
}
