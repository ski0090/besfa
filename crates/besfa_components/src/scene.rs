//! Loads the game's scene from a file, and another when the game asks.
//!
//! A scene is a file such as `assets/scenes/main.scn.ron` in Bevy's RON
//! world format: the entities and their reflected components. Meshes and
//! materials are handles, which cannot go in a file, so the file holds a
//! [`MeshShape`] and a [`MeshColor`] instead and the plugin builds the
//! assets when it loads.
//!
//! The game starts in [`first_scene`]. It goes to another scene by writing
//! [`SwitchScene`]; once that file has loaded, the old scene's entities go,
//! the new ones spawn, [`CurrentScene`] names it and [`SceneSpawned`] comes,
//! all in one frame. Entities the game spawned stay, and so do scene
//! entities it marked [`KeepOnSceneSwitch`].

use std::{any::TypeId, collections::HashSet, path::PathBuf};

use bevy::{
    asset::{LoadState, io::file::FileAssetReader},
    camera::visibility::VisibilitySystems,
    ecs::{entity::EntityHashMap, entity_disabling::Disabled, name::NameOrEntity, query::Allow},
    prelude::*,
    transform::TransformSystems,
    world_serialization::{DynamicWorld, WorldInstanceSpawnError},
};

/// The scene file a game starts in unless told otherwise, relative to the
/// game's `assets` directory.
pub const SCENE_PATH: &str = "scenes/main.scn.ron";

/// The scene file the game starts in, relative to `assets`: the one
/// `BESFA_SCENE` names, as the editor does with the scene it has open, or
/// [`SCENE_PATH`].
pub fn first_scene() -> String {
    std::env::var("BESFA_SCENE").unwrap_or_else(|_| SCENE_PATH.into())
}

/// Asks the game to go to the scene file at this path, relative to
/// `assets`, as `scenes/tutorial.scn.ron`. The scene the game is in stays
/// until that file has loaded; asked again before then, the last one wins.
/// A file that does not load is logged and the game stays where it is.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct SwitchScene(pub String);

/// The scene file at this path has spawned: its entities are in the world,
/// and the last scene's are gone. It comes for the first scene too. Their
/// meshes, and children in their parents' `Children`, come in `PostUpdate`
/// of the same frame.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct SceneSpawned(pub String);

/// The scene file whose entities are in the world, relative to `assets`.
/// It changes when a scene spawns, not when the game asks for one, and is
/// not there until the first scene has spawned.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct CurrentScene(pub String);

/// Taken along to the next scene with what is under it, when it came from
/// the scene file: it loses [`SceneEntity`] and [`FileEntity`], as the
/// next scene numbers its own entities, and leaves a parent that goes,
/// keeping its place in the world. Entities the game spawned stay without
/// it, unless they are under a scene entity that goes.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct KeepOnSceneSwitch;

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
        app.add_message::<SwitchScene>()
            .add_message::<SceneSpawned>()
            .add_systems(Startup, load)
            .add_systems(PreUpdate, (start_switch, spawn_loaded).chain())
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

/// Startup runs again when the editor's Stop starts the game over, which
/// empties the world, so no scene is current until the first one spawns.
fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.remove_resource::<CurrentScene>();
    commands.insert_resource(Loading(assets.load(first_scene())));
}

/// Loads the scene the game asked for last, in place of any loading.
#[doc(hidden)]
pub fn start_switch(
    mut switches: MessageReader<SwitchScene>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    if let Some(SwitchScene(path)) = switches.read().last() {
        commands.insert_resource(Loading(assets.load(path.clone())));
    }
}

/// Spawns the scene once its file has loaded, in place of the last one.
/// Bevy's spawner would keep the entities' ids in the file to itself. A
/// load failure is only logged and leaves the last scene, so a game that
/// builds its scene in code keeps working.
fn spawn_loaded(world: &mut World) {
    let Some(handle) = world
        .get_resource::<Loading>()
        .map(|loading| loading.0.clone())
    else {
        return;
    };
    let path = handle.path().map(ToString::to_string).unwrap_or_default();
    if let LoadState::Failed(error) = world.resource::<AssetServer>().load_state(&handle) {
        error!("Could not load {path}: {error}");
        world.remove_resource::<Loading>();
        return;
    }
    let last: Vec<Entity> = world
        .query_filtered::<Entity, (With<SceneEntity>, Allow<Disabled>)>()
        .iter(world)
        .collect();
    let spawned = world.resource_scope(|world, scenes: Mut<Assets<DynamicWorld>>| {
        scenes.get(&handle).map(|scene| spawn_scene(world, scene))
    });
    match spawned {
        None => return,
        Some(Ok(())) => {
            leave(world, &last);
            info!("Loaded {path}");
            world.insert_resource(CurrentScene(path.clone()));
            world.write_message(SceneSpawned(path));
        }
        Some(Err(error)) => error!("Could not load {path}: {error}"),
    }
    world.remove_resource::<Loading>();
}

