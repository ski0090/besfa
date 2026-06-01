use super::{
    resources::{
        RuntimeIpcFrameStats, RuntimeIpcProject, RuntimeIpcSelection, RuntimeIpcServer,
        RuntimeIpcSnapshotCursor,
    },
    snapshot::build_scene_snapshot,
};
use crate::{
    external_preview::{PREVIEW_SURFACE_HEIGHT, PREVIEW_SURFACE_WIDTH},
    preview::{EditorPreviewCamera, PreviewPickTarget, PreviewSceneNode, PreviewSceneObjects},
};
use besfa_ipc::{
    CreateEntityResult, EditorCameraInputParams, FrameStatsPayload, IpcError, PickEntityParams,
    PickEntityResult, RuntimeCommand, empty_ok_response, error_response, frame_stats_message,
    log_message, ok_response, scene_snapshot_message,
};
use bevy::math::bounding::{Aabb3d, RayCast3d};
use bevy::prelude::*;
use serde_json::json;

const LOCAL_AXIS_LENGTH: f32 = 1.5;
const LOCAL_AXIS_TIP_LENGTH: f32 = 0.18;
const EDITOR_CAMERA_ROTATE_SENSITIVITY: f32 = 0.006;
const EDITOR_CAMERA_BASE_SPEED: f32 = 5.0;
const EDITOR_CAMERA_MAX_DELTA_SECONDS: f32 = 0.1;

pub(super) fn process_runtime_ipc_commands(
    server: Res<RuntimeIpcServer>,
    mut commands: Commands,
    mut project: ResMut<RuntimeIpcProject>,
    mut selection: ResMut<RuntimeIpcSelection>,
    mut scene_objects: ResMut<PreviewSceneObjects>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    scene_nodes: Query<&PreviewSceneNode>,
    mut transforms: Query<(&PreviewSceneNode, &mut Transform), Without<EditorPreviewCamera>>,
    cameras: Query<(&Camera, &GlobalTransform), With<EditorPreviewCamera>>,
    mut editor_cameras: Query<&mut Transform, With<EditorPreviewCamera>>,
    pick_targets: Query<(&PreviewSceneNode, &GlobalTransform, &PreviewPickTarget)>,
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
            RuntimeCommand::PickEntity(params) => {
                match pick_entity_from_viewport(&params, &cameras, &pick_targets) {
                    Ok(entity_id) => {
                        selection.selected_entity_id = entity_id.clone();
                        let _ = request.response_tx.send(ok_response(
                            request.id,
                            json!(PickEntityResult { entity_id }),
                        ));
                        server.request_snapshot();
                    }
                    Err(error) => {
                        let _ = request.response_tx.send(error_response(request.id, error));
                    }
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
                    PreviewPickTarget {
                        half_extents: Vec3::splat(0.5),
                    },
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
            RuntimeCommand::EditorCameraInput(params) => {
                if let Some(mut transform) = editor_cameras.iter_mut().next() {
                    apply_editor_camera_input(&params, &mut transform);
                    let _ = request.response_tx.send(empty_ok_response(request.id));
                } else {
                    let _ = request.response_tx.send(error_response(
                        request.id,
                        IpcError::new(
                            "editor_camera_not_found",
                            "Runtime editor preview camera was not found.",
                        ),
                    ));
                }
            }
        }
    }
}

pub(super) fn draw_selected_local_axes(
    mut gizmos: Gizmos,
    selection: Res<RuntimeIpcSelection>,
    scene_nodes: Query<(&PreviewSceneNode, &Transform)>,
) {
    let Some(selected_entity_id) = selection.selected_entity_id.as_deref() else {
        return;
    };
    let Some((_, transform)) = scene_nodes
        .iter()
        .find(|(node, _)| node.id.as_str() == selected_entity_id)
    else {
        return;
    };

    let origin = transform.translation;
    draw_local_axis(
        &mut gizmos,
        origin,
        transform.rotation * Vec3::X,
        Color::srgb(0.95, 0.16, 0.12),
    );
    draw_local_axis(
        &mut gizmos,
        origin,
        transform.rotation * Vec3::Y,
        Color::srgb(0.25, 0.86, 0.32),
    );
    draw_local_axis(
        &mut gizmos,
        origin,
        transform.rotation * Vec3::Z,
        Color::srgb(0.22, 0.48, 1.0),
    );
}

