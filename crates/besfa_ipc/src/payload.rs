use serde::{Deserialize, Serialize};

/// Payload for a runtime scene hierarchy snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneSnapshotPayload {
    /// Root entity for the hierarchy tree.
    pub root: SceneEntityPayload,
    /// Optional id of the currently selected entity.
    #[serde(default)]
    pub selected_entity_id: Option<String>,
}

/// Serializable runtime entity node used by scene snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneEntityPayload {
    /// Stable runtime entity id.
    pub id: String,
    /// Display name shown by the editor.
    pub name: String,
    /// Lightweight kind hint used for editor icons and grouping.
    pub kind: String,
    /// Optional transform metadata for editor inspection and placement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<SceneTransformPayload>,
    /// Child entities in hierarchy order.
    #[serde(default)]
    pub children: Vec<SceneEntityPayload>,
}

/// Transform metadata for a runtime scene entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneTransformPayload {
    /// Local translation in runtime world units.
    pub translation: Vec3Payload,
}

/// Three-dimensional numeric vector payload.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3Payload {
    /// X axis component.
    pub x: f32,
    /// Y axis component.
    pub y: f32,
    /// Z axis component.
    pub z: f32,
}

/// Runtime frame timing telemetry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameStatsPayload {
    /// Estimated frames per second.
    pub fps: f64,
    /// Average frame time in milliseconds.
    pub frame_time_ms: f64,
}

/// Runtime log event payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogPayload {
    /// Log level string such as `info` or `error`.
    pub level: String,
    /// Human-readable log message.
    pub message: String,
}

/// Descriptor for a runtime-owned shared preview surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewSurfacePayload {
    /// Native shared handle name that the editor can open on Windows.
    pub shared_handle_name: String,
    /// Surface width in physical pixels.
    pub width: u32,
    /// Surface height in physical pixels.
    pub height: u32,
    /// Texture format name used by the runtime and editor.
    pub format: String,
}
