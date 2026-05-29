use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneSnapshotPayload {
    pub root: SceneEntityPayload,
    #[serde(default)]
    pub selected_entity_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneEntityPayload {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub children: Vec<SceneEntityPayload>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameStatsPayload {
    pub fps: f64,
    pub frame_time_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogPayload {
    pub level: String,
    pub message: String,
}