fn draw_local_axis(gizmos: &mut Gizmos, origin: Vec3, axis: Vec3, color: Color) {
    gizmos
        .arrow(
            origin,
            origin + axis.normalize_or_zero() * LOCAL_AXIS_LENGTH,
            color,
        )
        .with_tip_length(LOCAL_AXIS_TIP_LENGTH);
}

fn apply_editor_camera_input(params: &EditorCameraInputParams, transform: &mut Transform) {
    let rotate_delta_x = finite_or_zero(params.rotate_delta_x).clamp(-500.0, 500.0);
    let rotate_delta_y = finite_or_zero(params.rotate_delta_y).clamp(-500.0, 500.0);

    if rotate_delta_x != 0.0 {
        transform.rotate_y(-rotate_delta_x * EDITOR_CAMERA_ROTATE_SENSITIVITY);
    }
    if rotate_delta_y != 0.0 {
        transform.rotate_local_x(-rotate_delta_y * EDITOR_CAMERA_ROTATE_SENSITIVITY);
    }
    transform.rotation = transform.rotation.normalize();

    let movement = *transform.forward() * finite_or_zero(params.move_forward).clamp(-1.0, 1.0)
        + *transform.right() * finite_or_zero(params.move_right).clamp(-1.0, 1.0)
        + Vec3::Y * finite_or_zero(params.move_up).clamp(-1.0, 1.0);
    if movement == Vec3::ZERO {
        return;
    }

    let delta_seconds = finite_or_zero(params.delta_seconds)
        .max(0.0)
        .min(EDITOR_CAMERA_MAX_DELTA_SECONDS);
    let speed_multiplier = finite_or_zero(params.speed_multiplier).max(0.1).min(8.0);
    transform.translation +=
        movement.normalize() * EDITOR_CAMERA_BASE_SPEED * speed_multiplier * delta_seconds;
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn pick_entity_from_viewport(
    params: &PickEntityParams,
    cameras: &Query<(&Camera, &GlobalTransform), With<EditorPreviewCamera>>,
    pick_targets: &Query<(&PreviewSceneNode, &GlobalTransform, &PreviewPickTarget)>,
) -> Result<Option<String>, IpcError> {
    if !params.viewport_x.is_finite() || !params.viewport_y.is_finite() {
        return Err(IpcError::new(
            "invalid_pick_coordinates",
            "Viewport pick coordinates must be finite.",
        ));
    }

    let Some((camera, camera_transform)) = cameras.iter().next() else {
        return Err(IpcError::new(
            "camera_not_found",
            "Runtime preview camera was not found.",
        ));
    };

    let viewport_position = Vec2::new(
        params.viewport_x.clamp(0.0, 1.0) * PREVIEW_SURFACE_WIDTH as f32,
        params.viewport_y.clamp(0.0, 1.0) * PREVIEW_SURFACE_HEIGHT as f32,
    );
    let ray = camera
        .viewport_to_world(camera_transform, viewport_position)
        .map_err(|_| {
            IpcError::new(
                "pick_ray_failed",
                "Viewport pick ray could not be computed.",
            )
        })?;
    let raycast = RayCast3d::from_ray(ray, 10_000.0);

    let mut nearest_hit: Option<(f32, String)> = None;
    for (node, transform, pick_target) in pick_targets.iter() {
        let bounds = Aabb3d::new(transform.translation(), pick_target.half_extents);
        let Some(distance) = raycast.aabb_intersection_at(&bounds) else {
            continue;
        };

        if nearest_hit
            .as_ref()
            .is_none_or(|(nearest_distance, _)| distance < *nearest_distance)
        {
            nearest_hit = Some((distance, node.id.clone()));
        }
    }

    Ok(nearest_hit.map(|(_, entity_id)| entity_id))
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
