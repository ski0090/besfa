//! Connects a Bevy game to the Besfa editor.
//!
//! Games use [`default_plugins`] in place of `DefaultPlugins`. Run on its own,
//! the game behaves like `DefaultPlugins` and loads its scene from
//! `assets/scenes/main.scn.ron`. Launched by the editor, it renders into the
//! editor's viewport instead of opening a window, starts paused in edit mode
//! until the editor says play, reports its entities, components and systems
//! to the editor, and saves the scene back when asked. Launched by
//! `besfa_mcp`, it runs the same way with no window at all.

use std::time::Duration;

use bevy::{
    app::{PluginGroupBuilder, ScheduleRunnerPlugin},
    prelude::*,
    window::ExitCondition,
    winit::WinitPlugin,
};

mod animation;
mod edit;
mod edit_mode;
mod ground;
mod input;
mod inspect;
mod part_changes;
mod preview;
mod reimport;
mod scene;
mod scene_view;
mod shot;
mod terrain;
mod thumbnail;
mod viewport;

pub use animation::{
    AnimationClips, AnimationCondition, AnimationParams, AnimationState, AnimationStateMachine,
    AnimationStateMachinePlugin, AnimationTransition, ConditionTest,
};
pub use ground::{ScatterGroup, Scattered};
pub use part_changes::{PartChange, PartChanges};
pub use reimport::Reimported;
pub use scene::{MeshColor, MeshShape, SCENE_PATH, SceneEntity};
pub use scene_view::EditorOnly;
pub use terrain::{Terrain, TerrainData, TerrainFile, TerrainHandle, TerrainHeight};

/// `DefaultPlugins` plus the scene file, adjusted to start in edit mode and
/// render into the editor viewport when the editor launched the game, or
/// into no window under `BESFA_HEADLESS`.
pub fn default_plugins() -> PluginGroupBuilder {
    // Saved changes to placed models' parts apply in play as well.
    let mut plugins = DefaultPlugins
        .build()
        .add(scene::ScenePlugin)
        .add(part_changes::PartChangesPlugin)
        .add(terrain::TerrainPlugin)
        .add(ground::GroundPlugin)
        .add(animation::AnimationStateMachinePlugin);
    if edit_mode::requested() {
        plugins = plugins
            .add(edit_mode::EditModePlugin)
            .add(inspect::InspectPlugin)
            .add(scene_view::SceneViewPlugin)
            .add(preview::PreviewPlugin)
            .add(reimport::ReimportPlugin)
            .add(thumbnail::ThumbnailPlugin)
            .add(shot::ShotPlugin);
    }
    let viewport = viewport::ViewportConfig::from_env();
    // Set by besfa_mcp, so a game it drives opens no window to take focus.
    if viewport.is_none() && std::env::var_os("BESFA_HEADLESS").is_none() {
        return plugins;
    }

    plugins = plugins
        .set(WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..default()
        })
        // Without a window winit has nothing to drive, so tick at a fixed rate.
        .disable::<WinitPlugin>()
        .add(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    match viewport {
        Some(config) => plugins.add(viewport::ViewportPlugin(config)),
        None => plugins,
    }
}
