//! Loads the game's scene from a file.
//!
//! The scene is `assets/scenes/main.scn.ron` in Bevy's RON world format: the
//! entities and their reflected components. Meshes and materials are
//! handles, which cannot go in a file, so the file holds a [`MeshShape`] and
//! a [`MeshColor`] instead and the plugin builds the assets when it loads.

use std::{any::TypeId, collections::HashSet, path::PathBuf};

use bevy::{
    asset::{LoadState, io::file::FileAssetReader},
    camera::visibility::VisibilitySystems,
    ecs::{entity::EntityHashMap, entity_disabling::Disabled, name::NameOrEntity, query::Allow},
    prelude::*,
    transform::TransformSystems,
    world_serialization::{DynamicWorld, WorldInstanceSpawnError},
};

/// The scene file, relative to the game's `assets` directory.
pub const SCENE_PATH: &str = "scenes/main.scn.ron";

/// Components a running game puts on scene entities by itself, as a fog it
/// sets on its camera from the time of day: a scene saved from the editor
/// leaves them out, since the game puts them back.
#[derive(Resource, Clone, Debug, Default)]
pub struct RuntimeOnly(HashSet<TypeId>);

impl RuntimeOnly {
    /// Leaves `T` out of saved scenes.
    pub fn add<T: Component>(&mut self) -> &mut Self {
        self.0.insert(TypeId::of::<T>());
        self
    }

    /// Whether components of type `id` are left out.
    pub fn contains(&self, id: TypeId) -> bool {
        self.0.contains(&id)
    }
}

/// Marks the entities that came from the scene file; saving writes them back.
#[derive(Component)]
pub struct SceneEntity;

/// The id an entity has in the scene file, which saving writes it back
/// under, so a save changes only the lines of what changed. A copy does not
/// take it: it is new to the file.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[component(clone_behavior = Ignore)]
pub struct FileEntity(pub Entity);

/// A mesh the plugin builds from a primitive shape, and rebuilds when the
/// shape changes.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::scene"]
#[reflect(Component, Default)]
pub enum MeshShape {
    Cuboid { size: Vec3 },
    Sphere { radius: f32 },
    Plane { size: Vec2 },
}

impl Default for MeshShape {
    fn default() -> Self {
        Self::Cuboid { size: Vec3::ONE }
    }
}

impl MeshShape {
    fn mesh(&self) -> Mesh {
        match *self {
            Self::Cuboid { size } => Cuboid::from_size(size).into(),
            Self::Sphere { radius } => Sphere::new(radius).into(),
            Self::Plane { size } => Plane3d::default().mesh().size(size.x, size.y).into(),
        }
    }
}

/// A standard material of one color the plugin builds, and rebuilds when
/// the color changes.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::scene"]
#[reflect(Component, Default)]
pub struct MeshColor(pub Color);

impl Default for MeshColor {
    fn default() -> Self {
        Self(Color::srgb(0.8, 0.8, 0.8))
    }
}

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load)
            .add_systems(PreUpdate, spawn_loaded)
            // After SpawnScene, so a loaded scene gets its meshes this frame.
            .add_systems(PostUpdate, (build_meshes, build_materials))
            .add_systems(
                PostUpdate,
                adopt_loaded_children
                    .before(TransformSystems::Propagate)
                    .before(VisibilitySystems::VisibilityPropagate),
            )
            .add_observer(drop_mesh)
            .add_observer(drop_material);
    }
}

/// The scene file until it has spawned.
#[derive(Resource)]
#[doc(hidden)]
pub struct Loading(pub Handle<DynamicWorld>);

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Loading(assets.load(SCENE_PATH)));
}

