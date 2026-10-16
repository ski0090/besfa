//! Loads the game's scene from a file and saves it back.
//!
//! The scene is `assets/scenes/main.scn.ron` in Bevy's RON world format: the
//! entities and their reflected components. Meshes and materials are
//! handles, which cannot go in a file, so the file holds a [`MeshShape`] and
//! a [`MeshColor`] instead and the plugin builds the assets when it loads.

use std::{
    any::TypeId,
    collections::HashSet,
    path::{Path, PathBuf},
};

use bevy::{
    asset::{
        AssetLoadFailedEvent, EphemeralHandleBehavior, HandleSerializeProcessor,
        io::file::FileAssetReader,
    },
    camera::{
        CameraMainTextureUsages, Exposure, RenderTarget,
        primitives::{Aabb, CascadesFrusta, CubemapFrusta, Frustum, Sphere as BoundingSphere},
        visibility::{
            CascadesVisibleEntities, CubemapVisibleEntities, VisibilityClass, VisibilitySystems,
            VisibleEntities,
        },
    },
    ecs::{
        component::{ComponentId, ComponentInfo},
        entity_disabling::Disabled,
        query::Allow,
    },
    light::Cascades,
    prelude::*,
    reflect::{PartialReflect, TypeRegistry, serde::TypedReflectSerializer},
    render::{
        camera::CameraRenderGraph,
        sync_world::{RenderEntity, SyncToRenderWorld},
        view::ColorGrading,
    },
    transform::{TransformSystems, components::TransformTreeChanged},
    world_serialization::{
        DynamicEntity, DynamicWorld, DynamicWorldRoot, InstanceId, WorldAssetRoot,
        WorldInstanceSpawner,
        serde::{
            ENTITY_FIELD_COMPONENTS, ENTITY_STRUCT, WORLD_ENTITIES, WORLD_RESOURCES, WORLD_STRUCT,
            WorldMapSerializer,
        },
        serialize_ron,
    },
};
use serde::ser::{Serialize, SerializeStruct, Serializer};

/// The scene file, relative to the game's `assets` directory.
pub const SCENE_PATH: &str = "scenes/main.scn.ron";

/// Marks the entities that came from the scene file; saving writes them back.
#[derive(Component)]
pub struct SceneEntity;

/// A mesh the plugin builds from a primitive shape, and rebuilds when the
/// shape changes.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
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
#[reflect(Component, Default)]
pub struct MeshColor(pub Color);

impl Default for MeshColor {
    fn default() -> Self {
        Self(Color::srgb(0.8, 0.8, 0.8))
    }
}

pub(crate) struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load)
            .add_systems(PreUpdate, mark_loaded)
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

/// The scene instance until it has spawned.
#[derive(Resource)]
pub(crate) struct Loading(InstanceId);

fn load(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut spawner: ResMut<WorldInstanceSpawner>,
) {
    let instance = spawner.spawn_dynamic(assets.load::<DynamicWorld>(SCENE_PATH));
    commands.insert_resource(Loading(instance));
}

