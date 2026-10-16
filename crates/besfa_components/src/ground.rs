//! Markers for what the editor's scatter brush placed.

use bevy::prelude::*;

/// A thing scatter placed, which erasing may take away again; things
/// placed by hand have none.
#[derive(Component, Reflect, Clone, Copy, Debug, Default)]
#[type_path = "besfa_editor_plugin::ground"]
#[reflect(Component, Default)]
pub struct Scattered;

/// The group scattered things go under, named Scatter.
#[derive(Component, Reflect, Clone, Copy, Debug, Default)]
#[type_path = "besfa_editor_plugin::ground"]
#[reflect(Component, Default)]
pub struct ScatterGroup;
