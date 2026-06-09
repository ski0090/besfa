use crate::{
    BesfaRuntimeIpcPlugin, PreviewRuntimeOptions,
    external_preview::{
        BesfaExternalPreviewPlugin, create_camera_preview_surface_image,
        create_preview_surface_image,
    },
    scene_file::{
        PreviewSceneSource, SceneEntityDefinition, SceneLoadStatus, SceneMeshDefinition,
        SceneMeshPrimitive, ScenePointLightDefinition, load_scene_file,
    },
};
use bevy::{
    asset::Assets,
    camera::RenderTarget,
    prelude::*,
    render::{
        RenderPlugin,
        settings::{Backends, RenderCreation, WgpuSettings},
    },
    window::{PresentMode, WindowPosition},
};
use std::f32::consts::FRAC_PI_2;

/// Runs a standalone Bevy preview app.
pub fn run(options: PreviewRuntimeOptions) {
    let PreviewRuntimeOptions { ipc, scene_path } = options;
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Besfa Preview".into(),
                    resolution: (960, 640).into(),
                    present_mode: PresentMode::AutoVsync,
                    position: WindowPosition::At(IVec2::new(-32_000, -32_000)),
                    visible: true,
                    focused: false,
                    skip_taskbar: true,
                    ..default()
                }),
                ..default()
            })
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(WgpuSettings {
                    backends: Some(Backends::DX12),
                    ..default()
                }),
                ..default()
            }),
    )
    .insert_resource(PreviewSceneSource::from_path(scene_path))
    .add_plugins((BesfaPreviewPlugin, BesfaExternalPreviewPlugin));

    if let Some(ipc_config) = ipc {
        app.add_plugins(BesfaRuntimeIpcPlugin::new(ipc_config));
    }

    app.run();
}

/// Bevy plugin that installs the current placeholder preview scene.
pub struct BesfaPreviewPlugin;

impl Plugin for BesfaPreviewPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.06, 0.07, 0.08)))
            .init_resource::<PreviewSceneObjects>()
            .init_resource::<PreviewPlaybackState>()
            .add_systems(Startup, (setup_scene, pause_game_time_on_startup).chain())
            .add_systems(Update, (draw_grid, rotate_preview_spinners));
    }
}

#[derive(Resource, Default)]
pub(crate) struct PreviewSceneObjects {
    next_cube_index: u64,
}

impl PreviewSceneObjects {
    pub(crate) fn reset(&mut self) {
        self.next_cube_index = 0;
    }

    pub(crate) fn observe_scene_entity_id(&mut self, id: &str) {
        let Some(index) = id
            .strip_prefix("cube_")
            .and_then(|suffix| suffix.parse::<u64>().ok())
        else {
            return;
        };

        self.next_cube_index = self.next_cube_index.max(index);
    }

    pub(crate) fn next_cube(&mut self) -> (String, String, Vec3) {
        self.next_cube_index += 1;
        let index = self.next_cube_index;
        let row = ((index - 1) / 5) as f32;
        let column = ((index - 1) % 5) as f32;
        let position = Vec3::new(column * 1.8 - 3.6, 0.5, row * 1.8 - 1.8);

        (format!("cube_{index}"), format!("Cube {index}"), position)
    }
}

#[derive(Resource, Default)]
pub(crate) struct PreviewPlaybackState {
    pub(crate) playing: bool,
}

#[derive(Component, Debug, Clone)]
pub(crate) struct PreviewSceneNode {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub parent_id: Option<String>,
}

impl PreviewSceneNode {
    pub(crate) fn root(
        id: impl Into<String>,
        name: impl Into<String>,
        kind: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: kind.into(),
            parent_id: None,
        }
    }

    pub(crate) fn child(
        id: impl Into<String>,
        name: impl Into<String>,
        kind: impl Into<String>,
        parent_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: kind.into(),
            parent_id: Some(parent_id.into()),
        }
    }
}

#[derive(Component)]
pub(crate) struct PreviewSpinner {
    pub(crate) y_radians_per_second: f32,
}

/// Runtime-only camera used to render the editor Scene View.
#[derive(Component)]
pub(crate) struct EditorPreviewCamera;

/// Runtime-only camera used to render the selected scene camera preview.
#[derive(Component)]
pub(crate) struct SelectedCameraPreviewCamera;

/// Runtime-side pick bounds for a scene object.
#[derive(Component)]
pub(crate) struct PreviewPickTarget {
    /// Local axis-aligned half extents used for initial viewport ray picking.
    pub(crate) half_extents: Vec3,
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    scene_source: Res<PreviewSceneSource>,
    mut scene_objects: ResMut<PreviewSceneObjects>,
) {
    let (preview_surface_image, preview_surface_target) = create_preview_surface_image(&mut images);
    let (camera_preview_surface_image, camera_preview_surface_target) =
        create_camera_preview_surface_image(&mut images);
    commands.insert_resource(preview_surface_target);
    commands.insert_resource(camera_preview_surface_target);

    let camera_transform =
        Transform::from_xyz(-4.5, 4.2, 7.5).looking_at(Vec3::new(0.0, 0.6, 0.0), Vec3::Y);

    commands.spawn((
        Camera3d::default(),
        RenderTarget::from(preview_surface_image),
        camera_transform,
        EditorPreviewCamera,
        Name::new("Editor Preview Camera"),
    ));

    commands.spawn((
        Camera3d::default(),
        Camera {
            is_active: false,
            ..default()
        },
        RenderTarget::from(camera_preview_surface_image),
        camera_transform,
        SelectedCameraPreviewCamera,
        Name::new("Selected Camera Preview"),
    ));

    spawn_preview_scene(
        &mut commands,
        &scene_source,
        &mut scene_objects,
        &mut meshes,
        &mut materials,
    );
}

