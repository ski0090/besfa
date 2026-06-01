use super::{
    resources::{
        RuntimeIpcFrameStats, RuntimeIpcProject, RuntimeIpcSelection, RuntimeIpcServer,
        RuntimeIpcSnapshotCursor,
    },
    snapshot::build_scene_snapshot,
};
use crate::preview::{PreviewSceneNode, PreviewSceneObjects};
use besfa_ipc::{
    CreateEntityResult, FrameStatsPayload, IpcError, RuntimeCommand, empty_ok_response,
    error_response, frame_stats_message, log_message, ok_response, scene_snapshot_message,
};
use bevy::prelude::*;
use serde_json::json;

pub(super) fn process_runtime_ipc_commands(
    server: Res<RuntimeIpcServer>,
    mut commands: Commands,
    mut project: ResMut<RuntimeIpcProject>,
    mut selection: ResMut<RuntimeIpcSelection>,
    mut scene_objects: ResMut<PreviewSceneObjects>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    scene_nodes: Query<&PreviewSceneNode>,
    mut transforms: Query<(&PreviewSceneNode, &mut Transform)>,
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
                if scene_nodes
                    .iter()
                    .any(|node| node.id.as_str() == params.entity_id)
                {
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
            RuntimeCommand::CreateEntity(params) => {
                if params.kind != "cube" {
                    let _ = request.response_tx.send(error_response(
                        request.id,
                        IpcError::new(
                            "unsupported_entity_kind",
                            format!("Runtime cannot create entity kind: {}", params.kind),
                        ),
                    ));
                    continue;
                }

                let parent_entity_id = params
                    .parent_entity_id
                    .clone()
                    .unwrap_or_else(|| "world".to_string());
                if !scene_nodes
                    .iter()
                    .any(|node| node.id.as_str() == parent_entity_id)
                {
                    let _ = request.response_tx.send(error_response(
                        request.id,
                        IpcError::new(
                            "parent_entity_not_found",
                            format!("Parent runtime entity was not found: {parent_entity_id}"),
                        ),
                    ));
                    continue;
                }

                let (entity_id, default_name, position) = scene_objects.next_cube();
                let name = params.name.clone().unwrap_or(default_name);
                commands.spawn((
                    Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: Color::srgb(0.35, 0.47, 0.95),
                        metallic: 0.05,
                        perceptual_roughness: 0.6,
                        ..default()
                    })),
                    Transform::from_translation(position),
                    Name::new(name.clone()),
                    PreviewSceneNode::child(
                        entity_id.clone(),
                        name.clone(),
                        "mesh",
                        parent_entity_id,
                    ),
                ));

                selection.selected_entity_id = Some(entity_id.clone());
                let _ = request.response_tx.send(ok_response(
                    request.id,
                    json!(CreateEntityResult {
                        entity_id: entity_id.clone(),
                    }),
                ));
                server.broadcast(log_message("info", format!("Created {name}")));
                server.request_snapshot();
            }
            RuntimeCommand::SetTransform(params) => {
                if let Some((_, mut transform)) = transforms
                    .iter_mut()
                    .find(|(node, _)| node.id.as_str() == params.entity_id)
                {
                    transform.translation = Vec3::new(
                        params.translation.x,
                        params.translation.y,
                        params.translation.z,
                    );
                    selection.selected_entity_id = Some(params.entity_id.clone());
                    let _ = request.response_tx.send(empty_ok_response(request.id));
                    server.request_snapshot();
                } else {
                    let _ = request.response_tx.send(error_response(
                        request.id,
                        IpcError::new(
                            "transform_not_found",
                            format!(
                                "Runtime entity transform was not found: {}",
                                params.entity_id
                            ),
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
    scene_nodes: Query<(&PreviewSceneNode, Option<&Transform>)>,
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
