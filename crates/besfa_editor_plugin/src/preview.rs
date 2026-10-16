//! The model preview: one glTF model from the asset folder, shown in place
//! of the scene view with its clips, its parts and a choice of light, with
//! what it is made of and overlays of its skeleton, markers and bounds.
//!
//! The model, its camera and its light are on a render layer of their own,
//! so the scene neither shows in the preview nor lights it, and the scene
//! view never sees the model. The model is `EditorOnly`, out of the Hierarchy
//! and the scene file, but it lives in the game's world, so the game's own
//! systems (materials, outlines) treat it like any model they come across.
//! Clips move on real time: virtual time stands still in edit mode.

use std::{
    collections::HashSet,
    f32::consts::{FRAC_PI_2, FRAC_PI_4, PI},
    path::PathBuf,
};

use besfa_protocol::{
    Button, ClipInfo, EntityInfo, LightPreset, Marker, MaterialInfo, ModelStats, Overlays,
    PointerEvent, PreviewModel, Report, View, clip_time,
};
use bevy::{
    asset::LoadState,
    camera::{Exposure, RenderTarget, primitives::Aabb, visibility::RenderLayers},
    gltf::{Gltf, GltfMaterialName},
    mesh::{PrimitiveTopology, skinning::SkinnedMesh},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
    transform::TransformSystems,
    world_serialization::{
        WorldAssetRoot, WorldInstance, WorldInstanceReady, WorldInstanceSpawner,
    },
};

use crate::{
    edit::subtree,
    inspect::send_report,
    scene_view::{AXIS_COLORS, EDITOR_CAMERA_ORDER, EditorCamera, EditorOnly},
    thumbnail,
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
    /// Frame the model on the next frame, from `view`.
    pub(crate) fit: bool,
    view: View,
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
    /// Every node's transform as the model loaded, for its pose without a
    /// clip.
    rest: Vec<(Entity, Transform)>,
    /// The part or root whose components are reported.
    pub(crate) selected: Option<Entity>,
    /// Meshes `preview_parts` hid, with the visibility they had.
    hid: Vec<(Entity, Visibility)>,
    overlays: Overlays,
    /// The model's skin joints and markers, for the overlays.
    joints: HashSet<Entity>,
    markers: Vec<Entity>,
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

/// A model shown apart from the scene: what it spawns goes on the root's
/// render layer and stays out of the Hierarchy and the scene file.
#[derive(Component)]
pub(crate) struct Isolated;

/// Orbits the previewed model the way the scene view's camera does.
#[derive(Component)]
pub(crate) struct PreviewCamera(EditorCamera);

/// The preview's light, by the preset the editor chose.
#[derive(Component, Default)]
struct PreviewLight(LightPreset);

/// The preview's grid, drawn on its layer only.
#[derive(Default, Reflect, GizmoConfigGroup)]
struct PreviewGizmos;

/// The overlays, drawn over the model.
#[derive(Default, Reflect, GizmoConfigGroup)]
struct OverlayGizmos;

const SKELETON_COLOR: Color = Color::srgb(1.0, 0.75, 0.25);
const MARKER_COLOR: Color = Color::srgb(0.95, 0.4, 0.95);
const BOUNDS_COLOR: Color = Color::srgb(0.55, 0.85, 1.0);

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
            .insert_gizmo_config(
                OverlayGizmos,
                GizmoConfig {
                    render_layers: layer(),
                    // In front of everything.
                    depth_bias: -1.0,
                    ..default()
                },
            )
            .add_systems(Startup, spawn_camera_and_light)
            .add_observer(respawned)
            .add_systems(
                Update,
                ((load, orbit, animate).chain(), lighting).run_if(showing),
            )
            .add_systems(
                PostUpdate,
                // After the scene spawner, before the Hierarchy is reported.
                draw.after(TransformSystems::Propagate).run_if(showing),
            )
            // After the scene spawner, before the Hierarchy is reported.
            .add_systems(PostUpdate, isolate);
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
        PreviewCamera(framing(
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
            View::default(),
        )),
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
                Isolated,
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
    let Some(mut preview) = world.get_resource_mut::<ModelPreview>() else {
        return;
    };
    let stopped = clip.is_none() && preview.animation.clip.is_some();
    preview.animation = Animation {
        clip,
        time,
        speed,
        looping,
    };
    // Without a clip the model goes back to how it loaded, once, so it can
    // be posed by hand.
    if stopped {
        let (root, rest) = (preview.root, preview.rest.clone());
        for entity in root.map(|root| subtree(world, root)).unwrap_or_default() {
            if let Some(mut player) = world.get_mut::<AnimationPlayer>(entity) {
                player.stop_all();
            }
        }
        for (entity, transform) in rest {
            if let Some(mut now) = world.get_mut::<Transform>(entity) {
                *now = transform;
            }
        }
    }
}

