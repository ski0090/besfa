//! The model preview: one glTF model from the asset folder, shown in place
//! of the scene view with its clips, its parts and a choice of light.
//!
//! The model, its camera and its light are on a render layer of their own,
//! so the scene neither shows in the preview nor lights it, and the scene
//! view never sees the model. The model is `EditorOnly`, out of the Hierarchy
//! and the scene file, but it lives in the game's world, so the game's own
//! systems (materials, outlines) treat it like any model they come across.
//! Clips move on real time: virtual time stands still in edit mode.

use std::{
    collections::HashSet,
    f32::consts::{FRAC_PI_2, PI},
};

use besfa_protocol::{Button, ClipInfo, EntityInfo, LightPreset, PointerEvent, Report, clip_time};
use bevy::{
    asset::LoadState,
    camera::{Exposure, primitives::Aabb, visibility::RenderLayers},
    gltf::Gltf,
    prelude::*,
    transform::TransformSystems,
    world_serialization::{WorldAssetRoot, WorldInstance, WorldInstanceSpawner},
};

use crate::{
    edit::subtree,
    inspect::send_report,
    scene_view::{AXIS_COLORS, EDITOR_CAMERA_ORDER, EditorCamera, EditorOnly},
};

/// The preview's render layer, out of the way of a game's own.
const PREVIEW_LAYER: usize = 31;

/// How long a spawned model waits for its meshes' bounds before it is
/// reported without the ones still missing.
const BOUNDS_FRAMES: u32 = 30;

fn layer() -> RenderLayers {
    RenderLayers::layer(PREVIEW_LAYER)
}

/// The model preview's state, fed by the editor's commands.
#[derive(Resource, Default)]
pub(crate) struct ModelPreview {
    /// Shown in place of the scene view.
    pub(crate) showing: bool,
    /// Pointer events since the last frame.
    pub(crate) events: Vec<PointerEvent>,
    /// Frame the model on the next frame.
    pub(crate) fit: bool,
    path: Option<String>,
    /// The entity the model spawns under.
    root: Option<Entity>,
    /// Held so the clips stay loaded.
    gltf: Handle<Gltf>,
    /// Each clip's animation node and length, once the model has loaded.
    clips: Option<Vec<(AnimationNodeIndex, f32)>>,
    /// The model's world bounds, once it has loaded.
    bounds: Option<(Vec3, Vec3)>,
    /// The editor has the model's clips and parts.
    reported: bool,
    /// Frames the spawned model has waited for its meshes' bounds.
    waited: u32,
    animation: Animation,
    /// Meshes `preview_parts` hid, with the visibility they had.
    hid: Vec<(Entity, Visibility)>,
    pressed: Option<Button>,
    pointer: Vec2,
}

#[derive(Default, Clone, Copy)]
struct Animation {
    clip: Option<usize>,
    time: f32,
    speed: f32,
    looping: bool,
}

/// Orbits the previewed model the way the scene view's camera does.
#[derive(Component)]
pub(crate) struct PreviewCamera(EditorCamera);

/// The preview's light, by the preset the editor chose.
#[derive(Component, Default)]
struct PreviewLight(LightPreset);

/// The preview's grid, drawn on its layer only.
#[derive(Default, Reflect, GizmoConfigGroup)]
struct PreviewGizmos;

pub(crate) struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModelPreview>()
            .insert_gizmo_config(
                PreviewGizmos,
                GizmoConfig {
                    render_layers: layer(),
                    ..default()
                },
            )
            .add_systems(Startup, spawn_camera_and_light)
            .add_systems(
                Update,
                ((load, orbit, animate).chain(), lighting).run_if(showing),
            )
            .add_systems(
                PostUpdate,
                // After the scene spawner, before the Hierarchy is reported.
                (mark_model, draw.after(TransformSystems::Propagate)).run_if(showing),
            );
    }
}