/// Tags the scene's entities once they exist. A load failure is only
/// logged, so a game that builds its scene in code keeps working.
fn mark_loaded(
    mut commands: Commands,
    loading: Option<Res<Loading>>,
    spawner: Res<WorldInstanceSpawner>,
    mut failures: MessageReader<AssetLoadFailedEvent<DynamicWorld>>,
) {
    let Some(loading) = loading else {
        return;
    };
    if let Some(failure) = failures.read().next() {
        error!("Could not load {SCENE_PATH}: {}", failure.error);
        commands.remove_resource::<Loading>();
        return;
    }
    if !spawner.instance_is_ready(loading.0) {
        return;
    }
    for entity in spawner.iter_instance_entities(loading.0) {
        // A parent the file names but does not hold leaves a dead id here.
        commands.entity(entity).try_insert(SceneEntity);
    }
    commands.remove_resource::<Loading>();
    info!("Loaded {SCENE_PATH}");
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
fn adopt_loaded_children(
    mut commands: Commands,
    children: Query<(Entity, &ChildOf, Has<InheritedVisibility>, Option<&Name>), Added<ChildOf>>,
    parents: Query<(Option<&Children>, Has<InheritedVisibility>), Allow<Disabled>>,
) {
    for (child, child_of, visible, name) in &children {
        let parent = child_of.parent();
        let Ok((held, parent_visible)) = parents.get(parent) else {
            let name = name.map_or_else(|| child.to_string(), ToString::to_string);
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
fn build_meshes(
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

fn build_materials(
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
fn drop_mesh(remove: On<Remove<MeshShape>>, mut commands: Commands) {
    if let Ok(mut entity) = commands.get_entity(remove.entity) {
        entity.try_remove::<Mesh3d>();
    }
}

fn drop_material(remove: On<Remove<MeshColor>>, mut commands: Commands) {
    if let Ok(mut entity) = commands.get_entity(remove.entity) {
        entity.try_remove::<MeshMaterial3d<StandardMaterial>>();
    }
}

/// Where the scene file is on disk: the asset directory the game loads from.
pub(crate) fn scene_file() -> PathBuf {
    asset_file(SCENE_PATH)
}

/// Where the asset at `path`, relative to the asset directory, is on disk.
pub(crate) fn asset_file(path: &str) -> PathBuf {
    project_file("assets").join(path)
}

/// Where `path`, relative to the project, is on disk: the folder the asset
/// directory is in.
pub(crate) fn project_file(path: &str) -> PathBuf {
    FileAssetReader::get_base_path().join(path)
}

/// Writes the scene entities to `path` in the scene file format.
pub(crate) fn save(world: &mut World, path: &Path) -> Result<(), String> {
    let mut entities: Vec<Entity> = world
        .query_filtered::<Entity, With<SceneEntity>>()
        .iter(world)
        .collect();
    // Spawn order, so the file does not shuffle between saves.
    entities.sort_unstable_by_key(|entity| (entity.index_u32(), entity.generation().to_bits()));
    write(world, &entities, &[], false, path)
}

/// Writes `root` and the scene entities under it to `path` as a prefab.
/// The root is written at the origin and without its parent, so where the
/// prefab is placed decides where it shows.
pub(crate) fn save_prefab(world: &mut World, root: Entity, path: &Path) -> Result<(), String> {
    let mut entities = vec![root];
    let mut next = 0;
    while let Some(&entity) = entities.get(next) {
        next += 1;
        if let Some(children) = world.get::<Children>(entity) {
            entities.extend(children.iter().filter(|&child| {
                world.get::<SceneEntity>(child).is_some()
                    && world.get::<crate::edit::Deleted>(child).is_none()
            }));
        }
    }
    write(world, &entities, &[root], true, path)
}

/// The prefab file the instance `root` shows, as an asset path.
pub(crate) fn instance_prefab(world: &World, root: Entity) -> Result<String, String> {
    world
        .get::<DynamicWorldRoot>(root)
        .and_then(|instance| instance.0.path())
        .map(|path| path.without_label().to_string())
        .ok_or_else(|| format!("entity {root} is not a placed scene or prefab"))
}

/// Writes what the instance `root` shows to `path`: the entities the
/// prefab spawned under it, as they are now, so changes made inside the
/// instance reach the prefab. Instances inside it are written as their
/// paths, as the scene file does.
pub(crate) fn save_instance(world: &mut World, root: Entity, path: &Path) -> Result<(), String> {
    // Writing the scene placed in itself would replace the scene with it,
    // and reloading would respawn the whole scene from that.
    if path == scene_file() {
        return Err(format!("{SCENE_PATH} is saved with Save scene"));
    }
    // Copies the editor made inside the instance are the scene's own.
    let content = |world: &World, entity: Entity| {
        let children = world.get::<Children>(entity).into_iter().flatten();
        children
            .copied()
            .filter(|&child| {
                world.get::<SceneEntity>(child).is_none()
                    && world.get::<crate::edit::Deleted>(child).is_none()
            })
            .collect::<Vec<_>>()
    };
    let tops = content(world, root);
    let mut entities = tops.clone();
    let mut next = 0;
    while let Some(&entity) = entities.get(next) {
        next += 1;
        if world.get::<DynamicWorldRoot>(entity).is_none()
            && world.get::<WorldAssetRoot>(entity).is_none()
        {
            entities.extend(content(world, entity));
        }
    }
    write(world, &entities, &tops, false, path)
}

/// Writes `entities` to `path`. `roots` lose their parent, and with
/// `at_origin` their place too.
fn write(
    world: &World,
    entities: &[Entity],
    roots: &[Entity],
    at_origin: bool,
    path: &Path,
) -> Result<(), String> {
    // A file that places itself would load forever.
    if let Some(placed) = entities
        .iter()
        .filter_map(|&entity| instance_prefab(world, entity).ok())
        .find(|placed| asset_file(placed) == path)
    {
        return Err(format!("{placed} cannot be placed inside itself"));
    }
    let registry = world.resource::<AppTypeRegistry>().read();
    let mut scene = dynamic_world(world, entities, &registry);
    let written: HashSet<Entity> = entities.iter().copied().collect();
    for saved in &mut scene.entities {
        let root = roots.contains(&saved.entity);
        // A parent the file does not hold would load as a dead entity.
        let orphan = world
            .get::<ChildOf>(saved.entity)
            .is_some_and(|child_of| !written.contains(&child_of.parent()));
        if orphan && !root {
            warn!(
                "Saving {} at the top: its parent is not saved.",
                saved.entity
            );
        }
        if root || orphan {
            saved
                .components
                .retain(|component| !is::<ChildOf>(component.as_ref()));
        }
        for component in &mut saved.components {
            if root && at_origin && is::<Transform>(component.as_ref()) {
                *component = Box::new(Transform::default());
            }
        }
    }
    let ron = serialize_ron(InOrder {
        world: &scene,
        registry: &registry,
    })
    .map_err(|error| error.to_string())?;
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, ron).map_err(|error| error.to_string())
}

/// Bevy's world format with each entity's components in the order given,
/// which is the order loading inserts them in. Bevy's own serializer sorts
/// them by type path.
struct InOrder<'a> {
    world: &'a DynamicWorld,
    registry: &'a TypeRegistry,
}

impl Serialize for InOrder<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut world = serializer.serialize_struct(WORLD_STRUCT, 2)?;
        world.serialize_field(
            WORLD_RESOURCES,
            &WorldMapSerializer {
                entries: &self.world.resources,
                registry: self.registry,
            },
        )?;
        world.serialize_field(WORLD_ENTITIES, &Entities(self))?;
        world.end()
    }
}

struct Entities<'a>(&'a InOrder<'a>);

impl Serialize for Entities<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(
            self.0
                .world
                .entities
                .iter()
                .map(|entity| (entity.entity, Components(entity, self.0.registry))),
        )
    }
}

