//! Running a game with no window: driven by the editor or `besfa_mcp`, or
//! as a dedicated server.

use std::time::Duration;

use bevy::{
    app::{PluginGroupBuilder, ScheduleRunnerPlugin},
    prelude::*,
    render::{Render, RenderApp, RenderSystems, render_resource::PollType, renderer::RenderDevice},
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
        .add(PaceToGpu)
}

/// Holds each frame until the GPU has drawn it, as presenting to a window
/// would. Without a window nothing else waits for the GPU: when it draws
/// slower than `hz`, frames queue up with the memory they hold, gigabytes
/// a minute, until the device runs out of memory.
struct PaceToGpu;

impl Plugin for PaceToGpu {
    fn build(&self, app: &mut App) {
        // None when the game renders nothing, as a dedicated server.
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(Render, wait_for_gpu.in_set(RenderSystems::Cleanup));
        }
    }
}

// ponytail: waits for everything submitted, so the GPU idles while the next
// frame is prepared; wait for the frame before's submission instead if a
// windowless game needs the throughput.
fn wait_for_gpu(device: Res<RenderDevice>) {
    if let Err(error) = device.poll(PollType::wait_indefinitely()) {
        warn!("Could not wait for the GPU: {error}");
    }
}