pub(crate) fn showing(preview: Option<Res<ModelPreview>>) -> bool {
    preview.is_some_and(|preview| preview.showing)
}

pub(crate) fn showing_in(world: &World) -> bool {
    world
        .get_resource::<ModelPreview>()
        .is_some_and(|preview| preview.showing)
}

fn spawn_camera_and_light(mut commands: Commands) {
    commands.spawn((
        Name::new("Preview camera"),
        EditorOnly,
        PreviewCamera(framing(Vec3::splat(-0.5), Vec3::splat(0.5))),
        Camera3d::default(),
        Camera {
            order: EDITOR_CAMERA_ORDER + 1,
            is_active: false,
            ..default()
        },
        // Small models come close.
        Projection::Perspective(PerspectiveProjection {
            near: 0.01,
            ..default()
        }),
        AmbientLight::default(),
        layer(),
    ));
    commands.spawn((
        Name::new("Preview light"),
        EditorOnly,
        PreviewLight::default(),
        DirectionalLight::default(),
        layer(),
    ));
}

/// Shows the model at `path`, or the scene view again without one.
pub(crate) fn show(world: &mut World, path: Option<String>) -> Result<(), String> {
    let Some(path) = path else {
        close(world);
        return Ok(());
    };
    if !(path.ends_with(".glb") || path.ends_with(".gltf")) {
        return Err(format!("{path} is not a glTF model"));
    }
    let preview = world.resource::<ModelPreview>();
    if preview.path.as_deref() != Some(path.as_str()) {
        drop_model(world);
        let assets = world.resource::<AssetServer>().clone();
        let root = world
            .spawn((
                Name::new("Model preview"),
                EditorOnly,
                layer(),
                Transform::default(),
                WorldAssetRoot(assets.load(format!("{path}#Scene0"))),
            ))
            .id();
        let mut preview = world.resource_mut::<ModelPreview>();
        preview.gltf = assets.load(path.clone());
        preview.root = Some(root);
        preview.path = Some(path);
        preview.fit = true;
    }
    // Shown again: every part shows, and the editor learns them anew.
    restore_parts(world);
    let mut preview = world.resource_mut::<ModelPreview>();
    preview.reported = false;
    preview.showing = true;
    switch_cameras(world, true);
    Ok(())
}

/// Drops the model and shows the scene view again.
pub(crate) fn close(world: &mut World) {
    if world.get_resource::<ModelPreview>().is_none() {
        return;
    }
    drop_model(world);
    world.resource_mut::<ModelPreview>().showing = false;
    switch_cameras(world, false);
}

fn drop_model(world: &mut World) {
    let preview = std::mem::take(&mut *world.resource_mut::<ModelPreview>());
    if let Some(root) = preview.root {
        world.despawn(root);
    }
}

/// Only one camera may see a mesh in Bevy 0.20, so the preview's and the
/// scene view's take turns.
fn switch_cameras(world: &mut World, preview: bool) {
    let mut cameras = world.query::<(&mut Camera, Has<PreviewCamera>, Has<EditorCamera>)>();
    for (mut camera, previews, edits) in cameras.iter_mut(world) {
        if previews || edits {
            camera.is_active = previews == preview;
        }
    }
}

pub(crate) fn animate_with(
    world: &mut World,
    clip: Option<usize>,
    time: f32,
    speed: f32,
    looping: bool,
) {
    if let Some(mut preview) = world.get_resource_mut::<ModelPreview>() {
        preview.animation = Animation {
            clip,
            time,
            speed,
            looping,
        };
    }
}