struct Components<'a>(&'a DynamicEntity, &'a TypeRegistry);

impl Serialize for Components<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut entity = serializer.serialize_struct(ENTITY_STRUCT, 1)?;
        entity.serialize_field(ENTITY_FIELD_COMPONENTS, &ComponentMap(self))?;
        entity.end()
    }
}

struct ComponentMap<'a>(&'a Components<'a>);

impl Serialize for ComponentMap<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Components(entity, registry) = self.0;
        serializer.collect_map(entity.components.iter().map(|component| {
            let path = component
                .get_represented_type_info()
                .map_or_else(|| component.reflect_type_path(), |info| info.type_path());
            (
                path,
                TypedReflectSerializer::with_processor(
                    component.as_partial_reflect(),
                    registry,
                    &HANDLE_PATHS,
                ),
            )
        }))
    }
}

fn is<T: 'static>(component: &dyn PartialReflect) -> bool {
    component
        .get_represented_type_info()
        .is_some_and(|info| info.type_id() == TypeId::of::<T>())
}

/// Serializes handles to loaded assets as their paths, the way scene files
/// store them; a handle to an asset made in code has no path and fails.
pub(crate) const HANDLE_PATHS: HandleSerializeProcessor = HandleSerializeProcessor {
    ephemeral_handle_behavior: EphemeralHandleBehavior::Error,
};

fn dynamic_world(world: &World, entities: &[Entity], registry: &TypeRegistry) -> DynamicWorld {
    let entities = entities
        .iter()
        .map(|&entity| DynamicEntity {
            entity,
            components: saved_components(world, entity, registry),
        })
        .collect();
    DynamicWorld {
        resources: Vec::new(),
        entities,
    }
}

/// The entity's components that belong in the file: reflected, not built
/// at runtime from other components, and serializable. Handles and other
/// opaque values are left out with a warning. So is a component that
/// another saved one requires and that still has its default value:
/// loading brings it back with its requirer.
///
/// Requirers come first, because loading inserts components in file order
/// and a component can look for what its requirer brings: `Camera` warns
/// about a missing render graph unless `Camera3d` is already there.
// ponytail: compares with `Default`, so a value set to the default where
// the requirer brings another one (Camera3d's `DebandDither::Enabled`) is
// dropped and loads as the requirer's. Build the requirer in a scratch
// world to compare if that bites.
fn saved_components(
    world: &World,
    entity: Entity,
    registry: &TypeRegistry,
) -> Vec<Box<dyn PartialReflect>> {
    let entity_ref = world.entity(entity);
    let parked = world
        .get::<crate::scene_view::Parked>(entity)
        .map(|parked| parked.active);
    let saved: Vec<(ComponentId, &ComponentInfo, Box<dyn PartialReflect>)> = world
        .inspect_entity(entity)
        .into_iter()
        .flatten()
        .filter_map(|(id, info)| {
            let type_id = info.type_id().filter(|id| !left_out(*id))?;
            let value = registry
                .get_type_data::<ReflectComponent>(type_id)?
                .reflect(entity_ref)?;
            let serializer = TypedReflectSerializer::with_processor(
                value.as_partial_reflect(),
                registry,
                &HANDLE_PATHS,
            );
            let copy = saved_value(value, parked);
            if serde_json::to_value(serializer).is_err() || copy.is_none() {
                warn!("Not saving {}: it does not serialize.", info.name());
                return None;
            }
            Some((id, info, copy?))
        })
        .collect();
    let requirers = |id: ComponentId| {
        saved
            .iter()
            .filter(|(_, other, _)| other.required_components().iter_ids().any(|r| r == id))
            .count()
    };
    let requirers: Vec<usize> = saved.iter().map(|(id, ..)| requirers(*id)).collect();
    let mut kept: Vec<(usize, Box<dyn PartialReflect>)> = saved
        .into_iter()
        .zip(requirers)
        .filter(|((_, info, value), requirers)| {
            *requirers == 0 || !is_default(registry, info, value.as_ref())
        })
        .map(|((.., value), requirers)| (requirers, value))
        .collect();
    // Stable, and `required_components` holds requirements of requirements,
    // so whatever requires a component has fewer requirers than it.
    kept.sort_by_key(|(requirers, _)| *requirers);
    kept.into_iter().map(|(_, value)| value).collect()
}

