use crate::{
    BesfaRuntimeIpcPlugin, PreviewRuntimeOptions,
    external_preview::{BesfaExternalPreviewPlugin, create_preview_surface_image},
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
use std::f32::consts::{FRAC_PI_2, PI};

/// Runs a standalone Bevy preview app.
pub fn run(options: PreviewRuntimeOptions) {
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
    .add_plugins((BesfaPreviewPlugin, BesfaExternalPreviewPlugin));

    if let Some(ipc_config) = options.ipc {
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
            .add_systems(Startup, setup_scene)
            .add_systems(Update, (draw_grid, draw_world_axes, rotate_preview_cube));
    }
}

#[derive(Resource, Default)]
pub(crate) struct PreviewSceneObjects {
    next_cube_index: u64,
}

impl PreviewSceneObjects {
    pub(crate) fn next_cube(&mut self) -> (String, String, Vec3) {
        self.next_cube_index += 1;
        let index = self.next_cube_index;
        let row = ((index - 1) / 5) as f32;
        let column = ((index - 1) % 5) as f32;
        let position = Vec3::new(column * 1.8 - 3.6, 0.5, row * 1.8 - 1.8);

        (format!("cube_{index}"), format!("Cube {index}"), position)
    }
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
struct PreviewCube;

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
) {
    let (preview_surface_image, preview_surface_target) = create_preview_surface_image(&mut images);
    commands.insert_resource(preview_surface_target);

    commands.spawn((
        Name::new("World"),
        PreviewSceneNode::root("world", "World", "world"),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(20.0, 20.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.14, 0.14),
            perceptual_roughness: 0.9,
            ..default()
        })),
        PreviewPickTarget {
            half_extents: Vec3::new(10.0, 0.02, 10.0),
        },
        Name::new("Ground"),
        PreviewSceneNode::child("ground", "Ground", "mesh", "world"),
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
        PreviewPickTarget {
            half_extents: Vec3::splat(0.7),
        },
        Name::new("Preview Cube"),
        PreviewSceneNode::child("preview_cube", "Preview Cube", "mesh", "world"),
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
        PreviewSceneNode::child("key_light", "Key Light", "light", "world"),
    ));

    commands.spawn((
        Camera3d::default(),
        RenderTarget::from(preview_surface_image),
        Transform::from_xyz(-4.5, 4.2, 7.5).looking_at(Vec3::new(0.0, 0.6, 0.0), Vec3::Y),
        Name::new("Camera3d"),
        PreviewSceneNode::child("camera_3d", "Camera3d", "camera", "world"),
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

fn draw_world_axes(mut gizmos: Gizmos) {
    const POSITIVE_AXIS_LENGTH: f32 = 2.8;
    const NEGATIVE_AXIS_LENGTH: f32 = 1.2;
    const TIP_LENGTH: f32 = 0.28;

    let origin = Vec3::ZERO;
    let x_color = Color::srgb(0.95, 0.16, 0.12);
    let y_color = Color::srgb(0.25, 0.86, 0.32);
    let z_color = Color::srgb(0.22, 0.48, 1.0);
    let faded_x = Color::srgba(0.95, 0.16, 0.12, 0.45);
    let faded_y = Color::srgba(0.25, 0.86, 0.32, 0.45);
    let faded_z = Color::srgba(0.22, 0.48, 1.0, 0.45);

    gizmos.line(origin, -Vec3::X * NEGATIVE_AXIS_LENGTH, faded_x);
    gizmos.line(origin, -Vec3::Y * NEGATIVE_AXIS_LENGTH, faded_y);
    gizmos.line(origin, -Vec3::Z * NEGATIVE_AXIS_LENGTH, faded_z);
    gizmos
        .arrow(origin, Vec3::X * POSITIVE_AXIS_LENGTH, x_color)
        .with_tip_length(TIP_LENGTH);
    gizmos
        .arrow(origin, Vec3::Y * POSITIVE_AXIS_LENGTH, y_color)
        .with_tip_length(TIP_LENGTH);
    gizmos
        .arrow(origin, Vec3::Z * POSITIVE_AXIS_LENGTH, z_color)
        .with_tip_length(TIP_LENGTH);
}

fn rotate_preview_cube(mut cubes: Query<&mut Transform, With<PreviewCube>>, time: Res<Time>) {
    for mut transform in &mut cubes {
        transform.rotate_y(time.delta_secs() * 0.6);
    }
}