/// Hides the meshes under `hidden` parts, and with `solo` the ones outside
/// it, giving back what the last call hid first.
pub(crate) fn parts(world: &mut World, hidden: &[u64], solo: Option<u64>) -> Result<(), String> {
    restore_parts(world);
    let root = world
        .get_resource::<ModelPreview>()
        .and_then(|preview| preview.root)
        .ok_or("no model is previewed")?;
    let hidden: HashSet<Entity> = hidden
        .iter()
        .filter_map(|&id| Entity::try_from_bits(id))
        .collect();
    let solo = solo.and_then(Entity::try_from_bits);
    let mut hid = Vec::new();
    for mesh in meshes_to_hide(world, root, &hidden, solo) {
        if let Some(mut visibility) = world.get_mut::<Visibility>(mesh) {
            hid.push((
                mesh,
                std::mem::replace(&mut *visibility, Visibility::Hidden),
            ));
        }
    }
    world.resource_mut::<ModelPreview>().hid = hid;
    Ok(())
}

/// Gives back the visibility the parts had, which the game may have set.
fn restore_parts(world: &mut World) {
    let hid = std::mem::take(&mut world.resource_mut::<ModelPreview>().hid);
    for (mesh, was) in hid {
        if let Some(mut visibility) = world.get_mut::<Visibility>(mesh) {
            *visibility = was;
        }
    }
}

/// The meshes under `root` inside a `hidden` part, or outside `solo`.
fn meshes_to_hide(
    world: &World,
    root: Entity,
    hidden: &HashSet<Entity>,
    solo: Option<Entity>,
) -> Vec<Entity> {
    subtree(world, root)
        .into_iter()
        .filter(|&entity| world.get::<Mesh3d>(entity).is_some())
        .filter(|&mesh| {
            let mut line = std::iter::successors(Some(mesh), |&entity| {
                world.get::<ChildOf>(entity).map(ChildOf::parent)
            })
            .take_while(|&entity| entity != root);
            let line: Vec<Entity> = line.by_ref().collect();
            line.iter().any(|entity| hidden.contains(entity))
                || solo.is_some_and(|solo| !line.contains(&solo))
        })
        .collect()
}

/// Chooses the preview's light, which `lighting` keeps up.
pub(crate) fn light(world: &mut World, preset: LightPreset) {
    let mut lights = world.query::<&mut PreviewLight>();
    for mut light in lights.iter_mut(world) {
        light.0 = preset;
    }
}

/// Lights the model's front, -Z, except at night. The presets are set for
/// Bevy's default exposure and scaled to the one the game gives the
/// preview's camera: duskhold gives every camera sunlight's, which left all
/// three nearly black and alike.
fn lighting(
    mut lights: Query<(&PreviewLight, &mut DirectionalLight, &mut Transform)>,
    mut cameras: Query<(Option<&Exposure>, &mut AmbientLight), With<PreviewCamera>>,
) {
    let (Ok((preset, mut light, mut transform)), Ok((exposure, mut ambient))) =
        (lights.single_mut(), cameras.single_mut())
    else {
        return;
    };
    // Lux and cd/m².
    let (illuminance, color, from, brightness, sky) = match preset.0 {
        LightPreset::Studio => (
            6000.0,
            Color::WHITE,
            Vec3::new(-2.0, 4.0, -3.0),
            500.0,
            Color::WHITE,
        ),
        LightPreset::Daylight => (
            12000.0,
            Color::srgb(1.0, 0.96, 0.88),
            Vec3::new(1.0, 6.0, -2.0),
            800.0,
            Color::srgb(0.75, 0.85, 1.0),
        ),
        LightPreset::Night => (
            1500.0,
            Color::srgb(0.6, 0.7, 1.0),
            Vec3::new(1.0, 3.0, 4.0),
            120.0,
            Color::srgb(0.4, 0.45, 0.7),
        ),
    };
    let ev100 = exposure.map_or(Exposure::EV100_BLENDER, |exposure| exposure.ev100);
    let scale = (ev100 - Exposure::EV100_BLENDER).exp2();
    *light = DirectionalLight {
        illuminance: illuminance * scale,
        color,
        shadow_maps_enabled: true,
        ..default()
    };
    *transform = Transform::from_translation(from).looking_at(Vec3::ZERO, Vec3::Y);
    *ambient = AmbientLight {
        color: sky,
        brightness: brightness * scale,
        ..default()
    };
}