/// A copy of `value` for the file. A concrete clone keeps types that
/// serialize through serde, like `Name`, serializable; a dynamic value
/// loses that. Bevy recomputes a camera's matrices and target size and a
/// perspective's aspect ratio every frame, so the file keeps their defaults.
/// A game camera switched off in edit mode keeps the switch it plays with,
/// `parked`. None for a value that cannot be copied either way.
// ponytail: the template's camera; an orthographic projection's `area` is
// recomputed too, clear it once 2D scenes are saved.
fn saved_value(value: &dyn Reflect, parked: Option<bool>) -> Option<Box<dyn PartialReflect>> {
    if let Some(camera) = value.downcast_ref::<Camera>() {
        return Some(Box::new(Camera {
            computed: default(),
            is_active: parked.unwrap_or(camera.is_active),
            ..camera.clone()
        }));
    }
    if let Some(Projection::Perspective(perspective)) = value.downcast_ref::<Projection>() {
        return Some(Box::new(Projection::Perspective(PerspectiveProjection {
            aspect_ratio: PerspectiveProjection::default().aspect_ratio,
            ..perspective.clone()
        })));
    }
    value
        .reflect_clone()
        .map(|clone| clone.into_partial_reflect())
        .or_else(|_| value.to_dynamic())
        .ok()
}

fn is_default(registry: &TypeRegistry, info: &ComponentInfo, value: &dyn PartialReflect) -> bool {
    info.type_id()
        .and_then(|id| registry.get_type_data::<ReflectDefault>(id))
        .and_then(|default| value.reflect_partial_eq(default.default().as_partial_reflect()))
        == Some(true)
}

/// Components that do not belong in the file: what the plugin or Bevy
/// builds from other components when the scene loads (`Children` follows
/// from each child's `ChildOf`), a directional light's shadow cascades,
/// which Bevy works out for each camera every frame, camera settings Bevy
/// does not make
/// serializable, the render target, which the editor points at its viewport
/// after the camera spawns, and the plugin's note of a camera it switched off.
// ponytail: what the template's entities carry; grow it as files show more.
pub(crate) fn left_out(type_id: TypeId) -> bool {
    [
        TypeId::of::<SceneEntity>(),
        TypeId::of::<crate::AnimationClips>(),
        TypeId::of::<crate::scene_view::Parked>(),
        TypeId::of::<Children>(),
        TypeId::of::<RenderTarget>(),
        TypeId::of::<Mesh3d>(),
        TypeId::of::<MeshMaterial3d<StandardMaterial>>(),
        TypeId::of::<GlobalTransform>(),
        TypeId::of::<TransformTreeChanged>(),
        TypeId::of::<InheritedVisibility>(),
        TypeId::of::<ViewVisibility>(),
        TypeId::of::<VisibilityClass>(),
        TypeId::of::<Aabb>(),
        TypeId::of::<BoundingSphere>(),
        TypeId::of::<Frustum>(),
        TypeId::of::<CubemapFrusta>(),
        TypeId::of::<VisibleEntities>(),
        TypeId::of::<CubemapVisibleEntities>(),
        TypeId::of::<Cascades>(),
        TypeId::of::<CascadesFrusta>(),
        TypeId::of::<CascadesVisibleEntities>(),
        TypeId::of::<SyncToRenderWorld>(),
        TypeId::of::<RenderEntity>(),
        TypeId::of::<CameraMainTextureUsages>(),
        TypeId::of::<CameraRenderGraph>(),
        TypeId::of::<ColorGrading>(),
        TypeId::of::<Exposure>(),
    ]
    .contains(&type_id)
}

#[cfg(test)]
mod tests {
    use bevy::{
        asset::{AssetPath, LoadFromPath, UntypedHandle},
        ecs::entity::EntityHashMap,
        world_serialization::serde::WorldDeserializer,
    };
    use serde::de::DeserializeSeed;

    use super::*;

    /// The template game's component, as `besfa new` writes it.
    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct Spin;

    /// The scene references no asset paths, so no handle is ever asked for.
    struct NoAssets;

    impl LoadFromPath for NoAssets {
        fn load_from_path_erased(&mut self, _: TypeId, path: AssetPath<'static>) -> UntypedHandle {
            unreachable!("the test scene references {path}")
        }
    }

