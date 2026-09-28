use bevy::prelude::*;

fn main() {
    App::new()
        // DefaultPlugins that render into the Besfa editor when it runs the game.
        .add_plugins(besfa_editor_plugin::default_plugins())
        .add_systems(Startup, setup)
        .add_systems(Update, spin)
        .run();
}

#[derive(Component)]
struct Spin;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Name::new("Cube"),
        Mesh3d(meshes.add(Cuboid::default())),
        MeshMaterial3d(materials.add(Color::srgb(0.55, 0.7, 1.0))),
        Spin,
    ));
    commands.spawn((
        Name::new("Light"),
        PointLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
    commands.spawn((
        Name::new("Camera"),
        Camera3d::default(),
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spin(time: Res<Time>, mut query: Query<&mut Transform, With<Spin>>) {
    for mut transform in &mut query {
        transform.rotate_y(time.delta_secs());
    }
}
