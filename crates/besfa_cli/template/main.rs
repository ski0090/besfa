use bevy::prelude::*;

fn main() -> AppExit {
    App::new()
        // DefaultPlugins that load the scene from assets/scenes/main.scn.ron
        // and connect to the Besfa editor when it runs the game.
        .add_plugins(besfa_editor_plugin::default_plugins())
        .add_systems(Update, spin)
        .run()
}

/// Spins an entity around Y; the scene file says which entities have it.
#[derive(Component, Reflect)]
#[reflect(Component)]
struct Spin;

fn spin(time: Res<Time>, mut query: Query<&mut Transform, With<Spin>>) {
    for mut transform in &mut query {
        transform.rotate_y(time.delta_secs());
    }
}