    #[test]
    fn puts_loaded_children_under_their_parents_wherever_the_file_lists_them() {
        let mut app = test_app();
        app.add_plugins(TransformPlugin)
            .register_type::<Visibility>()
            .add_systems(PostUpdate, adopt_loaded_children);
        // The child before its parent, as saved files list them, and one
        // whose parent the file does not hold.
        let ron = r#"(
  resources: {},
  entities: {
    5: (components: {
      "bevy_ecs::hierarchy::ChildOf": (6),
      "bevy_ecs::name::Name": "Oak",
      "bevy_transform::components::transform::Transform": (translation: (1.0, 0.0, 2.0), rotation: (0.0, 0.0, 0.0, 1.0), scale: (1.0, 1.0, 1.0)),
      "bevy_camera::visibility::Visibility": Inherited,
    }),
    6: (components: {
      "bevy_ecs::name::Name": "Forest",
      "bevy_transform::components::transform::Transform": (translation: (-50.0, 0.0, 0.0), rotation: (0.0, 0.0, 0.0, 1.0), scale: (1.0, 1.0, 1.0)),
    }),
    7: (components: {
      "bevy_ecs::hierarchy::ChildOf": (99),
      "bevy_ecs::name::Name": "Lost",
    }),
  },
)"#;
        let registry = app.world().resource::<AppTypeRegistry>().clone();
        let scene = WorldDeserializer {
            type_registry: &registry.read(),
            load_from_path: &mut NoAssets,
        }
        .deserialize(&mut ron::de::Deserializer::from_str(ron).unwrap())
        .unwrap();
        let mut map = EntityHashMap::default();
        scene.write_to_world(app.world_mut(), &mut map).unwrap();
        app.update();
        app.update();

        let named = |app: &mut App, name: &str| {
            let mut query = app.world_mut().query::<(Entity, &Name)>();
            query
                .iter(app.world())
                .find(|(_, found)| found.as_str() == name)
                .map(|(entity, _)| entity)
                .unwrap()
        };
        let (oak, forest, lost) = (
            named(&mut app, "Oak"),
            named(&mut app, "Forest"),
            named(&mut app, "Lost"),
        );
        let world = app.world();
        assert!(
            world
                .get::<Children>(forest)
                .is_some_and(|c| c.contains(&oak))
        );
        assert_eq!(
            world.get::<GlobalTransform>(oak).unwrap().translation(),
            Vec3::new(-49.0, 0.0, 2.0),
            "drawn where it stands under its parent"
        );
        assert!(
            world.get::<Visibility>(forest).is_some(),
            "the group takes part in visibility"
        );
        assert!(
            world.get::<ChildOf>(lost).is_none(),
            "no parent: at the top"
        );

