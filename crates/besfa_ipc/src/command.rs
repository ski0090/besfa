use crate::IpcError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Runtime command method name for opening a project.
pub const METHOD_OPEN_PROJECT: &str = "open_project";
/// Runtime command method name for reloading the active scene.
pub const METHOD_RELOAD_SCENE: &str = "reload_scene";
/// Runtime command method name for selecting an entity.
pub const METHOD_SELECT_ENTITY: &str = "select_entity";

/// Typed editor-to-runtime command set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCommand {
    /// Open or switch the runtime to a project path.
    OpenProject(OpenProjectParams),
    /// Reload the current runtime scene.
    ReloadScene,
    /// Select one runtime entity by id.
    SelectEntity(SelectEntityParams),
}

impl RuntimeCommand {
    /// Returns the wire method name for this command.
    pub fn method(&self) -> &'static str {
        match self {
            RuntimeCommand::OpenProject(_) => METHOD_OPEN_PROJECT,
            RuntimeCommand::ReloadScene => METHOD_RELOAD_SCENE,
            RuntimeCommand::SelectEntity(_) => METHOD_SELECT_ENTITY,
        }
    }

    /// Converts command parameters into their JSON wire representation.
    pub fn params(&self) -> Value {
        match self {
            RuntimeCommand::OpenProject(params) => json!(params),
            RuntimeCommand::ReloadScene => json!({}),
            RuntimeCommand::SelectEntity(params) => json!(params),
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
