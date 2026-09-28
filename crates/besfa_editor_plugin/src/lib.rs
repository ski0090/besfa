//! Connects a Bevy game to the Besfa editor.
//!
//! Games use [`default_plugins`] in place of `DefaultPlugins`. Run on its own,
//! the game behaves exactly like `DefaultPlugins`. Launched by the editor, it
//! renders into the editor's viewport instead of opening a window.

use std::time::Duration;

use bevy::{
    app::{PluginGroupBuilder, ScheduleRunnerPlugin},
    prelude::*,
    window::ExitCondition,
    winit::WinitPlugin,
};

mod viewport;

/// `DefaultPlugins`, adjusted to render into the editor viewport when the
/// editor launched the game.
pub fn default_plugins() -> PluginGroupBuilder {
    let plugins = DefaultPlugins.build();
    let Some(config) = viewport::ViewportConfig::from_env() else {
        return plugins;
    };

    plugins
        .set(WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..default()
        })
        // Without a window winit has nothing to drive, so tick at a fixed rate.
        .disable::<WinitPlugin>()
        .add(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .add(viewport::ViewportPlugin(config))
}
