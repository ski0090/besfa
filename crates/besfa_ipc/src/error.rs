use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

impl IpcError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn unsupported_command(method: &str) -> Self {
        Self::new(
            "unsupported_command",
            format!("Unsupported runtime command: {method}"),
        )
    }

    pub fn invalid_params(method: &str, error: serde_json::Error) -> Self {
        Self::new(
            "invalid_params",
            format!("Invalid params for runtime command {method}: {error}"),
        )
    }
}