/// Picks the part whose components are reported; only the model's own.
pub(crate) fn select(world: &mut World, id: Option<u64>) -> Result<(), String> {
    let Some(mut preview) = world.get_resource_mut::<ModelPreview>() else {
        return Err("no model is previewed".into());
    };
    let Some(entity) = id.and_then(Entity::try_from_bits) else {
        preview.selected = None;
        return Ok(());
    };
    let root = preview.root.ok_or("no model is previewed")?;
    if !subtree(world, root).contains(&entity) {
        return Err(format!("{id:?} is not part of the previewed model"));
    }
    world.resource_mut::<ModelPreview>().selected = Some(entity);
    Ok(())
}

/// Picks what the preview draws over the model.
pub(crate) fn overlays(world: &mut World, overlays: Overlays) {
    if let Some(mut preview) = world.get_resource_mut::<ModelPreview>() {
        preview.overlays = overlays;
    }
}

/// Frames the model from `view`, and keeps to it when framing it again.
pub(crate) fn look(world: &mut World, view: View) {
    if let Some(mut preview) = world.get_resource_mut::<ModelPreview>() {
        preview.view = view;
        preview.fit = true;
    }
}

/// Saves what the preview's camera shows to `file` once it renders the
/// frame, and reports it as `path`.
pub(crate) fn screenshot(world: &mut World, path: String, file: PathBuf) -> Result<(), String> {
    let target = world
        .query_filtered::<(&Camera, &RenderTarget), With<PreviewCamera>>()
        .iter(world)
        .find(|(camera, _)| camera.is_active)
        .map(|(_, target)| target.clone())
        .ok_or("no model is previewed")?;
    // Headless, Bevy would skip the capture every frame and never report.
    if matches!(target, RenderTarget::Window(_))
        && world.query::<&Window>().iter(world).next().is_none()
    {
        return Err("the game has no window or viewport to picture the preview in".into());
    }
    world
        .spawn(Screenshot(target))
        .observe(move |captured: On<ScreenshotCaptured>| {
            let error = thumbnail::save(&captured.image, &file, true).err();
            match &error {
                None => info!("Saved {path}"),
                Some(error) => error!("Could not save {path}: {error}"),
            }
            send_report(&Report::Screenshot {
                path: path.clone(),
                error,
            });
        });
    Ok(())
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
    (*light, *transform, *ambient) = lit(preset.0, exposure);
}

