use bevy::prelude::*;

fn main() -> AppExit {
    App::new()
        // DefaultPlugins that load the scene from assets/scenes/main.scn.ron,
        // and connect to the Besfa editor when it runs the game.
        .add_plugins(plugins())
        .add_systems(Update, spin)
        .run()
}

/// The editor runs the game with the `besfa_editor` feature, which a
/// release build leaves out.
#[cfg(feature = "besfa_editor")]
fn plugins() -> bevy::app::PluginGroupBuilder {
    besfa_editor_plugin::default_plugins()
}

#[cfg(not(feature = "besfa_editor"))]
fn plugins() -> bevy::app::PluginGroupBuilder {
    besfa_components::default_plugins()
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
