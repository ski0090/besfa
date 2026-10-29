//! Running a game with no window: driven by the editor or `besfa_mcp`, or
//! as a dedicated server.

use std::time::Duration;

use bevy::{
    app::{PluginGroupBuilder, ScheduleRunnerPlugin},
    prelude::*,
    window::ExitCondition,
    winit::WinitPlugin,
};

/// `plugins` with no window, ticking `hz` times a second, since without a
/// window winit has nothing to drive. Rendering stays, into images.
pub fn windowless(plugins: PluginGroupBuilder, hz: f64) -> PluginGroupBuilder {
    plugins
        .set(WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..default()
        })
        .disable::<WinitPlugin>()
        .add(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / hz,
        )))
}
