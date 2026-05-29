/// Launch-time IPC settings shared between the editor and runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeIpcConfig {
    /// Localhost TCP port the runtime should bind.
    pub port: u16,
    /// Random token the editor must echo in the handshake.
    pub token: u64,
}

impl RuntimeIpcConfig {
    /// Creates a runtime IPC configuration.
    pub const fn new(port: u16, token: u64) -> Self {
        Self { port, token }
    }
}
