#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeIpcConfig {
    pub port: u16,
    pub token: u64,
}

impl RuntimeIpcConfig {
    pub const fn new(port: u16, token: u64) -> Self {
        Self { port, token }
    }
}