/// Spawns the scene once its file has loaded. Bevy's spawner would keep
/// the entities' ids in the file to itself. A load failure is only logged,
/// so a game that builds its scene in code keeps working.
fn spawn_loaded(world: &mut World) {
    let Some(handle) = world
        .get_resource::<Loading>()
        .map(|loading| loading.0.clone())
    else {
        return;
    };
    if let LoadState::Failed(error) = world.resource::<AssetServer>().load_state(&handle) {
        error!("Could not load {SCENE_PATH}: {error}");
        world.remove_resource::<Loading>();
        return;
    }
    let spawned = world.resource_scope(|world, scenes: Mut<Assets<DynamicWorld>>| {
        scenes.get(&handle).map(|scene| spawn_scene(world, scene))
    });
    match spawned {
        None => return,
        Some(Ok(())) => info!("Loaded {SCENE_PATH}"),
        Some(Err(error)) => error!("Could not load {SCENE_PATH}: {error}"),
    }
    world.remove_resource::<Loading>();
}

/// Spawns `scene` into `world` and tags its entities with their ids in it.
#[doc(hidden)]
pub fn spawn_scene(world: &mut World, scene: &DynamicWorld) -> Result<(), WorldInstanceSpawnError> {
    let mut map = EntityHashMap::default();
    scene.write_to_world(world, &mut map)?;
    for (file, entity) in map {
        // A parent the file names but does not hold leaves a dead id here.
        if let Ok(mut entity) = world.get_entity_mut(entity) {
            entity.insert((SceneEntity, FileEntity(file)));
        }
    }
    Ok(())
}

/// Puts children loaded from a scene or prefab file into their parents'
/// `Children`. Bevy writes files into the world without the hooks that keep
/// the two sides of a relationship together, and the files hold only
/// `ChildOf`; without this, transforms and visibility never reach the
/// children, and despawning a parent leaves them behind. A child whose
/// parent is not there goes to the top, and a parent that does not take
/// part in visibility, like a group made empty, gets a `Visibility`.
#[allow(
    clippy::type_complexity,
    reason = "a system's query: children loaded this frame"
)]
#[doc(hidden)]
pub fn adopt_loaded_children(
    mut commands: Commands,
    children: Query<(NameOrEntity, &ChildOf, Has<InheritedVisibility>), Added<ChildOf>>,
    parents: Query<(Option<&Children>, Has<InheritedVisibility>), Allow<Disabled>>,
) {
    for (name, child_of, visible) in &children {
        let (child, parent) = (name.entity, child_of.parent());
        let Ok((held, parent_visible)) = parents.get(parent) else {
            warn!("{name}'s parent {parent} is not there; it goes to the top.");
            commands.entity(child).try_remove::<ChildOf>();
            continue;
        };
        if !held.is_some_and(|held| held.contains(&child)) {
            // Inserted again, with the hooks this time.
            commands.entity(child).try_insert(ChildOf(parent));
        }
        if visible && !parent_visible {
            commands.entity(parent).try_insert(Visibility::default());
        }
    }
}

/// Gives each new or changed shape a new mesh. Replacing `Mesh3d` drops the
/// old handle, which frees the old mesh, and a duplicated entity never
/// shares its mesh with the original.
#[doc(hidden)]
pub fn build_meshes(
    shapes: Query<(Entity, &MeshShape), Changed<MeshShape>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    for (entity, shape) in &shapes {
        commands
            .entity(entity)
            .insert(Mesh3d(meshes.add(shape.mesh())));
    }
}

#[doc(hidden)]
pub fn build_materials(
    colors: Query<(Entity, &MeshColor), Changed<MeshColor>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (entity, color) in &colors {
        commands
            .entity(entity)
            .insert(MeshMaterial3d(materials.add(color.0)));
    }
}

/// Removing the shape removes the mesh built from it.
#[doc(hidden)]
pub fn drop_mesh(remove: On<Remove<MeshShape>>, mut commands: Commands) {
    commands.entity(remove.entity).try_remove::<Mesh3d>();
}

#[doc(hidden)]
pub fn drop_material(remove: On<Remove<MeshColor>>, mut commands: Commands) {
    commands
        .entity(remove.entity)
        .try_remove::<MeshMaterial3d<StandardMaterial>>();
}

/// Where the asset at `path`, relative to the asset directory, is on disk.
pub fn asset_file(path: &str) -> PathBuf {
    project_file("assets").join(path)
}

/// Where `path`, relative to the project, is on disk: the folder the asset
/// directory is in.
pub fn project_file(path: &str) -> PathBuf {
    FileAssetReader::get_base_path().join(path)
}
