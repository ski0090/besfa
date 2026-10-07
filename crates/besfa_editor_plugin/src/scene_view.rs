//! The editor's view of the scene in edit mode: a camera that orbits, pans
//! and zooms, click selection, a ground grid, the selection's bounds, and
//! handles that move, rotate and scale it.
//!
//! The editor forwards the viewport's pointer in viewport pixels as
//! `pointer` commands. The editor camera renders on top of the game's
//! cameras, which keep their settings so saving never sees a change. When
//! the game plays, the editor camera goes and everything here stops.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::{
    camera::primitives::Aabb,
    picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings},
    prelude::*,
    reflect::{TypePath, serde::TypedReflectSerializer},
    transform::TransformSystems,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::inspect::{Selected, report_selection, send_report};

/// Kept out of the Hierarchy and the scene file.
#[derive(Component)]
pub struct EditorOnly;

/// What dragging a handle does to the selected entity.
#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Tool {
    #[default]
    Translate,
    Rotate,
    Scale,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Button {
    Left,
    Middle,
    Right,
}

/// One pointer event from the viewport, in viewport pixels. Scrolling is in
/// pixels too: positive scrolls down, which zooms out.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub(crate) enum PointerEvent {
    Down { x: f32, y: f32, button: Button },
    Move { x: f32, y: f32 },
    Up { x: f32, y: f32, button: Button },
    Scroll { delta: f32 },
}

/// The scene view's state, fed by the editor's commands.
#[derive(Resource)]
pub(crate) struct SceneView {
    /// Off once the game plays.
    pub(crate) active: bool,
    pub(crate) tool: Tool,
    /// Pointer events since the last frame.
    pub(crate) events: Vec<PointerEvent>,
    /// Frame the selected entity on the next frame.
    pub(crate) focus: bool,
    pointer: Vec2,
    /// The button held down and where it went down.
    pressed: Option<(Button, Vec2)>,
    /// The handle under the pointer, for highlighting.
    hovered: Option<usize>,
    drag: Option<Drag>,
}

impl Default for SceneView {
    fn default() -> Self {
        Self {
            active: true,
            tool: Tool::default(),
            events: Vec::new(),
            focus: false,
            pointer: Vec2::ZERO,
            pressed: None,
            hovered: None,
            drag: None,
        }
    }
}

/// A handle being dragged.
struct Drag {
    entity: Entity,
    axis: usize,
    /// The handle's world direction and origin when the drag began.
    direction: Vec3,
    origin: Vec3,
    length: f32,
    start: Transform,
    /// Where the pointer started: a distance along the axis for moving and
    /// scaling, a direction in the rotation plane for rotating.
    grab: Vec3,
    parent: Option<GlobalTransform>,
}

/// Orbits `focus` from `distance` away, `yaw` and `pitch` in radians.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct EditorCamera {
    focus: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    /// Took over the pose of a game camera.
    adopted: bool,
}

/// After every game camera, so the editor's view is the one left.
const EDITOR_CAMERA_ORDER: isize = 1_000_000;

/// How far, in pixels, the pointer may be from a handle and still grab it,
/// and may move between press and release and still click.
const GRAB_PIXELS: f32 = 8.0;
const CLICK_PIXELS: f32 = 4.0;

const AXIS_COLORS: [Color; 3] = [
    Color::srgb(0.93, 0.3, 0.3),
    Color::srgb(0.45, 0.85, 0.35),
    Color::srgb(0.35, 0.55, 1.0),
];
const ACTIVE_COLOR: Color = Color::srgb(1.0, 0.85, 0.2);

/// Handles drawn over everything, so the selection never hides them.
#[derive(Default, Reflect, GizmoConfigGroup)]
struct HandleGizmos;

pub(crate) struct SceneViewPlugin;