        // Despawning the parent takes the loaded child with it.
        app.world_mut().despawn(forest);
        assert!(app.world().get_entity(oak).is_err());
    }

    #[test]
    fn leaves_a_lights_shadow_cascades_out() {
        let mut app = test_app();
        app.register_type::<DirectionalLight>()
            .register_type::<Cascades>()
            .register_type::<CascadesFrusta>()
            .register_type::<CascadesVisibleEntities>();
        // What Bevy works out for a camera each frame.
        let camera = Entity::from_raw_u32(7).unwrap();
        let mut cascades = Cascades::default();
        cascades
            .cascades
            .insert(camera, vec![bevy::light::cascade::Cascade::default()]);
        let mut frusta = CascadesFrusta::default();
        frusta.frusta.insert(camera, vec![Frustum::default()]);
        let sun = app
            .world_mut()
            .spawn((SceneEntity, DirectionalLight::default(), cascades, frusta))
            .id();
        let dir = std::env::temp_dir().join(format!("besfa-sun-{}", std::process::id()));
        let path = dir.join(SCENE_PATH);
        save(app.world_mut(), &path).expect("the scene should save");
        let ron = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(ron.contains("DirectionalLight"), "{ron}");
        assert!(!ron.contains("Cascades"), "{ron}");
        assert!(app.world().get::<CascadesFrusta>(sun).is_some());
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .register_type::<Name>()
            .register_type::<Transform>()
            .register_type::<PointLight>()
            .register_type::<Camera3d>()
            .register_type::<MeshShape>()
            .register_type::<MeshColor>()
            .register_type::<Spin>()
            .register_type::<ChildOf>()
            .add_systems(Update, (build_meshes, build_materials))
            .add_observer(drop_mesh)
            .add_observer(drop_material);
        app
    }

    /// Spawns `ron` into the app the way the scene loader does and tags the
    /// entities, returning them in spawn order.
    fn spawn(app: &mut App, ron: &str) -> Vec<Entity> {
        let registry = app.world().resource::<AppTypeRegistry>().clone();
        let scene = WorldDeserializer {
            type_registry: &registry.read(),
            load_from_path: &mut NoAssets,
        }
        .deserialize(&mut ron::de::Deserializer::from_str(ron).unwrap())
        .expect("the scene should deserialize");
        let mut map = EntityHashMap::default();
        scene
            .write_to_world(app.world_mut(), &mut map)
            .expect("the scene should spawn");
        let mut entities: Vec<Entity> = map.values().copied().collect();
        entities.sort_unstable_by_key(|entity| entity.index_u32());
        for &entity in &entities {
            app.world_mut().entity_mut(entity).insert(SceneEntity);
        }
        // Applies the observers' commands.
        app.update();
        entities
    }

    #[test]
    fn loads_the_new_project_template() {
        let ron = include_str!("../../besfa_cli/template/main.scn.ron")
            .replace("{{crate}}", "besfa_editor_plugin::scene::tests");
        let mut app = test_app();

        let entities = spawn(&mut app, &ron);

        let world = app.world();
        let names: Vec<&str> = entities
            .iter()
            .map(|&entity| world.get::<Name>(entity).unwrap().as_str())
            .collect();
        assert_eq!(names, ["Cube", "Light", "Camera"]);
        assert!(world.get::<Spin>(entities[0]).is_some());
        assert!(world.get::<Mesh3d>(entities[0]).is_some());
        assert!(
            world
                .get::<MeshMaterial3d<StandardMaterial>>(entities[0])
                .is_some()
        );
        // Fields the file leaves out keep their defaults.
        let light = world.get::<PointLight>(entities[1]).unwrap();
        assert!(light.shadow_maps_enabled);
        assert_eq!(light.intensity, PointLight::default().intensity);
        assert!(world.get::<Camera3d>(entities[2]).is_some());
        assert_eq!(
            world.get::<Transform>(entities[2]).unwrap().translation,
            Vec3::new(-2.5, 4.5, 9.0)
        );
    }

    #[test]
    fn saves_the_scene_and_loads_it_back() {
        let mut app = test_app();
        let cube = app
            .world_mut()
            .spawn((
                Name::new("Cube"),
                MeshShape::Cuboid { size: Vec3::ONE },
                MeshColor(Color::srgb(0.55, 0.7, 1.0)),
                Transform::from_xyz(1.0, 2.0, 3.0),
                Spin,
                SceneEntity,
            ))
            .id();
        app.world_mut().spawn((
            Name::new("Camera"),
            Camera3d::default(),
            Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
            SceneEntity,
        ));
        app.update();
        let mesh = app
            .world()
            .get::<Mesh3d>(cube)
            .expect("the shape builds a mesh")
            .clone();
        *app.world_mut().get_mut::<MeshShape>(cube).unwrap() = MeshShape::Sphere { radius: 1.0 };
        app.update();
        assert_ne!(
            app.world().get::<Mesh3d>(cube),
            Some(&mesh),
            "a new shape builds a new mesh"
        );
        *app.world_mut().get_mut::<MeshShape>(cube).unwrap() = MeshShape::default();
        app.world_mut().entity_mut(cube).remove::<MeshColor>();
        app.update();
        assert!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(cube)
                .is_none(),
            "removing the color removes the material"
        );
        app.world_mut()
            .entity_mut(cube)
            .insert(MeshColor(Color::srgb(0.55, 0.7, 1.0)));
        app.update();
        let dir = std::env::temp_dir().join(format!("besfa-scene-{}", std::process::id()));
        let path = dir.join(SCENE_PATH);

        save(app.world_mut(), &path).expect("the scene should save");
        let ron = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        println!("{ron}");

        assert!(ron.contains("besfa_editor_plugin::scene::MeshShape"));
        assert!(ron.contains("besfa_editor_plugin::scene::tests::Spin"));
        assert!(!ron.contains("Mesh3d"), "handles stay out of the file");
        assert!(!ron.contains("GlobalTransform"), "computed values stay out");

        let mut reloaded = test_app();
        let entities = spawn(&mut reloaded, &ron);
        assert_eq!(entities.len(), 2);
        let world = reloaded.world();
        assert_eq!(world.get::<Name>(entities[0]).unwrap().as_str(), "Cube");
        assert_eq!(
            world.get::<Transform>(entities[0]).unwrap().translation,
            Vec3::new(1.0, 2.0, 3.0)
        );
        assert!(world.get::<Spin>(entities[0]).is_some());
        assert!(world.get::<Mesh3d>(entities[0]).is_some());
        assert!(
            world
                .get::<MeshMaterial3d<StandardMaterial>>(entities[0])
                .is_some()
        );
        assert!(world.get::<Camera3d>(entities[1]).is_some());
    }

    #[test]
    fn leaves_out_required_defaults_and_writes_requirers_first() {
        use bevy::{camera::Projection, reflect::TypePath};

        let mut app = test_app();
        app.register_type::<Camera>().register_type::<Projection>();
        let camera = app
            .world_mut()
            .spawn((
                Name::new("Camera"),
                Camera3d::default(),
                Transform::from_xyz(0.0, 1.0, 5.0),
                SceneEntity,
            ))
            .id();
        // What a running game computes every frame.
        app.world_mut()
            .get_mut::<Camera>(camera)
            .unwrap()
            .computed
            .old_viewport_size = Some(UVec2::new(1280, 720));
        if let Some(mut projection) = app.world_mut().get_mut::<Projection>(camera)
            && let Projection::Perspective(perspective) = &mut *projection
        {
            perspective.aspect_ratio = 16.0 / 9.0;
        }
        let dir = std::env::temp_dir().join(format!("besfa-required-{}", std::process::id()));
        let path = dir.join(SCENE_PATH);
        let quoted = |path: &str| format!("\"{path}\"");

        save(app.world_mut(), &path).expect("the scene should save");
        let ron = std::fs::read_to_string(&path).unwrap();
        assert!(!ron.contains(&quoted(Camera::type_path())), "{ron}");
        assert!(!ron.contains(&quoted(Projection::type_path())), "{ron}");
        assert!(
            ron.contains(&quoted(Transform::type_path())),
            "not a default"
        );

        // Switched off in edit mode, it is saved the way the game plays it.
        app.world_mut().get_mut::<Camera>(camera).unwrap().is_active = false;
        app.world_mut()
            .entity_mut(camera)
            .insert(crate::scene_view::Parked { active: true });
        save(app.world_mut(), &path).expect("the scene should save");
        let ron = std::fs::read_to_string(&path).unwrap();
        assert!(!ron.contains(&quoted(Camera::type_path())), "{ron}");

        app.world_mut().get_mut::<Camera>(camera).unwrap().order = 3;
        save(app.world_mut(), &path).expect("the scene should save");
        let ron = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let at = |path: &str| ron.find(&quoted(path)).unwrap_or_else(|| panic!("{ron}"));
        assert!(at(Camera3d::type_path()) < at(Camera::type_path()), "{ron}");

        let mut loaded = test_app();
        loaded
            .register_type::<Camera>()
            .register_type::<Projection>();
        // Bevy's renderer warns from this hook when the render graph that
        // Camera3d brings is missing.
        loaded
            .world_mut()
            .register_component_hooks::<Camera>()
            .on_add(|world, context| {
                assert!(world.entity(context.entity).contains::<Camera3d>());
            });
        let entities = spawn(&mut loaded, &ron);
        let loaded = loaded.world().get::<Camera>(entities[0]).unwrap();
        assert_eq!(loaded.order, 3);
        assert!(loaded.is_active);
    }

    #[test]
    fn saves_a_prefab_at_the_origin_with_its_scene_children() {
        let mut app = test_app();
        let world = app.world_mut();
        let parent = world.spawn((Name::new("Garden"), SceneEntity)).id();
        let tree = world
            .spawn((
                Name::new("Tree"),
                Transform::from_xyz(5.0, 0.0, 0.0),
                SceneEntity,
                ChildOf(parent),
            ))
            .id();
        world.spawn((
            Name::new("Leaf"),
            Transform::from_xyz(0.0, 1.0, 0.0),
            SceneEntity,
            ChildOf(tree),
        ));
        world.spawn((Name::new("Runtime"), ChildOf(tree)));
        let dir = std::env::temp_dir().join(format!("besfa-prefab-{}", std::process::id()));
        let path = dir.join("prefabs/tree.scn.ron");

        save_prefab(app.world_mut(), tree, &path).expect("the prefab should save");
        let ron = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(!ron.contains("Garden") && !ron.contains("Runtime"), "{ron}");
        let mut loaded = test_app();
        let entities = spawn(&mut loaded, &ron);
        let world = loaded.world();
        let [tree, leaf] = entities[..] else {
            panic!("two entities in {ron}");
        };
        assert_eq!(world.get::<Name>(tree).unwrap().as_str(), "Tree");
        assert_eq!(world.get::<Transform>(tree), Some(&Transform::default()));
        assert!(
            world.get::<ChildOf>(tree).is_none(),
            "the root has no parent"
        );
        assert_eq!(world.get::<ChildOf>(leaf).map(ChildOf::parent), Some(tree));
        assert_eq!(
            world.get::<Transform>(leaf).unwrap().translation,
            Vec3::new(0.0, 1.0, 0.0)
        );
    }

    #[test]
    fn applies_what_a_placed_prefab_shows_back_to_its_file() {
        use bevy::world_serialization::WorldSerializationPlugin;

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            WorldSerializationPlugin,
        ))
        .register_type::<Name>()
        .register_type::<Transform>()
        .register_type::<ChildOf>();
        let assets = app.world().resource::<AssetServer>().clone();
        let world = app.world_mut();
        let placed = world
            .spawn((
                Name::new("Tree"),
                Transform::from_xyz(9.0, 0.0, 0.0),
                DynamicWorldRoot(assets.load("prefabs/tree.scn.ron")),
                SceneEntity,
            ))
            .id();
        // What the prefab spawned, changed in the editor.
        let trunk = world
            .spawn((
                Name::new("Trunk"),
                Transform::from_xyz(0.0, 2.0, 0.0),
                ChildOf(placed),
            ))
            .id();
        let rock = world
            .spawn((
                Name::new("Rock"),
                DynamicWorldRoot(assets.load("prefabs/rock.scn.ron")),
                ChildOf(trunk),
            ))
            .id();
        world.spawn((Name::new("RockInside"), ChildOf(rock)));
        world.spawn((Name::new("Gone"), ChildOf(trunk), crate::edit::Deleted));
        world.spawn((Name::new("Copy"), ChildOf(placed), SceneEntity));
        let dir = std::env::temp_dir().join(format!("besfa-apply-{}", std::process::id()));
        let path = dir.join("tree.scn.ron");

        assert_eq!(
            instance_prefab(app.world(), placed).as_deref(),
            Ok("prefabs/tree.scn.ron")
        );
        save_instance(app.world_mut(), placed, &path).expect("the prefab should save");
        let ron = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        for left_out in ["\"Tree\"", "RockInside", "Gone", "Copy", "tree.scn.ron"] {
            assert!(!ron.contains(left_out), "{left_out} in {ron}");
        }
        assert!(ron.contains("\"prefabs/rock.scn.ron\""), "{ron}");
        let registry = app.world().resource::<AppTypeRegistry>().clone();
        let scene = WorldDeserializer {
            type_registry: &registry.read(),
            load_from_path: &mut &assets,
        }
        .deserialize(&mut ron::de::Deserializer::from_str(&ron).unwrap())
        .expect("the prefab should deserialize");
        let mut map = EntityHashMap::default();
        let mut loaded = World::new();
        loaded.insert_resource(registry.clone());
        scene
            .write_to_world(&mut loaded, &mut map)
            .expect("the prefab should spawn");
        let named = |name: &str| {
            *map.values()
                .find(|&&entity| loaded.get::<Name>(entity).unwrap().as_str() == name)
                .unwrap()
        };
        let (loaded_trunk, loaded_rock) = (named("Trunk"), named("Rock"));
        assert_eq!(map.len(), 2, "{ron}");
        assert!(loaded.get::<ChildOf>(loaded_trunk).is_none(), "a top");
        assert_eq!(
            loaded.get::<Transform>(loaded_trunk).unwrap().translation,
            Vec3::new(0.0, 2.0, 0.0),
            "changes inside the instance are kept"
        );
        assert_eq!(
            loaded.get::<ChildOf>(loaded_rock).map(ChildOf::parent),
            Some(loaded_trunk)
        );

        // An instance that holds itself would load forever.
        app.world_mut()
            .entity_mut(rock)
            .insert(DynamicWorldRoot(assets.load("prefabs/tree.scn.ron")));
        // Refused before anything is written.
        let untouched = |path: &Path| std::fs::read(path).ok();
        for refused in [asset_file("prefabs/tree.scn.ron"), scene_file()] {
            let before = untouched(&refused);
            assert!(save_instance(app.world_mut(), placed, &refused).is_err());
            assert_eq!(untouched(&refused), before, "{}", refused.display());
        }
    }

    #[test]
    fn saves_a_placed_asset_as_its_path_and_loads_it_back() {
        use bevy::world_serialization::{DynamicWorldRoot, WorldSerializationPlugin};

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            WorldSerializationPlugin,
        ))
        .register_type::<Name>();
        let assets = app.world().resource::<AssetServer>().clone();
        app.world_mut().spawn((
            Name::new("Tree"),
            DynamicWorldRoot(assets.load("prefabs/tree.scn.ron")),
            SceneEntity,
        ));
        let dir = std::env::temp_dir().join(format!("besfa-placed-{}", std::process::id()));
        let path = dir.join("main.scn.ron");

        save(app.world_mut(), &path).expect("the scene should save");
        let ron = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(ron.contains("\"prefabs/tree.scn.ron\""), "{ron}");

        let registry = app.world().resource::<AppTypeRegistry>().clone();
        let scene = WorldDeserializer {
            type_registry: &registry.read(),
            load_from_path: &mut &assets,
        }
        .deserialize(&mut ron::de::Deserializer::from_str(&ron).unwrap())
        .expect("the scene should deserialize");
        let mut map = EntityHashMap::default();
        scene
            .write_to_world(app.world_mut(), &mut map)
            .expect("the scene should spawn");
        let placed = *map.values().next().unwrap();
        let root = app.world().get::<DynamicWorldRoot>(placed).unwrap();
        assert_eq!(
            root.0.path().map(ToString::to_string).as_deref(),
            Some("prefabs/tree.scn.ron")
        );
    }
}