/// Keeps what the model spawns, whenever it spawns it, on the preview's
/// layer and out of the Hierarchy.
fn mark_model(
    mut commands: Commands,
    preview: Res<ModelPreview>,
    children: Query<&Children>,
    unmarked: Query<(), Without<EditorOnly>>,
) {
    let Some(root) = preview.root else {
        return;
    };
    for entity in children.iter_descendants(root) {
        if unmarked.contains(entity) {
            commands.entity(entity).insert((EditorOnly, layer()));
        }
    }
}

/// Once the model has spawned, plays its clips through a graph of its own
/// and reports it; reports why not if the file did not load.
fn load(world: &mut World) {
    let preview = world.resource::<ModelPreview>();
    let (Some(root), Some(path), false) = (preview.root, preview.path.clone(), preview.reported)
    else {
        return;
    };
    let assets = world.resource::<AssetServer>();
    let scene = world.get::<WorldAssetRoot>(root).map(|scene| scene.0.id());
    let failed = [
        Some(preview.gltf.id().untyped()),
        scene.map(|id| id.untyped()),
    ]
    .into_iter()
    .flatten()
    .find_map(|id| match assets.load_state(id) {
        LoadState::Failed(error) => Some(error.to_string()),
        _ => None,
    });
    if let Some(error) = failed {
        world.resource_mut::<ModelPreview>().reported = true;
        return send_report(&Report::Preview {
            path,
            clips: vec![],
            parts: vec![],
            size: None,
            error: Some(error),
        });
    }
    let gltf = preview.gltf.clone();
    // The scene spawns after the file loads, and may hold nothing.
    let spawned = world.get::<WorldInstance>(root).is_some_and(|instance| {
        world
            .resource::<WorldInstanceSpawner>()
            .instance_is_ready(**instance)
    });
    if !spawned {
        return;
    }
    let entities = subtree(world, root);
    // Meshes get bounds the frame after they spawn. Some never do, like
    // skinned meshes left unculled.
    if entities
        .iter()
        .any(|&entity| world.get::<Mesh3d>(entity).is_some() && world.get::<Aabb>(entity).is_none())
    {
        let mut preview = world.resource_mut::<ModelPreview>();
        preview.waited += 1;
        if preview.waited < BOUNDS_FRAMES {
            return;
        }
    }
    let Some(gltf) = world.resource::<Assets<Gltf>>().get(&gltf) else {
        return;
    };

    let clip_handles = gltf.animations.clone();
    let clips_assets = world.resource::<Assets<AnimationClip>>();
    let infos: Vec<ClipInfo> = clip_handles
        .iter()
        .enumerate()
        .map(|(index, handle)| ClipInfo {
            name: gltf
                .named_animations
                .iter()
                .find(|(_, named)| *named == handle)
                .map_or_else(|| format!("Animation{index}"), |(name, _)| name.to_string()),
            duration: clips_assets
                .get(handle)
                .map_or(0.0, AnimationClip::duration),
        })
        .collect();
    if world.resource::<ModelPreview>().clips.is_none() {
        let (graph, nodes) = AnimationGraph::from_clips(clip_handles);
        let graph = world.resource_mut::<Assets<AnimationGraph>>().add(graph);
        for &entity in &entities {
            if world.get::<AnimationPlayer>(entity).is_some() {
                world
                    .entity_mut(entity)
                    .insert(AnimationGraphHandle(graph.clone()));
            }
        }
        let durations = infos.iter().map(|clip| clip.duration);
        world.resource_mut::<ModelPreview>().clips =
            Some(nodes.into_iter().zip(durations).collect());
    }

    let parts = entities[1..]
        .iter()
        .map(|&entity| EntityInfo {
            id: entity.to_bits(),
            name: world.get::<Name>(entity).map(|name| name.as_str().into()),
            parent: world
                .get::<ChildOf>(entity)
                .map(ChildOf::parent)
                .filter(|&parent| parent != root)
                .map(Entity::to_bits),
            scene: true,
            asset: None,
        })
        .collect();
    let bounds = bounds(world, &entities);
    let mut preview = world.resource_mut::<ModelPreview>();
    preview.reported = true;
    if preview.bounds.is_none() {
        preview.bounds = bounds;
        preview.fit = true;
    }
    send_report(&Report::Preview {
        path,
        clips: infos,
        parts,
        size: bounds.map(|(min, max)| (max - min).to_array()),
        error: None,
    });
}

