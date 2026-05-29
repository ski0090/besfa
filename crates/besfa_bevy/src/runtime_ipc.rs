mod resources;
mod snapshot;
mod systems;
mod transport;

use besfa_ipc::RuntimeIpcConfig;
use bevy::prelude::*;
use resources::{
    RuntimeIpcFrameStats, RuntimeIpcProject, RuntimeIpcSelection, RuntimeIpcServer,
    RuntimeIpcServerConfig, RuntimeIpcSnapshotCursor,
};
use systems::{emit_frame_stats, emit_requested_scene_snapshot, process_runtime_ipc_commands};
use transport::start_runtime_ipc_server;

pub struct BesfaRuntimeIpcPlugin {
    config: RuntimeIpcConfig,
}

impl BesfaRuntimeIpcPlugin {
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
                    process_runtime_ipc_commands,
                    emit_requested_scene_snapshot,
                    emit_frame_stats,
                ),
            );
    }
}
