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
    /// Child entities in hierarchy order.
    #[serde(default)]
    pub children: Vec<SceneEntityPayload>,
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
