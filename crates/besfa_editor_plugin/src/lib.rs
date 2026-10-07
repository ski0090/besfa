//! Connects a Bevy game to the Besfa editor.
//!
//! Games use [`default_plugins`] in place of `DefaultPlugins`. Run on its own,
//! the game behaves like `DefaultPlugins` and loads its scene from
//! `assets/scenes/main.scn.ron`. Launched by the editor, it renders into the
//! editor's viewport instead of opening a window, starts paused in edit mode
//! until the editor says play, reports its entities, components and systems
//! to the editor, and saves the scene back when asked.

use std::time::Duration;

use bevy::{
    app::{PluginGroupBuilder, ScheduleRunnerPlugin},
    prelude::*,
    window::ExitCondition,
    winit::WinitPlugin,
};

mod edit;
mod edit_mode;
mod inspect;
mod scene;
mod scene_view;
mod viewport;

pub use scene::{MeshColor, MeshShape, SCENE_PATH, SceneEntity};
pub use scene_view::EditorOnly;

/// `DefaultPlugins` plus the scene file, adjusted to start in edit mode and
/// render into the editor viewport when the editor launched the game.
pub fn default_plugins() -> PluginGroupBuilder {
    let mut plugins = DefaultPlugins.build().add(scene::ScenePlugin);
    if edit_mode::requested() {
        plugins = plugins
            .add(edit_mode::EditModePlugin)
            .add(inspect::InspectPlugin)
            .add(scene_view::SceneViewPlugin);
    }
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
