//! Foliage: plants that grow on a terrain as thickly as one of its surface
//! layers covers the ground, like grass wherever the ground is painted
//! grass. Painting the terrain's layers paints its foliage too.
//!
//! Plants are not saved: each grows in its own cell of a grid as fine as
//! the density, so they grow the same every time. Only the squares of
//! plants near a camera exist, each plant an entity per mesh of its model,
//! which Bevy draws instanced.

use std::{f32::consts::TAU, hash::BuildHasher};

use bevy::{
    camera::visibility::VisibilityRange,
    gltf::{Gltf, GltfMesh, GltfNode},
    light::NotShadowCaster,
    platform::{collections::HashMap, hash::FixedHasher},
    prelude::*,
};

use crate::{
    EditorOnly,
    terrain::{Terrain, TerrainData, TerrainHandle, local},
};

/// Meters along each side of the square of plants that comes and goes as
/// a whole.
const CHUNK: f32 = 16.0;

/// The most plants a square meter: past this a mistyped density would
/// spawn millions.
const MAX_DENSITY: f32 = 64.0;

pub struct FoliagePlugin;

impl Plugin for FoliagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (load, grow).chain());
    }
}

/// Plants that grow on the terrain this entity is, or is under, as thickly
/// as one of its surface layers covers the ground. One kind of plant each;
/// a terrain with grass and flowers has two.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::foliage"]
#[reflect(Component, Default)]
pub struct Foliage {
    /// The glTF model each plant shows, relative to the asset folder.
    pub model: String,
    /// The terrain's surface layer, 0 to 3, the plants grow on.
    pub layer: usize,
    /// Plants a square meter where the layer covers the ground.
    pub density: f32,
    /// The smallest and the largest size of a plant.
    pub scale: Vec2,
    /// How far from a camera plants show, in meters; they fade out over
    /// the last fifth.
    pub distance: f32,
    pub shadows: bool,
}

impl Default for Foliage {
    fn default() -> Self {
        Self {
            model: String::new(),
            layer: 0,
            density: 1.0,
            scale: Vec2::new(0.8, 1.2),
            distance: 60.0,
            shadows: false,
        }
    }
}

/// The model a [`Foliage`] shows, loading or loaded.
#[derive(Component)]
struct FoliageModel(Handle<Gltf>);

/// One square of a foliage's plants, in its terrain's space; the plants
/// are its children.
#[derive(Component)]
struct Chunk {
    foliage: Entity,
    at: IVec2,
    /// What it grew from, to tell when it must grow again.
    grown: (Foliage, Vec<Transform>),
}

fn load(
    mut commands: Commands,
    foliage: Query<(Entity, &Foliage), Changed<Foliage>>,
    server: Res<AssetServer>,
) {
    for (entity, foliage) in &foliage {
        // The model itself, also when named by one of its scenes.
        let path = foliage.model.split('#').next().unwrap_or_default();
        let mut entity = commands.entity(entity);
        if path.is_empty() {
            entity.remove::<FoliageModel>();
        } else {
            entity.insert(FoliageModel(server.load(path.to_owned())));
        }
    }
}