impl Plugin for SceneViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneView>()
            .insert_gizmo_config(
                HandleGizmos,
                GizmoConfig {
                    depth_bias: -1.0,
                    ..default()
                },
            )
            .add_systems(Startup, spawn_editor_camera)
            .add_systems(
                Update,
                (adopt_game_camera, handle_pointer)
                    .chain()
                    .run_if(|view: Res<SceneView>| view.active),
            )
            .add_systems(
                PostUpdate,
                draw.after(TransformSystems::Propagate)
                    .run_if(|view: Res<SceneView>| view.active),
            );
    }
}

/// Stops the scene view and removes the editor camera, so the game's own
/// cameras show.
pub(crate) fn stop(world: &mut World) {
    if let Some(mut view) = world.get_resource_mut::<SceneView>() {
        view.active = false;
    }
    let cameras: Vec<Entity> = world
        .query_filtered::<Entity, With<EditorCamera>>()
        .iter(world)
        .collect();
    for camera in cameras {
        world.despawn(camera);
    }
}

impl EditorCamera {
    /// Looking the way `from` does, at where that view meets the ground.
    fn looking(from: &Transform) -> Self {
        let forward = from.forward();
        let distance = if forward.y < -1e-3 {
            (-from.translation.y / forward.y).clamp(1.0, 100.0)
        } else {
            10.0
        };
        let (yaw, pitch, _) = from.rotation.to_euler(EulerRot::YXZ);
        Self {
            focus: from.translation + forward * distance,
            yaw,
            pitch,
            distance,
            adopted: false,
        }
    }

    fn rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }

    fn transform(&self) -> Transform {
        let rotation = self.rotation();
        Transform::from_translation(self.focus + rotation * Vec3::Z * self.distance)
            .with_rotation(rotation)
    }

    fn orbit(&mut self, delta: Vec2) {
        self.yaw -= delta.x * 0.005;
        self.pitch = (self.pitch - delta.y * 0.005).clamp(-FRAC_PI_2 + 0.01, FRAC_PI_2 - 0.01);
    }

    /// Moves the view with the pointer, faster the farther out it is.
    fn pan(&mut self, delta: Vec2) {
        let rotation = self.rotation();
        self.focus += (rotation * Vec3::NEG_X * delta.x + rotation * Vec3::Y * delta.y)
            * self.distance
            * 0.0015;
    }

    fn zoom(&mut self, scroll: f32) {
        self.distance = (self.distance * (1.0 + scroll * 0.001)).clamp(0.05, 5000.0);
    }
}

fn spawn_editor_camera(mut commands: Commands) {
    // The template camera's pose until a game camera shows up.
    let pose = Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y);
    let camera = EditorCamera::looking(&pose);
    commands.spawn((
        Name::new("Editor camera"),
        EditorOnly,
        camera,
        camera.transform(),
        Camera3d::default(),
        Camera {
            order: EDITOR_CAMERA_ORDER,
            ..default()
        },
    ));
}

/// Starts the view where the game's camera looks once the scene has one.
fn adopt_game_camera(
    mut editor: Query<&mut EditorCamera>,
    game: Query<&Transform, (With<Camera>, Without<EditorCamera>)>,
) {
    let Ok(mut editor) = editor.single_mut() else {
        return;
    };
    if let Some(pose) = game.iter().next().filter(|_| !editor.adopted) {
        *editor = EditorCamera {
            adopted: true,
            ..EditorCamera::looking(pose)
        };
    }
}

/// The handles' world directions for the selection at `transform`. Moving
/// and rotating use the world axes, scaling the entity's own.
fn handle_axes(tool: Tool, transform: &GlobalTransform) -> [Vec3; 3] {
    let rotation = match tool {
        Tool::Translate | Tool::Rotate => Quat::IDENTITY,
        Tool::Scale => transform.rotation(),
    };
    [rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z]
}

/// Handles keep the same size on screen.
fn handle_length(camera: &GlobalTransform, origin: Vec3) -> f32 {
    camera.translation().distance(origin) * 0.15
}