/// The world bounds of the meshes among `entities`.
fn bounds(world: &World, entities: &[Entity]) -> Option<(Vec3, Vec3)> {
    let mut min = Vec3::INFINITY;
    let mut max = Vec3::NEG_INFINITY;
    for &entity in entities {
        let (Some(aabb), Some(global)) = (
            world.get::<Aabb>(entity),
            world.get::<GlobalTransform>(entity),
        ) else {
            continue;
        };
        let (center, half) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
        for corner in 0..8 {
            let sign = Vec3::new(
                if corner & 1 == 0 { -1.0 } else { 1.0 },
                if corner & 2 == 0 { -1.0 } else { 1.0 },
                if corner & 4 == 0 { -1.0 } else { 1.0 },
            );
            let point = global.transform_point(center + half * sign);
            min = min.min(point);
            max = max.max(point);
        }
    }
    (min.x <= max.x).then_some((min, max))
}

/// Looking at the model's front from above one side, near enough to fill
/// the view with it. Models made for Bevy face its forward, -Z.
fn framing(min: Vec3, max: Vec3) -> EditorCamera {
    let radius = ((max - min).length() * 0.5).max(0.05);
    // Half of Bevy's default vertical field of view, 45°.
    let half_view = FRAC_PI_2 * 0.25;
    EditorCamera {
        focus: (min + max) * 0.5,
        yaw: PI - 0.6,
        pitch: -0.3,
        distance: radius / half_view.sin() * 1.1,
        adopted: false,
    }
}

/// Left or right drag orbits, middle drag pans, scrolling zooms.
fn orbit(
    mut preview: ResMut<ModelPreview>,
    mut cameras: Query<(&mut PreviewCamera, &mut Transform)>,
) {
    let preview = &mut *preview;
    let Ok((mut camera, mut transform)) = cameras.single_mut() else {
        preview.events.clear();
        return;
    };
    for event in std::mem::take(&mut preview.events) {
        match event {
            PointerEvent::Down { x, y, button } => {
                preview.pressed = Some(button);
                preview.pointer = Vec2::new(x, y);
            }
            PointerEvent::Move { x, y } => {
                let pointer = Vec2::new(x, y);
                let delta = pointer - std::mem::replace(&mut preview.pointer, pointer);
                match preview.pressed {
                    Some(Button::Left | Button::Right) => camera.0.orbit(delta),
                    Some(Button::Middle) => camera.0.pan(delta),
                    None => {}
                }
            }
            PointerEvent::Up { .. } => preview.pressed = None,
            PointerEvent::Scroll { delta } => camera.0.zoom(delta),
        }
    }
    if let (true, Some((min, max))) = (preview.fit, preview.bounds) {
        preview.fit = false;
        camera.0 = framing(min, max);
    }
    *transform = camera.0.transform();
}

/// Poses the model at the chosen clip's time, moved on by real time.
fn animate(
    mut preview: ResMut<ModelPreview>,
    time: Res<Time<Real>>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let animation = preview.animation;
    let (Some(root), Some(&(node, duration))) = (
        preview.root,
        animation
            .clip
            .and_then(|clip| preview.clips.as_ref()?.get(clip)),
    ) else {
        return;
    };
    let seek = clip_time(
        animation.time,
        time.delta_secs(),
        animation.speed,
        duration,
        animation.looping,
    );
    preview.animation.time = seek;
    for entity in children.iter_descendants(root) {
        if let Ok(mut player) = players.get_mut(entity) {
            if !player.is_playing_animation(node) {
                player.stop_all();
            }
            // Paused, so only this clock moves it; paused clips still pose.
            player.play(node).pause().set_seek_time(seek);
        }
    }
}

