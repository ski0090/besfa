pub const ABI_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineInfo {
    pub name: &'static str,
    pub abi_version: u32,
}

impl EngineInfo {
    pub const fn current() -> Self {
        Self {
            name: "Besfa",
            abi_version: ABI_VERSION,
        }
    }
}

pub fn bridge_hello() -> &'static str {
    "Besfa bridge is awake"
}