#[allow(
    clippy::too_many_arguments,
    reason = "a system: each parameter is a query or resource it needs"
)]
fn handle_pointer(
    mut view: ResMut<SceneView>,
    mut selected: ResMut<Selected>,
    mut cameras: Query<(&mut EditorCamera, &mut Transform, &Camera, &GlobalTransform)>,
    mut targets: Query<(&mut Transform, &GlobalTransform, Option<&ChildOf>), Without<EditorCamera>>,
    globals: Query<&GlobalTransform>,
    editor_only: Query<(), With<EditorOnly>>,
    mut ray_cast: MeshRayCast,
    registry: Res<AppTypeRegistry>,
) {
    let Ok((mut camera, mut camera_transform, render, camera_global)) = cameras.single_mut() else {
        view.events.clear();
        return;
    };
    let view = &mut *view;
    let tool = view.tool;
    for event in std::mem::take(&mut view.events) {
        let pointer = match event {
            PointerEvent::Down { x, y, .. }
            | PointerEvent::Move { x, y }
            | PointerEvent::Up { x, y, .. } => Vec2::new(x, y),
            PointerEvent::Scroll { .. } => view.pointer,
        };
        let delta = pointer - view.pointer;
        view.pointer = pointer;
        let ray = render.viewport_to_world(camera_global, pointer).ok();
        // The selection's handles, if it has a place in the world.
        let handles =
            selected
                .0
                .and_then(|entity| targets.get(entity).ok())
                .map(|(_, global, _)| {
                    let origin = global.translation();
                    (
                        origin,
                        handle_axes(tool, global),
                        handle_length(camera_global, origin),
                    )
                });
        let grab = |pointer: Vec2| {
            let (origin, axes, length) = handles?;
            nearest_handle(tool, render, camera_global, origin, axes, length, pointer)
        };

        match event {
            PointerEvent::Down { button, .. } => {
                view.pressed = Some((button, pointer));
                if let (Button::Left, Some(axis), Some(ray), Some(entity)) =
                    (button, grab(pointer), ray, selected.0)
                {
                    let (origin, axes, length) = handles.expect("a handle was grabbed");
                    let (transform, _, parent) = targets.get(entity).expect("selected");
                    let direction = axes[axis];
                    let grab = match tool {
                        Tool::Translate | Tool::Scale => {
                            Vec3::splat(axis_param(origin, direction, ray).unwrap_or(0.0))
                        }
                        Tool::Rotate => plane_point(origin, direction, ray)
                            .map_or(Vec3::ZERO, |point| point - origin),
                    };
                    view.drag = Some(Drag {
                        entity,
                        axis,
                        direction,
                        origin,
                        length,
                        start: *transform,
                        grab,
                        parent: parent
                            .and_then(|parent| globals.get(parent.parent()).ok().copied()),
                    });
                }
            }
            PointerEvent::Move { .. } => match (&view.drag, view.pressed) {
                (Some(drag), _) => {
                    if let (Some(ray), Ok((mut transform, ..))) =
                        (ray, targets.get_mut(drag.entity))
                    {
                        *transform = dragged(tool, drag, ray);
                    }
                }
                (None, Some((Button::Right, _))) => camera.orbit(delta),
                (None, Some((Button::Middle, _))) => camera.pan(delta),
                (None, _) => view.hovered = grab(pointer),
            },
            PointerEvent::Up { button, .. } => {
                let pressed = view.pressed.take();
                if let Some(drag) = view.drag.take() {
                    if let Ok((transform, ..)) = targets.get(drag.entity) {
                        report_edited(&registry, drag.entity, &drag.start, transform);
                    }
                } else if let (Button::Left, Some((Button::Left, down)), Some(ray)) =
                    (button, pressed, ray)
                    && down.distance(pointer) <= CLICK_PIXELS
                {
                    let filter = |entity: Entity| !editor_only.contains(entity);
                    let settings = MeshRayCastSettings::default().with_filter(&filter);
                    let hit = ray_cast
                        .cast_ray(ray, &settings)
                        .first()
                        .map(|(entity, _)| *entity);
                    if hit != selected.0 {
                        selected.0 = hit;
                        report_selection(hit);
                    }
                }
            }
            PointerEvent::Scroll { delta } => camera.zoom(delta),
        }
    }

    if std::mem::take(&mut view.focus)
        && let Some((_, global, _)) = selected.0.and_then(|entity| targets.get(entity).ok())
    {
        camera.focus = global.translation();
        camera.distance = (global.scale().max_element() * 3.0).max(2.0);
    }
    *camera_transform = camera.transform();
}