/// A ground grid a few times the model's size, in meters, and the origin's
/// axes.
fn draw(preview: Res<ModelPreview>, mut gizmos: Gizmos<PreviewGizmos>) {
    let extent = preview
        .bounds
        .map_or(1.0, |(min, max)| (max - min).max_element());
    let cells = ((extent * 1.5).ceil() as u32).clamp(2, 100) * 2;
    gizmos.grid(
        Isometry3d::from_rotation(Quat::from_rotation_x(FRAC_PI_2)),
        UVec2::splat(cells),
        Vec2::ONE,
        Color::srgba(0.6, 0.6, 0.6, 0.35),
    );
    let length = (extent * 0.25).max(0.25);
    for (axis, color) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().zip(AXIS_COLORS) {
        gizmos.line(Vec3::ZERO, axis * length, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_parts_with_what_is_under_them_or_all_but_the_solo_one() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let body = world.spawn(ChildOf(root)).id();
        let body_mesh = world.spawn((Mesh3d::default(), ChildOf(body))).id();
        let head = world.spawn((Mesh3d::default(), ChildOf(body))).id();
        let hat = world.spawn((Mesh3d::default(), ChildOf(head))).id();
        let sword = world.spawn((Mesh3d::default(), ChildOf(root))).id();
        let none = HashSet::new();

        assert!(meshes_to_hide(&world, root, &none, None).is_empty());
        let mut hidden = meshes_to_hide(&world, root, &HashSet::from([head]), None);
        hidden.sort();
        let mut expected = vec![head, hat];
        expected.sort();
        assert_eq!(hidden, expected);
        let mut hidden = meshes_to_hide(&world, root, &none, Some(head));
        hidden.sort();
        let mut expected = vec![body_mesh, sword];
        expected.sort();
        assert_eq!(hidden, expected, "the solo part keeps what is under it");
        assert_eq!(
            meshes_to_hide(&world, root, &HashSet::from([root]), None),
            [],
            "the root is not a part"
        );
    }

    #[test]
    fn lights_as_brightly_under_the_exposure_the_game_sets() {
        use bevy::ecs::system::RunSystemOnce;

        let mut world = World::new();
        let camera = world
            .spawn((
                PreviewCamera(framing(Vec3::ZERO, Vec3::ONE)),
                AmbientLight::default(),
            ))
            .id();
        let light = world
            .spawn((
                PreviewLight(LightPreset::Night),
                DirectionalLight::default(),
            ))
            .id();
        world.run_system_once(lighting).unwrap();
        assert_eq!(
            world.get::<DirectionalLight>(light).unwrap().illuminance,
            1500.0
        );

        world.entity_mut(camera).insert(Exposure::SUNLIGHT);
        world.run_system_once(lighting).unwrap();
        let illuminance = world.get::<DirectionalLight>(light).unwrap().illuminance;
        // 2^(15 - 9.7) ≈ 39 times as bright.
        assert!((illuminance / 1500.0 - 39.4).abs() < 0.1, "{illuminance}");
        let brightness = world.get::<AmbientLight>(camera).unwrap().brightness;
        assert!((brightness / 120.0 - 39.4).abs() < 0.1, "{brightness}");
    }

    #[test]
    fn frames_the_whole_model() {
        let camera = framing(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 2.0, 0.5));
        assert_eq!(camera.focus, Vec3::new(0.0, 1.0, 0.0));
        let eye = camera.transform().translation;
        assert!(eye.z < 0.0 && eye.y > 1.0, "in front and above: {eye}");
        assert!(eye.distance(camera.focus) > 1.2, "the model fits");
    }
}
