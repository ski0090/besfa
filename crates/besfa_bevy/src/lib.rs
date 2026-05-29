use besfa_core::EngineInfo;

#[cfg(feature = "runtime")]
mod preview;
#[cfg(feature = "runtime")]
mod runtime_ipc;

#[cfg(feature = "runtime")]
pub use besfa_ipc::RuntimeIpcConfig;
#[cfg(feature = "runtime")]
pub use preview::BesfaPreviewPlugin;
#[cfg(feature = "runtime")]
pub use runtime_ipc::BesfaRuntimeIpcPlugin;

/// Returns the display name for the Bevy integration layer.
pub fn integration_name() -> String {
    let info = EngineInfo::current();
    format!("{} Bevy integration", info.name)
}

#[cfg(feature = "runtime")]
/// Runs the standalone preview runtime with default options.
pub fn run_preview_runtime() {
    run_preview_runtime_with_options(PreviewRuntimeOptions::default());
}

#[cfg(feature = "runtime")]
/// Runs the standalone preview runtime with explicit options.
pub fn run_preview_runtime_with_options(options: PreviewRuntimeOptions) {
    preview::run(options);
}

#[cfg(feature = "runtime")]
/// Options used when launching the preview runtime Bevy app.
#[derive(Debug, Clone, Copy, Default)]
pub struct PreviewRuntimeOptions {
    /// Optional IPC configuration used to accept editor connections.
    pub ipc: Option<RuntimeIpcConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_integration_name() {
        assert_eq!(integration_name(), "Besfa Bevy integration");
    }
}