/// Despawns the `last` scene's entities but those [`KeepOnSceneSwitch`]
/// keeps, with what is under them, which become the game's own.
fn leave(world: &mut World, last: &[Entity]) {
    let kept: Vec<Entity> = world
        .query_filtered::<Entity, (With<KeepOnSceneSwitch>, Allow<Disabled>)>()
        .iter(world)
        .collect();
    let mut stays = HashSet::new();
    let mut under = kept.clone();
    while let Some(entity) = under.pop() {
        if stays.insert(entity)
            && let Some(children) = world.get::<Children>(entity)
        {
            under.extend(children.iter());
        }
    }
    let goes: HashSet<Entity> = last
        .iter()
        .copied()
        .filter(|entity| !stays.contains(entity))
        .collect();
    for &entity in &kept {
        let mut above = world.get::<ChildOf>(entity).map(ChildOf::parent);
        while let Some(parent) =
            above.filter(|parent| !goes.contains(parent) && !stays.contains(parent))
        {
            above = world.get::<ChildOf>(parent).map(ChildOf::parent);
        }
        if above.is_some_and(|parent| goes.contains(&parent)) {
            let place = world.get::<GlobalTransform>(entity).copied();
            let mut entity = world.entity_mut(entity);
            entity.remove::<ChildOf>();
            if let Some(place) = place {
                entity.insert(place.compute_transform());
            }
        }
    }
    for entity in stays {
        world
            .entity_mut(entity)
            .remove::<(SceneEntity, FileEntity)>();
    }
    for entity in goes {
        // Gone already when its parent went.
        let _ = world.try_despawn(entity);
    }
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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::Path,
        time::{Duration, Instant},
    };

    use bevy::{ecs::message::Messages, world_serialization::WorldSerializationPlugin};

    use super::*;

    /// A camp at x 10 with a hero 1 m into it, holding a sword.
    const MAIN: &str = r#"(
  resources: {},
  entities: {
    1: (components: {
      "bevy_ecs::name::Name": "Camp",
      "bevy_transform::components::transform::Transform": (translation: (10.0, 0.0, 0.0), rotation: (0.0, 0.0, 0.0, 1.0), scale: (1.0, 1.0, 1.0)),
    }),
    2: (components: {
      "bevy_ecs::hierarchy::ChildOf": (1),
      "bevy_ecs::name::Name": "Hero",
      "bevy_transform::components::transform::Transform": (translation: (1.0, 0.0, 0.0), rotation: (0.0, 0.0, 0.0, 1.0), scale: (1.0, 1.0, 1.0)),
    }),
    3: (components: {
      "bevy_ecs::hierarchy::ChildOf": (2),
      "bevy_ecs::name::Name": "Sword",
    }),
  },
)"#;

    /// Numbered from 1 as well: ids are the file's own.
    const TUTORIAL: &str = r#"(
  resources: {},
  entities: {
    1: (components: {
      "bevy_ecs::name::Name": "Fence",
    }),
  },
)"#;

    /// An `assets` folder holding `scenes`, each a name and its file.
    fn assets(test: &str, scenes: &[(&str, &str)]) -> PathBuf {
        let assets = std::env::temp_dir().join(format!("besfa-{test}-{}", std::process::id()));
        fs::create_dir_all(assets.join("scenes")).unwrap();
        for (name, ron) in scenes {
            fs::write(assets.join(format!("scenes/{name}.scn.ron")), ron).unwrap();
        }
        assets
    }

    /// A game that loads its scenes from `assets`.
    fn app(assets: &Path) -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin {
                file_path: assets.to_string_lossy().into(),
                ..default()
            },
            TransformPlugin,
            WorldSerializationPlugin,
            ScenePlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .register_type::<Name>()
        .register_type::<Transform>()
        .register_type::<ChildOf>();
        app
    }

    /// Updates until the scene asked for has spawned or failed to load,
    /// and returns the scenes that spawned meanwhile.
    fn settle(app: &mut App) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut spawned = Vec::new();
        loop {
            app.update();
            let mut messages = app.world_mut().resource_mut::<Messages<SceneSpawned>>();
            spawned.extend(messages.drain().map(|SceneSpawned(path)| path));
            if !app.world().contains_resource::<Loading>() {
                return spawned;
            }
            assert!(Instant::now() < deadline, "the scene should load");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn named(app: &mut App, name: &str) -> Vec<Entity> {
        let world = app.world_mut();
        world
            .query::<(Entity, &Name)>()
            .iter(world)
            .filter(|(_, named)| named.as_str() == name)
            .map(|(entity, _)| entity)
            .collect()
    }

    fn current(app: &App) -> Option<&str> {
        let current = app.world().get_resource::<CurrentScene>();
        current.map(|current| current.0.as_str())
    }

    fn switch(app: &mut App, path: &str) {
        app.world_mut().write_message(SwitchScene(path.into()));
    }

    #[test]
    fn switches_scenes_keeping_what_the_game_keeps() {
        let assets = assets("switch", &[("main", MAIN), ("tutorial", TUTORIAL)]);
        let mut app = app(&assets);
        let player = app.world_mut().spawn(Name::new("Player")).id();
        assert_eq!(settle(&mut app), ["scenes/main.scn.ron"]);
        assert_eq!(current(&app), Some("scenes/main.scn.ron"));
        let (hero, sword) = (named(&mut app, "Hero")[0], named(&mut app, "Sword")[0]);
        app.world_mut().entity_mut(hero).insert(KeepOnSceneSwitch);

        switch(&mut app, "scenes/tutorial.scn.ron");
        assert_eq!(settle(&mut app), ["scenes/tutorial.scn.ron"]);
        assert_eq!(current(&app), Some("scenes/tutorial.scn.ron"));
        assert!(named(&mut app, "Camp").is_empty(), "the last scene goes");
        assert_eq!(named(&mut app, "Fence").len(), 1);
        let world = app.world();
        assert!(world.get_entity(player).is_ok(), "the game's own stays");
        assert!(world.get::<ChildOf>(hero).is_none(), "out of the camp");
        assert_eq!(
            world.get::<Transform>(hero).map(|at| at.translation),
            Some(Vec3::new(11.0, 0.0, 0.0)),
            "where it was in the world"
        );
        assert_eq!(world.get::<ChildOf>(sword).map(ChildOf::parent), Some(hero));
        for kept in [hero, sword] {
            assert!(world.get::<SceneEntity>(kept).is_none());
            assert!(world.get::<FileEntity>(kept).is_none(), "the fence's id");
        }

        // A file that does not load leaves the game where it is.
        switch(&mut app, "scenes/missing.scn.ron");
        assert!(settle(&mut app).is_empty());
        assert_eq!(current(&app), Some("scenes/tutorial.scn.ron"));
        assert_eq!(named(&mut app, "Fence").len(), 1);

        switch(&mut app, "scenes/main.scn.ron");
        assert_eq!(settle(&mut app), ["scenes/main.scn.ron"]);
        assert!(named(&mut app, "Fence").is_empty());
        let heroes = named(&mut app, "Hero");
        assert_eq!(heroes.len(), 2, "the kept one and the file's");
        fs::remove_dir_all(assets).unwrap();
    }

    #[test]
    fn a_switch_asked_at_startup_starts_there() {
        let assets = assets("startup", &[("main", MAIN), ("tutorial", TUTORIAL)]);
        let mut app = app(&assets);
        app.add_systems(Startup, |mut switches: MessageWriter<SwitchScene>| {
            switches.write(SwitchScene("scenes/tutorial.scn.ron".into()));
        });
        assert_eq!(settle(&mut app), ["scenes/tutorial.scn.ron"]);
        assert!(named(&mut app, "Camp").is_empty(), "main never spawns");
        fs::remove_dir_all(assets).unwrap();
    }
}
