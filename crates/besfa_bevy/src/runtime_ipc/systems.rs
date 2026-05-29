use super::{
    resources::{
        RuntimeIpcFrameStats, RuntimeIpcProject, RuntimeIpcSelection, RuntimeIpcServer,
        RuntimeIpcSnapshotCursor,
    },
    snapshot::build_scene_snapshot,
};
use crate::preview::PreviewSceneNode;
use besfa_ipc::{
    FrameStatsPayload, IpcError, RuntimeCommand, empty_ok_response, error_response,
    frame_stats_message, log_message, scene_snapshot_message,
};
use bevy::prelude::*;

pub(super) fn process_runtime_ipc_commands(
    server: Res<RuntimeIpcServer>,
    mut project: ResMut<RuntimeIpcProject>,
    mut selection: ResMut<RuntimeIpcSelection>,
    scene_nodes: Query<&PreviewSceneNode>,
) {
    for request in server.drain_commands() {
        match request.command {
            RuntimeCommand::OpenProject(params) => {
                project.path = Some(params.path.clone());
                let _ = request.response_tx.send(empty_ok_response(request.id));
                server.broadcast(log_message(
                    "info",
                    format!("Opened project {}", params.path),
                ));
                server.request_snapshot();
            }
            RuntimeCommand::ReloadScene => {
                let _ = request.response_tx.send(empty_ok_response(request.id));
                server.broadcast(log_message("info", "Reloaded preview scene"));
                server.request_snapshot();
            }
            RuntimeCommand::SelectEntity(params) => {
                if scene_nodes.iter().any(|node| node.id == params.entity_id) {
                    selection.selected_entity_id = Some(params.entity_id.clone());
                    let _ = request.response_tx.send(empty_ok_response(request.id));
                    server.request_snapshot();
                } else {
                    let _ = request.response_tx.send(error_response(
                        request.id,
                        IpcError::new(
                            "entity_not_found",
                            format!("Runtime entity was not found: {}", params.entity_id),
                        ),
                    ));
                }
            }
        }
    }
}

pub(super) fn emit_requested_scene_snapshot(
    server: Res<RuntimeIpcServer>,
    mut cursor: ResMut<RuntimeIpcSnapshotCursor>,
    selection: Res<RuntimeIpcSelection>,
    scene_nodes: Query<&PreviewSceneNode>,
) {
    let requested = server.snapshot_request_count();
    if cursor.last_seen == requested {
        return;
    }

    cursor.last_seen = requested;
    if let Some(snapshot) =
        build_scene_snapshot(&scene_nodes, selection.selected_entity_id.as_deref())
    {
        server.broadcast(scene_snapshot_message(snapshot));
    }
}

pub(super) fn emit_frame_stats(
    server: Res<RuntimeIpcServer>,
    mut stats: ResMut<RuntimeIpcFrameStats>,
    time: Res<Time>,
) {
    stats.elapsed_secs += time.delta_secs();
    stats.frames += 1;

    if stats.elapsed_secs < 1.0 || stats.frames == 0 {
        return;
    }

    let fps = stats.frames as f64 / stats.elapsed_secs as f64;
    server.broadcast(frame_stats_message(FrameStatsPayload {
        fps,
        frame_time_ms: 1000.0 / fps,
    }));
    stats.elapsed_secs = 0.0;
    stats.frames = 0;
}