/// Grows the squares of plants near a camera and drops those that left,
/// and grows a square again when what it grows from changed: its foliage,
/// its terrain or the terrain's file, or the model.
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "a system: each parameter is a query or resource it needs"
)]
fn grow(
    mut commands: Commands,
    foliage: Query<(Entity, Ref<Foliage>, Option<&FoliageModel>, Option<&ChildOf>)>,
    terrains: Query<(Ref<Terrain>, Ref<TerrainHandle>, &GlobalTransform)>,
    mut chunks: Query<(Entity, &Chunk, &mut Transform)>,
    cameras: Query<&GlobalTransform, With<Camera>>,
    mut painted: MessageReader<AssetEvent<TerrainData>>,
    mut reloaded: MessageReader<AssetEvent<Gltf>>,
    files: Res<Assets<TerrainData>>,
    models: (Res<Assets<Gltf>>, Res<Assets<GltfNode>>, Res<Assets<GltfMesh>>),
    server: Res<AssetServer>,
) {
    let painted: Vec<_> = painted.read().map(event_id).collect();
    let reloaded: Vec<_> = reloaded.read().map(event_id).collect();
    let eyes: Vec<Vec3> = cameras.iter().map(GlobalTransform::translation).collect();
    let mut left: HashMap<(Entity, IVec2), Entity> = chunks
        .iter()
        .map(|(entity, chunk, _)| ((chunk.foliage, chunk.at), entity))
        .collect();
    for (entity, foliage, model, parent) in &foliage {
        let Some((terrain, file, global)) = terrains
            .get(entity)
            .ok()
            .or_else(|| terrains.get(parent?.parent()).ok())
        else {
            continue;
        };
        let (Some(data), Some(model)) = (files.get(&file.handle), model) else {
            continue;
        };
        let Some(gltf) = models.0.get(&model.0) else {
            continue;
        };
        let reload = reloaded.contains(&model.0.id());
        let fresh = reload
            || foliage.is_changed()
            || terrain.is_changed()
            || file.is_changed()
            || painted.contains(&file.handle.id());
        let transform = global.compute_transform();
        let eyes: Vec<Vec2> = eyes.iter().map(|&eye| local(&transform, eye)).collect();
        let mut parts = None;
        let side = (terrain.size / CHUNK).ceil() as i32;
        for at in (0..side).flat_map(|z| (0..side).map(move |x| IVec2::new(x, z))) {
            let min = at.as_vec2() * CHUNK - terrain.size / 2.0;
            let near = eyes
                .iter()
                .map(|&eye| eye.clamp(min, min + CHUNK).distance(eye))
                .reduce(f32::min);
            let had = left.remove(&(entity, at));
            // A square that is there stays a little longer, so one at the
            // edge does not come and go as the camera sways.
            let reach = foliage.distance + if had.is_some() { CHUNK } else { 0.0 };
            if near.is_none_or(|near| near >= reach) {
                if let Some(chunk) = had {
                    commands.entity(chunk).despawn();
                }
                continue;
            }
            let plants = match had.and_then(|chunk| chunks.get_mut(chunk).ok()) {
                Some((chunk, grown, mut placed)) => {
                    placed.set_if_neq(transform);
                    if !fresh {
                        continue;
                    }
                    let plants = plants(&foliage, terrain.size, data, at);
                    if !reload && grown.grown.0 == *foliage && grown.grown.1 == plants {
                        continue;
                    }
                    commands.entity(chunk).despawn();
                    plants
                }
                None => plants(&foliage, terrain.size, data, at),
            };
            let parts = parts.get_or_insert_with(|| {
                let path = foliage.model.split('#').next().unwrap_or_default();
                meshes(path, gltf, &models.1, &models.2, &server)
            });
            spawn(&mut commands, entity, at, &foliage, plants, parts, transform);
        }
    }
    // Squares of foliage that is gone, hidden, or off its terrain.
    for chunk in left.into_values() {
        commands.entity(chunk).despawn();
    }
}

fn event_id<A: Asset>(event: &AssetEvent<A>) -> AssetId<A> {
    match *event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::Removed { id }
        | AssetEvent::Unused { id }
        | AssetEvent::LoadedWithDependencies { id } => id,
    }
}

// ponytail: an entity for each mesh of each plant, which Bevy instances; a
// square drawn from one instance buffer if plants near the camera pass
// about a hundred thousand.
fn spawn(
    commands: &mut Commands,
    foliage_entity: Entity,
    at: IVec2,
    foliage: &Foliage,
    plants: Vec<Transform>,
    parts: &[(Handle<Mesh>, Handle<StandardMaterial>, Transform)],
    transform: Transform,
) {
    let range = VisibilityRange {
        start_margin: 0.0..0.0,
        end_margin: foliage.distance * 0.8..foliage.distance,
        use_aabb: false,
    };
    commands
        .spawn((transform, Visibility::default(), EditorOnly))
        .with_children(|chunk| {
            for plant in &plants {
                for (mesh, material, part) in parts {
                    let mut entity = chunk.spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        plant.mul_transform(*part),
                        range.clone(),
                        EditorOnly,
                    ));
                    if !foliage.shadows {
                        entity.insert(NotShadowCaster);
                    }
                }
            }
        })
        .insert(Chunk {
            foliage: foliage_entity,
            at,
            grown: (foliage.clone(), plants),
        });
}

/// The meshes a glTF model at `path` shows, each with its material and
/// where it is in the model.
fn meshes(
    path: &str,
    gltf: &Gltf,
    nodes: &Assets<GltfNode>,
    meshes: &Assets<GltfMesh>,
    server: &AssetServer,
) -> Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)> {
    let children: Vec<AssetId<GltfNode>> = gltf
        .nodes
        .iter()
        .filter_map(|node| nodes.get(node))
        .flat_map(|node| node.children.iter().map(Handle::id))
        .collect();
    let mut stack: Vec<(&Handle<GltfNode>, Transform)> = gltf
        .nodes
        .iter()
        .filter(|node| !children.contains(&node.id()))
        .map(|node| (node, Transform::IDENTITY))
        .collect();
    let mut parts = Vec::new();
    while let Some((node, above)) = stack.pop() {
        let Some(node) = nodes.get(node) else {
            continue;
        };
        let at = above.mul_transform(node.transform);
        stack.extend(node.children.iter().map(|child| (child, at)));
        let primitives = node.mesh.as_ref().and_then(|mesh| meshes.get(mesh));
        for primitive in primitives.into_iter().flat_map(|mesh| &mesh.primitives) {
            // Bevy keeps each glTF material's `StandardMaterial` under the
            // material's label with `/std`, as the model's scenes use it.
            let material = primitive
                .material
                .as_ref()
                .and_then(|material| material.path()?.label())
                .unwrap_or("DefaultMaterial");
            parts.push((
                primitive.mesh.clone(),
                server.load(format!("{path}#{material}/std")),
                at,
            ));
        }
    }
    parts
}

