mod resources;
mod snapshot;
mod systems;
mod transport;

use besfa_ipc::RuntimeIpcConfig;
use bevy::prelude::*;
use resources::{
    RuntimeIpcFrameStats, RuntimeIpcProject, RuntimeIpcSelection, RuntimeIpcServerConfig,
    RuntimeIpcSnapshotCursor,
};
use systems::{emit_frame_stats, emit_requested_scene_snapshot, process_runtime_ipc_commands};
use transport::start_runtime_ipc_server;

pub(crate) use resources::RuntimeIpcServer;

/// Bevy plugin that exposes runtime IPC commands and events over TCP.
pub struct BesfaRuntimeIpcPlugin {
    config: RuntimeIpcConfig,
}

impl BesfaRuntimeIpcPlugin {
    /// Creates the runtime IPC plugin for a specific launch configuration.
    pub const fn new(config: RuntimeIpcConfig) -> Self {
        Self { config }
    }
}

impl Plugin for BesfaRuntimeIpcPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(RuntimeIpcServerConfig(self.config))
            .insert_resource(RuntimeIpcServer::new())
            .init_resource::<RuntimeIpcProject>()
            .init_resource::<RuntimeIpcSelection>()
            .init_resource::<RuntimeIpcSnapshotCursor>()
            .init_resource::<RuntimeIpcFrameStats>()
            .add_systems(Startup, start_runtime_ipc_server)
            .add_systems(
                Update,
                (
                    emit_requested_scene_snapshot,
                    process_runtime_ipc_commands,
                    emit_frame_stats,
                )
                    .chain(),
            );
    }
}
