//! The components a Besfa game ships with, and the plugins that make them
//! work: the scene file, changes to placed models' parts, terrain and the
//! foliage on it, the sky and its lens flare, animation state machines,
//! inventories and equipment, health, world saves, and the editor's markers for what
//! it cannot show otherwise.
//!
//! Games use [`default_plugins`] in place of `DefaultPlugins`, or
//! `besfa_editor_plugin::default_plugins` while the Besfa editor runs them,
//! which adds the editor's side on top of these.
//!
//! Each reflected type keeps the path it had in `besfa_editor_plugin`
//! (`besfa_editor_plugin::sky::Sky`), which scene files name it by; newer
//! ones are named the same way (`besfa_editor_plugin::foliage::Foliage`).

use bevy::{app::PluginGroupBuilder, prelude::*};

pub mod animation;
pub mod foliage;
pub mod ground;
pub mod health;
pub mod inventory;
pub mod lens_flare;
pub mod marker;
pub mod part_changes;
#[doc(hidden)]
pub mod reflect;
pub mod save;
pub mod scene;
pub mod sky;
pub mod terrain;

pub use animation::{
    AnimationClips, AnimationCondition, AnimationParams, AnimationState, AnimationStateMachine,
    AnimationStateMachinePlugin, AnimationTransition, ConditionTest,
};
pub use foliage::{Foliage, FoliagePlugin};
pub use ground::{ScatterGroup, Scattered};
pub use health::{Damage, Died, Health, HealthPlugin};
pub use inventory::{EquipSlot, Equipment, Inventory, ItemStack};
pub use marker::{Marker, MarkerIcon};
pub use part_changes::{PartChange, PartChanges};
pub use save::{Persist, SaveFilter, SceneId, backup_path, load_world, save_world};
pub use scene::{MeshColor, MeshShape, RuntimeOnly, SCENE_PATH, SceneEntity};
pub use sky::{Moon, Sky, SkyMode, Sun};
pub use terrain::{Terrain, TerrainData, TerrainFile, TerrainHandle, TerrainHeight};

/// `DefaultPlugins` that load the scene from `assets/scenes/main.scn.ron`
/// and run the components in it.
pub fn default_plugins() -> PluginGroupBuilder {
    DefaultPlugins
        .build()
        .add(scene::ScenePlugin)
        // Saved changes to placed models' parts apply in play as well.
        .add(part_changes::PartChangesPlugin)
        .add(terrain::TerrainPlugin)
        .add(foliage::FoliagePlugin)
        .add(sky::SkyPlugin)
        .add(lens_flare::LensFlarePlugin)
        .add(animation::AnimationStateMachinePlugin)
        .add(health::HealthPlugin)
}

/// Kept out of the editor's Hierarchy and the scene file.
#[derive(Component)]
pub struct EditorOnly;

/// A placed or previewed model's root, once the model file it shows was
/// loaded again and the root spawned it anew. Games that change what a
/// model spawns, like swapping its materials, do it again here; Bevy's
/// `WorldInstanceReady` also comes for it.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct Reimported {
    pub entity: Entity,
}

#[cfg(test)]
mod tests {
    use bevy::reflect::TypePath;

    use super::*;

    #[test]
    fn keeps_the_type_paths_scene_files_name_types_by() {
        for (path, name) in [
            (Sky::type_path(), "sky::Sky"),
            (Sun::type_path(), "sky::Sun"),
            (Terrain::type_path(), "terrain::Terrain"),
            (Foliage::type_path(), "foliage::Foliage"),
            (TerrainFile::type_path(), "terrain::TerrainFile"),
            (MeshShape::type_path(), "scene::MeshShape"),
            (MeshColor::type_path(), "scene::MeshColor"),
            (PartChanges::type_path(), "part_changes::PartChanges"),
            (Scattered::type_path(), "ground::Scattered"),
            (ScatterGroup::type_path(), "ground::ScatterGroup"),
            (Inventory::type_path(), "inventory::Inventory"),
            (Health::type_path(), "health::Health"),
            (Marker::type_path(), "marker::Marker"),
            (Persist::type_path(), "save::Persist"),
            (ItemStack::type_path(), "inventory::ItemStack"),
            (Equipment::type_path(), "inventory::Equipment"),
            (EquipSlot::type_path(), "inventory::EquipSlot"),
            (
                AnimationStateMachine::type_path(),
                "animation::AnimationStateMachine",
            ),
            (AnimationParams::type_path(), "animation::AnimationParams"),
        ] {
            assert_eq!(path, format!("besfa_editor_plugin::{name}"));
        }
    }
}