/// Where the plants of `foliage` stand in square `at` of a terrain `size`
/// wide, in the terrain's space: one in each cell of a grid as fine as the
/// density, kept as often as the layer covers the ground there, turned
/// and sized at random by its cell alone.
fn plants(foliage: &Foliage, size: f32, data: &TerrainData, at: IVec2) -> Vec<Transform> {
    let density = foliage.density.min(MAX_DENSITY);
    if density <= 0.0 {
        return Vec::new();
    }
    let cell = density.recip().sqrt();
    // The cells whose corner is in the square.
    let start = (at.as_vec2() * CHUNK / cell).ceil().as_ivec2();
    let end = ((at + 1).as_vec2() * CHUNK / cell).ceil().as_ivec2();
    let mut plants = Vec::new();
    for z in start.y..end.y {
        for x in start.x..end.x {
            let hash = FixedHasher.hash_one((&foliage.model, foliage.layer, x, z));
            let [across, down, keep, turn, size_of] =
                [0, 12, 24, 36, 48].map(|shift| ((hash >> shift) & 0xfff) as f32 / 4096.0);
            let point = Vec2::new(x as f32 + across, z as f32 + down) * cell - size / 2.0;
            let (Some(cover), Some(height)) = (
                data.cover(size, point, foliage.layer),
                data.height(size, point),
            ) else {
                continue;
            };
            if keep >= cover {
                continue;
            }
            plants.push(
                Transform::from_xyz(point.x, height, point.y)
                    .with_rotation(Quat::from_rotation_y(turn * TAU))
                    .with_scale(Vec3::splat(
                        foliage.scale.x.lerp(foliage.scale.y, size_of),
                    )),
            );
        }
    }
    plants
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grass() -> Foliage {
        Foliage {
            model: "grass.glb".into(),
            ..default()
        }
    }

    #[test]
    fn grows_one_a_cell_where_the_layer_covers_the_ground() {
        // 16 m, one square, a sample a meter.
        let mut data = TerrainData::flat(17);
        data.heights.fill(2.0);
        let plants = plants(&grass(), 16.0, &data, IVec2::ZERO);
        assert_eq!(plants.len(), 256);
        assert!(plants.iter().all(|plant| plant.translation.y == 2.0));
        assert!(plants.iter().all(|plant| {
            let scale = plant.scale.x;
            (0.8..=1.2).contains(&scale)
        }));
        let flowers = Foliage {
            layer: 1,
            ..grass()
        };
        assert!(super::plants(&flowers, 16.0, &data, IVec2::ZERO).is_empty());
    }

    #[test]
    fn grows_as_thickly_as_the_layer_covers_and_the_same_each_time() {
        let mut data = TerrainData::flat(17);
        data.weights.fill([128, 127, 0, 0]);
        let half = plants(&grass(), 16.0, &data, IVec2::ZERO);
        assert!((96..160).contains(&half.len()), "{}", half.len());
        assert_eq!(half, plants(&grass(), 16.0, &data, IVec2::ZERO));
        let thick = Foliage {
            density: 4.0,
            ..grass()
        };
        let thick = plants(&thick, 16.0, &data, IVec2::ZERO).len();
        assert!((384..640).contains(&thick), "{thick}");
    }

    #[test]
    fn each_cell_grows_in_one_square() {
        // 32 m: four squares, a cell of 0.7 m that does not line up with
        // them.
        let data = TerrainData::flat(33);
        let foliage = Foliage {
            density: 2.0,
            ..grass()
        };
        let squares = [(0, 0), (1, 0), (0, 1), (1, 1)];
        let mut all: Vec<Transform> = squares
            .iter()
            .flat_map(|&(x, z)| plants(&foliage, 32.0, &data, IVec2::new(x, z)))
            .collect();
        let count = all.len();
        all.sort_by(|a, b| {
            let (a, b) = (a.translation, b.translation);
            a.x.total_cmp(&b.x).then(a.z.total_cmp(&b.z))
        });
        all.dedup_by(|a, b| a.translation == b.translation);
        assert_eq!(all.len(), count);
        // 46 cells a side, less those past the far edge.
        assert!((2025..=2116).contains(&count), "{count}");
    }

    #[test]
    fn grows_nothing_without_density() {
        let data = TerrainData::flat(17);
        let none = Foliage {
            density: 0.0,
            ..grass()
        };
        assert!(plants(&none, 16.0, &data, IVec2::ZERO).is_empty());
    }
}
