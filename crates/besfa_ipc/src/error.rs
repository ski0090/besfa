use serde::{Deserialize, Serialize};

/// Error shape returned in failed runtime command responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcError {
    /// Stable machine-readable error code.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}

impl IpcError {
    /// Creates a protocol error with a code and message.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    /// Creates an error for an unknown command method.
    pub fn unsupported_command(method: &str) -> Self {
        Self::new(
            "unsupported_command",
            format!("Unsupported runtime command: {method}"),
        )
    }

    /// Creates an error for command params that failed to deserialize.
    pub fn invalid_params(method: &str, error: serde_json::Error) -> Self {
        Self::new(
            "invalid_params",
            format!("Invalid params for runtime command {method}: {error}"),
        )
    }
}