/// The handle under `pointer`, if any is within grabbing distance.
fn nearest_handle(
    tool: Tool,
    camera: &Camera,
    camera_global: &GlobalTransform,
    origin: Vec3,
    axes: [Vec3; 3],
    length: f32,
    pointer: Vec2,
) -> Option<usize> {
    let project = |point: Vec3| camera.world_to_viewport(camera_global, point).ok();
    axes.iter()
        .enumerate()
        .filter_map(|(index, &axis)| {
            let distance = match tool {
                Tool::Translate | Tool::Scale => {
                    segment_distance(project(origin)?, project(origin + axis * length)?, pointer)
                }
                Tool::Rotate => {
                    let points: Option<Vec<Vec2>> =
                        circle_points(origin, axis, length).map(project).collect();
                    points?
                        .windows(2)
                        .map(|pair| segment_distance(pair[0], pair[1], pointer))
                        .fold(f32::INFINITY, f32::min)
                }
            };
            (distance <= GRAB_PIXELS).then_some((index, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

/// The dragged entity's transform with the pointer now on `ray`.
fn dragged(tool: Tool, drag: &Drag, ray: Ray3d) -> Transform {
    let mut transform = drag.start;
    match tool {
        Tool::Translate => {
            if let Some(param) = axis_param(drag.origin, drag.direction, ray) {
                let world = drag.direction * (param - drag.grab.x);
                transform.translation += drag.parent.map_or(world, |parent| {
                    parent.affine().inverse().transform_vector3(world)
                });
            }
        }
        Tool::Rotate => {
            if let Some(point) = plane_point(drag.origin, drag.direction, ray) {
                let angle = signed_angle(drag.grab, point - drag.origin, drag.direction);
                let parent = drag
                    .parent
                    .map_or(Quat::IDENTITY, |parent| parent.rotation());
                transform.rotation = parent.inverse()
                    * Quat::from_axis_angle(drag.direction, angle)
                    * parent
                    * drag.start.rotation;
            }
        }
        Tool::Scale => {
            if let Some(param) = axis_param(drag.origin, drag.direction, ray) {
                let factor = (1.0 + (param - drag.grab.x) / drag.length).max(0.01);
                transform.scale[drag.axis] = drag.start.scale[drag.axis] * factor;
            }
        }
    }
    transform
}

/// Tells the editor a drag changed `entity`'s transform, before and after.
fn report_edited(
    registry: &AppTypeRegistry,
    entity: Entity,
    before: &Transform,
    after: &Transform,
) {
    if before == after {
        return;
    }
    let registry = registry.read();
    let value = |transform: &Transform| {
        serde_json::to_value(TypedReflectSerializer::new(transform, &registry))
            .unwrap_or(Value::Null)
    };
    send_report(json!({
        "type": "edited",
        "id": entity.to_bits(),
        "component": Transform::type_path(),
        "before": value(before),
        "after": value(after),
    }));
}

fn draw(
    view: Res<SceneView>,
    selected: Res<Selected>,
    targets: Query<(&GlobalTransform, Option<&Aabb>), Without<EditorCamera>>,
    camera: Query<&GlobalTransform, With<EditorCamera>>,
    mut gizmos: Gizmos,
    mut handles: Gizmos<HandleGizmos>,
) {
    gizmos.grid(
        Isometry3d::from_rotation(Quat::from_rotation_x(FRAC_PI_2)),
        UVec2::splat(40),
        Vec2::ONE,
        Color::srgba(0.6, 0.6, 0.6, 0.35),
    );
    let (Some((global, aabb)), Ok(camera)) = (
        selected.0.and_then(|entity| targets.get(entity).ok()),
        camera.single(),
    ) else {
        return;
    };
    if let Some(aabb) = aabb {
        let bounds = Transform::from_translation(aabb.center.into())
            .with_scale(Vec3::from(aabb.half_extents) * 2.0);
        handles.cube(global.mul_transform(bounds), ACTIVE_COLOR.with_alpha(0.6));
    }
    let origin = global.translation();
    let length = handle_length(camera, origin);
    let active = view.drag.as_ref().map(|drag| drag.axis).or(view.hovered);
    for (index, axis) in handle_axes(view.tool, global).into_iter().enumerate() {
        let color = if active == Some(index) {
            ACTIVE_COLOR
        } else {
            AXIS_COLORS[index]
        };
        let end = origin + axis * length;
        match view.tool {
            Tool::Translate => {
                handles.arrow(origin, end, color);
            }
            Tool::Rotate => {
                handles.circle(
                    Isometry3d::new(origin, Quat::from_rotation_arc(Vec3::Z, axis)),
                    length,
                    color,
                );
            }
            Tool::Scale => {
                handles.line(origin, end, color);
                handles.cube(
                    Transform::from_translation(end).with_scale(Vec3::splat(length * 0.08)),
                    color,
                );
            }
        }
    }
}

/// How far along the line `origin + t * direction` the point nearest to
/// `ray` is, or none when they are parallel.
fn axis_param(origin: Vec3, direction: Vec3, ray: Ray3d) -> Option<f32> {
    let ray_direction = *ray.direction;
    let a = direction.dot(direction);
    let b = direction.dot(ray_direction);
    let c = ray_direction.dot(ray_direction);
    let w = origin - ray.origin;
    let d = direction.dot(w);
    let e = ray_direction.dot(w);
    let denominator = a * c - b * b;
    (denominator.abs() > 1e-6).then(|| (b * e - c * d) / denominator)
}

/// Where `ray` crosses the plane through `origin` facing `normal`, unless it
/// runs along the plane.
fn plane_point(origin: Vec3, normal: Vec3, ray: Ray3d) -> Option<Vec3> {
    let facing = normal.dot(*ray.direction);
    (facing.abs() > 1e-4)
        .then(|| ray.origin + *ray.direction * (normal.dot(origin - ray.origin) / facing))
}

/// The angle from `from` to `to` around `axis`, counterclockwise positive.
fn signed_angle(from: Vec3, to: Vec3, axis: Vec3) -> f32 {
    from.cross(to).dot(axis).atan2(from.dot(to))
}

fn circle_points(origin: Vec3, axis: Vec3, radius: f32) -> impl Iterator<Item = Vec3> {
    let (u, v) = axis.any_orthonormal_pair();
    (0..=32).map(move |step| {
        let angle = step as f32 / 32.0 * TAU;
        origin + (u * angle.cos() + v * angle.sin()) * radius
    })
}

fn segment_distance(start: Vec2, end: Vec2, point: Vec2) -> f32 {
    let segment = end - start;
    let t = ((point - start).dot(segment) / segment.length_squared().max(1e-6)).clamp(0.0, 1.0);
    point.distance(start + segment * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ray(origin: Vec3, direction: Vec3) -> Ray3d {
        Ray3d::new(origin, Dir3::new(direction).unwrap())
    }

    #[test]
    fn finds_where_the_pointer_is_along_an_axis() {
        // Looking down at the X axis from above x = 3.
        let param = axis_param(
            Vec3::ZERO,
            Vec3::X,
            ray(Vec3::new(3.0, 5.0, 0.0), Vec3::NEG_Y),
        );
        assert!((param.unwrap() - 3.0).abs() < 1e-5);
        assert_eq!(axis_param(Vec3::ZERO, Vec3::X, ray(Vec3::Y, Vec3::X)), None);
    }

    #[test]
    fn drags_move_rotate_and_scale_the_start_transform() {
        let mut drag = Drag {
            entity: Entity::PLACEHOLDER,
            axis: 0,
            direction: Vec3::X,
            origin: Vec3::ZERO,
            length: 2.0,
            start: Transform::from_xyz(0.0, 1.0, 0.0),
            grab: Vec3::splat(1.0),
            parent: None,
        };
        let above = |x: f32| ray(Vec3::new(x, 5.0, 0.0), Vec3::NEG_Y);

        let moved = dragged(Tool::Translate, &drag, above(4.0));
        assert!(
            moved
                .translation
                .abs_diff_eq(Vec3::new(3.0, 1.0, 0.0), 1e-5)
        );

        // Grabbed one unit out, now three: one handle length farther.
        let scaled = dragged(Tool::Scale, &drag, above(3.0));
        assert!((scaled.scale.x - 2.0).abs() < 1e-5);
        assert_eq!(scaled.scale.y, 1.0);

        // A quarter turn about Y, from +X towards -Z.
        drag.direction = Vec3::Y;
        drag.grab = Vec3::X;
        let rotated = dragged(
            Tool::Rotate,
            &drag,
            ray(Vec3::new(0.0, 5.0, -2.0), Vec3::NEG_Y),
        );
        assert!(
            rotated
                .rotation
                .abs_diff_eq(Quat::from_rotation_y(FRAC_PI_2), 1e-5)
        );

        // A parent scaled by two halves the local move.
        drag.direction = Vec3::X;
        drag.grab = Vec3::splat(1.0);
        drag.parent = Some(GlobalTransform::from_scale(Vec3::splat(2.0)));
        let moved = dragged(Tool::Translate, &drag, above(4.0));
        assert!(
            moved
                .translation
                .abs_diff_eq(Vec3::new(1.5, 1.0, 0.0), 1e-5)
        );
    }

    #[test]
    fn the_editor_camera_orbits_pans_and_zooms_around_its_focus() {
        let pose = Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y);
        let mut camera = EditorCamera::looking(&pose);
        assert!(camera.focus.abs_diff_eq(Vec3::ZERO, 1e-4), "{camera:?}");
        assert!(
            camera
                .transform()
                .translation
                .abs_diff_eq(pose.translation, 1e-4)
        );

        camera.orbit(Vec2::new(300.0, 0.0));
        assert!((camera.transform().translation.length() - pose.translation.length()).abs() < 1e-3);
        camera.zoom(-500.0);
        assert!((camera.distance - pose.translation.length() * 0.5).abs() < 1e-3);
        camera.pan(Vec2::new(0.0, 100.0));
        assert!(camera.focus.y > 0.0, "dragging down moves the view up");
    }

    #[test]
    fn reads_pointer_events() {
        let event: PointerEvent =
            serde_json::from_str(r#"{"event":"down","x":1.5,"y":2,"button":"right"}"#).unwrap();
        assert_eq!(
            event,
            PointerEvent::Down {
                x: 1.5,
                y: 2.0,
                button: Button::Right
            }
        );
    }

    #[test]
    fn measures_distance_to_a_segment() {
        assert_eq!(
            segment_distance(Vec2::ZERO, Vec2::X * 10.0, Vec2::new(5.0, 3.0)),
            3.0
        );
        assert_eq!(
            segment_distance(Vec2::ZERO, Vec2::X * 10.0, Vec2::new(-4.0, 3.0)),
            5.0
        );
    }
}