/// The light, where it shines from and the ambient light of `preset`, as
/// bright under `exposure` as under Bevy's default.
pub(crate) fn lit(
    preset: LightPreset,
    exposure: Option<&Exposure>,
) -> (DirectionalLight, Transform, AmbientLight) {
    // Lux and cd/m².
    let (illuminance, color, from, brightness, sky) = match preset {
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
    (
        DirectionalLight {
            illuminance: illuminance * scale,
            color,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(from).looking_at(Vec3::ZERO, Vec3::Y),
        AmbientLight {
            color: sky,
            brightness: brightness * scale,
            ..default()
        },
    )
}

/// The model spawned anew, after its file was loaded again: it is reported
/// again with its new parts, and its clips play through a new graph.
fn respawned(ready: On<WorldInstanceReady>, mut preview: ResMut<ModelPreview>) {
    if preview.root != Some(ready.entity) || !preview.reported {
        return;
    }
    let preview = &mut *preview;
    preview.reported = false;
    preview.waited = 0;
    preview.clips = None;
    preview.rest.clear();
    preview.hid.clear();
    preview.selected = None;
    preview.joints.clear();
    preview.markers.clear();
}

/// Keeps what an isolated model spawns, whenever it spawns it, on the
/// model's layer and out of the Hierarchy.
fn isolate(
    mut commands: Commands,
    roots: Query<(Entity, &RenderLayers), With<Isolated>>,
    children: Query<&Children>,
    unmarked: Query<(), Without<EditorOnly>>,
) {
    for (root, layers) in &roots {
        for entity in children.iter_descendants(root) {
            if unmarked.contains(entity) {
                commands.entity(entity).insert((EditorOnly, layers.clone()));
            }
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
            model: PreviewModel {
                error: Some(error),
                ..default()
            },
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
        let rest = entities
            .iter()
            .filter_map(|&entity| Some((entity, *world.get::<Transform>(entity)?)))
            .collect();
        world.resource_mut::<ModelPreview>().rest = rest;
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
    let (stats, joints, markers) = anatomy(world, root, &entities);
    let marker_infos = markers
        .iter()
        .map(|&(marker, position)| Marker {
            id: marker.to_bits(),
            position: position.to_array(),
        })
        .collect();
    let joint_ids = joints.iter().map(|joint| joint.to_bits()).collect();
    let mut preview = world.resource_mut::<ModelPreview>();
    preview.reported = true;
    preview.joints = joints;
    preview.markers = markers.into_iter().map(|(marker, _)| marker).collect();
    if preview.bounds.is_none() {
        preview.bounds = bounds;
        preview.fit = true;
    }
    send_report(&Report::Preview {
        path,
        model: PreviewModel {
            root: Some(root.to_bits()),
            clips: infos,
            parts,
            size: bounds.map(|(min, max)| (max - min).to_array()),
            error: None,
            stats: Some(stats),
            joints: joint_ids,
            markers: marker_infos,
        },
    });
}

/// What the meshes under `root` are made of, their skin joints, and the
/// parts that mark a place with where they are: leaves without a mesh that
/// are not joints, lights or cameras.
fn anatomy(
    world: &World,
    root: Entity,
    entities: &[Entity],
) -> (ModelStats, HashSet<Entity>, Vec<(Entity, Vec3)>) {
    let meshes = world.resource::<Assets<Mesh>>();
    let materials = world.resource::<Assets<StandardMaterial>>();
    let mut stats = ModelStats::default();
    let mut used: Vec<(AssetId<StandardMaterial>, MaterialInfo)> = Vec::new();
    let mut joints = HashSet::new();
    for &entity in entities {
        if let Some(skin) = world.get::<SkinnedMesh>(entity) {
            joints.extend(skin.joints.iter().copied());
        }
        let Some(mesh) = world.get::<Mesh3d>(entity) else {
            continue;
        };
        stats.meshes += 1;
        if let Some(mesh) = meshes.get(&mesh.0) {
            let (vertices, triangles) = mesh_size(mesh);
            stats.vertices += vertices;
            stats.triangles += triangles;
        }
        let Some(material) = world.get::<MeshMaterial3d<StandardMaterial>>(entity) else {
            continue;
        };
        match used.iter_mut().find(|(id, _)| *id == material.id()) {
            Some((_, info)) => info.meshes += 1,
            None => {
                if let Some(asset) = materials.get(&material.0) {
                    let name = world.get::<GltfMaterialName>(entity);
                    let info = material_info(asset, name.map(|name| name.0.clone()));
                    used.push((material.id(), info));
                }
            }
        }
    }
    stats.materials = used.into_iter().map(|(_, info)| info).collect();

    let inverse = world
        .get::<GlobalTransform>(root)
        .map_or(bevy::math::Affine3A::IDENTITY, |root| {
            root.affine().inverse()
        });
    let marks = |entity: Entity| {
        world.get::<Children>(entity).is_none_or(|children| children.is_empty())
            // The glTF scene's own root.
            && world.get::<ChildOf>(entity).is_some_and(|parent| parent.parent() != root)
            && !joints.contains(&entity)
            && world.get::<Mesh3d>(entity).is_none()
            && world.get::<Camera>(entity).is_none()
            && world.get::<PointLight>(entity).is_none()
            && world.get::<SpotLight>(entity).is_none()
            && world.get::<DirectionalLight>(entity).is_none()
    };
    let markers = entities[1..]
        .iter()
        .copied()
        .filter(|&entity| marks(entity))
        .filter_map(|marker| {
            let global = world.get::<GlobalTransform>(marker)?;
            Some((marker, inverse.transform_point3(global.translation())))
        })
        .collect();
    (stats, joints, markers)
}

/// The vertices and triangles of `mesh`; none once only the GPU holds it.
fn mesh_size(mesh: &Mesh) -> (u64, u64) {
    let vertices = mesh
        .try_attribute(Mesh::ATTRIBUTE_POSITION)
        .map_or(0, |positions| positions.len());
    let corners = match mesh.try_indices_option() {
        Ok(Some(indices)) => indices.len(),
        _ => vertices,
    };
    let triangles = match mesh.primitive_topology() {
        PrimitiveTopology::TriangleList => corners / 3,
        PrimitiveTopology::TriangleStrip => corners.saturating_sub(2),
        _ => 0,
    };
    (vertices as u64, triangles as u64)
}

fn material_info(material: &StandardMaterial, name: Option<String>) -> MaterialInfo {
    let textures = [
        ("base_color", &material.base_color_texture),
        ("normal", &material.normal_map_texture),
        ("metallic_roughness", &material.metallic_roughness_texture),
        ("emissive", &material.emissive_texture),
        ("occlusion", &material.occlusion_texture),
    ];
    MaterialInfo {
        name,
        color: material.base_color.to_srgba().to_f32_array(),
        textures: textures
            .into_iter()
            .filter(|(_, texture)| texture.is_some())
            .map(|(name, _)| name.into())
            .collect(),
        meshes: 1,
    }
}

/// The world bounds of the meshes among `entities`.
pub(crate) fn bounds(world: &World, entities: &[Entity]) -> Option<(Vec3, Vec3)> {
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

/// Looking at the model from `view`, near enough to fill the view with it.
/// Models made for Bevy face its forward, -Z; the camera sits along its own
/// +Z, so a yaw of π puts it in front.
pub(crate) fn framing(min: Vec3, max: Vec3, view: View) -> EditorCamera {
    let radius = ((max - min).length() * 0.5).max(0.05);
    // Half of Bevy's default vertical field of view, 45°.
    let half_view = FRAC_PI_2 * 0.25;
    let (yaw, pitch) = match view {
        View::Front => (PI, 0.0),
        View::Back => (0.0, 0.0),
        View::Left => (-FRAC_PI_2, 0.0),
        View::Right => (FRAC_PI_2, 0.0),
        // Straight down, with the model's front at the bottom.
        View::Top => (PI, -FRAC_PI_2 + 0.001),
        View::ThreeQuarter => (PI - FRAC_PI_4, -0.35),
    };
    EditorCamera {
        focus: (min + max) * 0.5,
        yaw,
        pitch,
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
    // Once the model is reported, with or without meshes to frame.
    if preview.fit && preview.reported {
        preview.fit = false;
        let (min, max) = preview
            .bounds
            .unwrap_or((Vec3::splat(-0.5), Vec3::splat(0.5)));
        camera.0 = framing(min, max, preview.view);
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

/// A ground grid a few times the model's size, in meters, the origin's
/// axes, and the overlays asked for.
fn draw(
    preview: Res<ModelPreview>,
    mut gizmos: Gizmos<PreviewGizmos>,
    mut over: Gizmos<OverlayGizmos>,
    nodes: Query<(&GlobalTransform, Option<&ChildOf>)>,
) {
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

    let small = (extent * 0.01).max(0.002);
    if preview.overlays.bounds
        && let Some((min, max)) = preview.bounds
    {
        let box_ = Transform::from_translation((min + max) * 0.5).with_scale(max - min);
        over.cube(box_, BOUNDS_COLOR);
    }
    if preview.overlays.skeleton {
        for &joint in &preview.joints {
            let Ok((global, parent)) = nodes.get(joint) else {
                continue;
            };
            over.sphere(
                Isometry3d::from_translation(global.translation()),
                small,
                SKELETON_COLOR,
            );
            if let Some(parent) = parent.filter(|parent| preview.joints.contains(&parent.parent()))
                && let Ok((parent, _)) = nodes.get(parent.parent())
            {
                over.line(parent.translation(), global.translation(), SKELETON_COLOR);
            }
        }
    }
    if preview.overlays.markers {
        for &marker in &preview.markers {
            if let Ok((global, _)) = nodes.get(marker) {
                over.axes(*global, small * 6.0);
                over.sphere(
                    Isometry3d::from_translation(global.translation()),
                    small,
                    MARKER_COLOR,
                );
            }
        }
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
                PreviewCamera(framing(Vec3::ZERO, Vec3::ONE, View::Front)),
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
    fn tells_markers_from_joints_meshes_and_lights() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        let root = world.spawn(GlobalTransform::default()).id();
        let scene = world.spawn(ChildOf(root)).id();
        let at = |x: f32| GlobalTransform::from_translation(Vec3::X * x);
        let hand = world.spawn((ChildOf(scene), at(1.0))).id();
        let grip = world.spawn((ChildOf(hand), at(2.0))).id();
        let finger = world.spawn((ChildOf(hand), at(3.0))).id();
        let body = world.spawn((ChildOf(scene), at(0.0))).id();
        let mesh = world.spawn((ChildOf(body), Mesh3d::default())).id();
        world.entity_mut(mesh).insert(SkinnedMesh {
            inverse_bindposes: default(),
            joints: vec![hand, finger],
        });
        world.spawn((ChildOf(scene), PointLight::default(), at(4.0)));
        let entities = subtree(&world, root);

        let (stats, joints, markers) = anatomy(&world, root, &entities);
        assert_eq!(stats.meshes, 1);
        assert_eq!(joints, HashSet::from([hand, finger]));
        assert_eq!(markers, [(grip, Vec3::new(2.0, 0.0, 0.0))]);
    }

    #[test]
    fn frames_the_whole_model() {
        let (min, max) = (Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 2.0, 0.5));
        let camera = framing(min, max, View::ThreeQuarter);
        assert_eq!(camera.focus, Vec3::new(0.0, 1.0, 0.0));
        let eye = camera.transform().translation;
        assert!(
            eye.z < 0.0 && eye.x > 0.0 && eye.y > 1.0,
            "in front, to the model's right and above: {eye}"
        );
        assert!(eye.distance(camera.focus) > 1.2, "the model fits");
        let from = |view| {
            let camera = framing(min, max, view);
            (camera.transform().translation - camera.focus).normalize()
        };
        assert!(from(View::Front).abs_diff_eq(Vec3::NEG_Z, 1e-3));
        assert!(from(View::Back).abs_diff_eq(Vec3::Z, 1e-3));
        assert!(from(View::Right).abs_diff_eq(Vec3::X, 1e-3));
        assert!(from(View::Left).abs_diff_eq(Vec3::NEG_X, 1e-3));
        assert!(from(View::Top).abs_diff_eq(Vec3::Y, 1e-2));
    }
}
