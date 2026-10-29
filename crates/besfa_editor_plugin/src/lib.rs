//! Connects a Bevy game to the Besfa editor.
//!
//! Games use [`default_plugins`] in place of `DefaultPlugins`. Run on its own,
//! the game behaves like `DefaultPlugins` and loads its scene from
//! `assets/scenes/main.scn.ron`. Launched by the editor, it renders into the
//! editor's viewport instead of opening a window, starts paused in edit mode
//! until the editor says play, reports its entities, components and systems
//! to the editor, and saves the scene back when asked. Launched by
//! `besfa_mcp`, it runs the same way with no window at all.

use bevy::app::PluginGroupBuilder;

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

// The game's components, which games built before `besfa_components` name
// through this crate.
pub(crate) use besfa_components::sky;
pub use besfa_components::{
    AnimationClips, AnimationCondition, AnimationParams, AnimationState, AnimationStateMachine,
    AnimationStateMachinePlugin, AnimationTransition, ConditionTest, EditorOnly, MeshColor,
    MeshShape, Moon, PartChange, PartChanges, Reimported, SCENE_PATH, ScatterGroup, Scattered,
    SceneEntity, Sky, SkyMode, Sun, Terrain, TerrainData, TerrainFile, TerrainHandle,
    TerrainHeight,
};

/// `besfa_components::default_plugins`, adjusted to start in edit mode and
/// render into the editor viewport when the editor launched the game, or
/// into no window under `BESFA_HEADLESS`.
pub fn default_plugins() -> PluginGroupBuilder {
    let mut plugins = besfa_components::default_plugins();
    if std::env::var_os("BESFA_EDIT_MODE").is_some() {
        plugins = plugins
            .add(edit_mode::EditModePlugin)
            .add(terrain::TerrainBrushPlugin)
            .add(inspect::InspectPlugin)
            .add(scene_view::SceneViewPlugin)
            .add(preview::PreviewPlugin)
            .add(reimport::ReimportPlugin)
            .add(thumbnail::ThumbnailPlugin)
            .add(shot::ShotPlugin);
    }
    // The editor's shared texture, which the game renders into.
    let viewport = std::env::var_os("BESFA_VIEWPORT").is_some();
    // Set by besfa_mcp, so a game it drives opens no window to take focus.
    if !viewport && std::env::var_os("BESFA_HEADLESS").is_none() {
        return plugins;
    }

    plugins = besfa_components::windowless(plugins, 60.0);
    if viewport {
        plugins.add(viewport::ViewportPlugin)
    } else {
        plugins
    }
}
