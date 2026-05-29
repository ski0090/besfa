use crate::{BesfaRuntimeIpcPlugin, PreviewRuntimeOptions};
use bevy::{
    prelude::*,
    render::{
        RenderPlugin,
        settings::{Backends, RenderCreation, WgpuSettings},
    },
    window::PresentMode,
};
use std::f32::consts::{FRAC_PI_2, PI};

pub fn run(options: PreviewRuntimeOptions) {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Besfa Preview".into(),
                    resolution: (960, 640).into(),
                    present_mode: PresentMode::AutoVsync,
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
    .add_plugins(BesfaPreviewPlugin);

    if let Some(ipc_config) = options.ipc {
        app.add_plugins(BesfaRuntimeIpcPlugin::new(ipc_config));
    }

    app.run();
}

pub struct BesfaPreviewPlugin;

impl Plugin for BesfaPreviewPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.06, 0.07, 0.08)))
            .add_systems(Startup, setup_scene)
            .add_systems(Update, (draw_grid, rotate_preview_cube));
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct PreviewSceneNode {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    pub parent_id: Option<&'static str>,
}

impl PreviewSceneNode {
    pub const fn new(
        id: &'static str,
        name: &'static str,
        kind: &'static str,
        parent_id: Option<&'static str>,
    ) -> Self {
        Self {
            id,
            name,
            kind,
            parent_id,
        }
    }
}

#[derive(Component)]
struct PreviewCube;

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Name::new("World"),
        PreviewSceneNode::new("world", "World", "world", None),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(20.0, 20.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.14, 0.14),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Name::new("Ground"),
        PreviewSceneNode::new("ground", "Ground", "mesh", Some("world")),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.4, 1.4, 1.4))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.08, 0.56, 0.47),
            metallic: 0.1,
            perceptual_roughness: 0.55,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.7, 0.0).with_rotation(Quat::from_rotation_y(PI / 4.0)),
        PreviewCube,
        Name::new("Preview Cube"),
        PreviewSceneNode::new("preview_cube", "Preview Cube", "mesh", Some("world")),
    ));

    commands.spawn((
        PointLight {
            intensity: 2_800_000.0,
            range: 40.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 7.0, 5.0),
        Name::new("Key Light"),
        PreviewSceneNode::new("key_light", "Key Light", "light", Some("world")),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-4.5, 4.2, 7.5).looking_at(Vec3::new(0.0, 0.6, 0.0), Vec3::Y),
        Name::new("Camera3d"),
        PreviewSceneNode::new("camera_3d", "Camera3d", "camera", Some("world")),
    ));
}

fn draw_grid(mut gizmos: Gizmos) {
    gizmos.grid(
        Quat::from_rotation_x(FRAC_PI_2),
        UVec2::splat(20),
        Vec2::splat(1.0),
        LinearRgba::gray(0.38),
    );
}

fn rotate_preview_cube(mut cubes: Query<&mut Transform, With<PreviewCube>>, time: Res<Time>) {
    for mut transform in &mut cubes {
        transform.rotate_y(time.delta_secs() * 0.6);
    }
}