fn pause_game_time_on_startup(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

pub(crate) fn reload_preview_scene(
    commands: &mut Commands,
    scene_source: &PreviewSceneSource,
    scene_objects: &mut PreviewSceneObjects,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    scene_entities: &Query<(Entity, &PreviewSceneNode)>,
) -> SceneLoadStatus {
    for (entity, _) in scene_entities.iter() {
        commands.entity(entity).despawn();
    }

    spawn_preview_scene(commands, scene_source, scene_objects, meshes, materials)
}

fn spawn_preview_scene(
    commands: &mut Commands,
    scene_source: &PreviewSceneSource,
    scene_objects: &mut PreviewSceneObjects,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> SceneLoadStatus {
    scene_objects.reset();
    let loaded = load_scene_file(scene_source);
    for entity in &loaded.scene.entities {
        if entity.id.trim().is_empty() {
            continue;
        }

        scene_objects.observe_scene_entity_id(&entity.id);
        spawn_scene_entity(commands, meshes, materials, entity);
    }

    loaded.status
}

fn spawn_scene_entity(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    entity: &SceneEntityDefinition,
) {
    if let Some(mesh) = &entity.mesh {
        spawn_mesh_entity(commands, meshes, materials, entity, mesh);
    } else if let Some(light) = entity.light {
        spawn_light_entity(commands, entity, light);
    } else if entity.camera.is_some() {
        spawn_camera_entity(commands, entity);
    } else {
        commands.spawn((Name::new(display_name(entity)), scene_node(entity)));
    }
}

fn spawn_mesh_entity(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    entity: &SceneEntityDefinition,
    mesh: &SceneMeshDefinition,
) {
    let mut spawned = commands.spawn((
        Mesh3d(meshes.add(scene_mesh(mesh))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: entity.material.base_color.to_color(),
            metallic: finite_or_default(entity.material.metallic, 0.05),
            perceptual_roughness: finite_or_default(entity.material.perceptual_roughness, 0.6),
            ..default()
        })),
        entity.transform.to_transform(),
        Name::new(display_name(entity)),
        scene_node(entity),
    ));

    if let Some(half_extents) = entity.pick_half_extents {
        spawned.insert(PreviewPickTarget {
            half_extents: half_extents.to_vec3(),
        });
    }

    if let Some(speed) = entity.spin_y_radians_per_second
        && speed.is_finite()
        && speed != 0.0
    {
        spawned.insert(PreviewSpinner {
            y_radians_per_second: speed,
        });
    }
}

fn spawn_light_entity(
    commands: &mut Commands,
    entity: &SceneEntityDefinition,
    light: ScenePointLightDefinition,
) {
    commands.spawn((
        PointLight {
            intensity: finite_or_default(light.intensity, 800_000.0),
            range: finite_or_default(light.range, 20.0),
            shadows_enabled: light.shadows_enabled,
            ..default()
        },
        entity.transform.to_transform(),
        Name::new(display_name(entity)),
        scene_node(entity),
    ));
}

fn spawn_camera_entity(commands: &mut Commands, entity: &SceneEntityDefinition) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            is_active: false,
            ..default()
        },
        entity.transform.to_transform(),
        Name::new(display_name(entity)),
        scene_node(entity),
    ));
}

fn scene_mesh(mesh: &SceneMeshDefinition) -> Mesh {
    match mesh.primitive {
        SceneMeshPrimitive::Plane => {
            let width = positive_or_default(mesh.size.x, 20.0);
            let depth = positive_or_default(mesh.size.z, 20.0);
            Plane3d::default().mesh().size(width, depth).into()
        }
        SceneMeshPrimitive::Cube => Cuboid::new(
            positive_or_default(mesh.size.x, 1.0),
            positive_or_default(mesh.size.y, 1.0),
            positive_or_default(mesh.size.z, 1.0),
        )
        .into(),
    }
}

fn scene_node(entity: &SceneEntityDefinition) -> PreviewSceneNode {
    match entity.parent_id.as_deref() {
        Some(parent_id) => PreviewSceneNode::child(
            entity.id.clone(),
            display_name(entity),
            entity.kind.clone(),
            parent_id.to_string(),
        ),
        None => {
            PreviewSceneNode::root(entity.id.clone(), display_name(entity), entity.kind.clone())
        }
    }
}

fn display_name(entity: &SceneEntityDefinition) -> String {
    if entity.name.trim().is_empty() {
        entity.id.clone()
    } else {
        entity.name.clone()
    }
}

fn draw_grid(mut gizmos: Gizmos) {
    gizmos.grid(
        Quat::from_rotation_x(FRAC_PI_2),
        UVec2::splat(20),
        Vec2::splat(1.0),
        LinearRgba::gray(0.38),
    );
}

fn rotate_preview_spinners(
    playback: Res<PreviewPlaybackState>,
    mut spinners: Query<(&PreviewSpinner, &mut Transform)>,
    time: Res<Time>,
) {
    if !playback.playing {
        return;
    }

    let delta_secs = time.delta_secs();
    if delta_secs == 0.0 {
        return;
    }

    for (spinner, mut transform) in &mut spinners {
        transform.rotate_y(delta_secs * spinner.y_radians_per_second);
    }
}

fn positive_or_default(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn finite_or_default(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}
