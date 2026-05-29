use crate::IpcError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const METHOD_OPEN_PROJECT: &str = "open_project";
pub const METHOD_RELOAD_SCENE: &str = "reload_scene";
pub const METHOD_SELECT_ENTITY: &str = "select_entity";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCommand {
    OpenProject(OpenProjectParams),
    ReloadScene,
    SelectEntity(SelectEntityParams),
}

impl RuntimeCommand {
    pub fn method(&self) -> &'static str {
        match self {
            RuntimeCommand::OpenProject(_) => METHOD_OPEN_PROJECT,
            RuntimeCommand::ReloadScene => METHOD_RELOAD_SCENE,
            RuntimeCommand::SelectEntity(_) => METHOD_SELECT_ENTITY,
        }
    }

    pub fn params(&self) -> Value {
        match self {
            RuntimeCommand::OpenProject(params) => json!(params),
            RuntimeCommand::ReloadScene => json!({}),
            RuntimeCommand::SelectEntity(params) => json!(params),
        }
    }

    pub fn from_method_params(method: &str, params: Value) -> Result<Self, IpcError> {
        match method {
            METHOD_OPEN_PROJECT => serde_json::from_value(params)
                .map(RuntimeCommand::OpenProject)
                .map_err(|error| IpcError::invalid_params(method, error)),
            METHOD_RELOAD_SCENE => Ok(RuntimeCommand::ReloadScene),
            METHOD_SELECT_ENTITY => serde_json::from_value(params)
                .map(RuntimeCommand::SelectEntity)
                .map_err(|error| IpcError::invalid_params(method, error)),
            _ => Err(IpcError::unsupported_command(method)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenProjectParams {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectEntityParams {
    pub entity_id: String,
}
