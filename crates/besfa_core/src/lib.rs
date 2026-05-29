/// Current native ABI version shared by the editor, plugin, and runtime.
pub const ABI_VERSION: u32 = 1;

/// Static metadata describing the Besfa engine build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineInfo {
    /// Human-readable engine name.
    pub name: &'static str,
    /// ABI version expected by native bridge consumers.
    pub abi_version: u32,
}

impl EngineInfo {
    /// Returns metadata for the current engine build.
    pub const fn current() -> Self {
        Self {
            name: "Besfa",
            abi_version: ABI_VERSION,
        }
    }
}

/// Minimal native bridge health-check message.
pub fn bridge_hello() -> &'static str {
    "Besfa bridge is awake"
}
