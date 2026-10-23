//! Terrain: ground of a given size whose heights and surface layers are
//! painted with the editor's brushes. The scene file keeps a terrain's size
//! and layer colors; its heights and where each layer shows are a
//! `.terrain` file of their own in the asset folder.
//!
//! Games ask the ground's height with [`TerrainHeight`].

use std::io;

use bevy::{
    asset::{AssetLoader, LoadContext, RenderAssetUsages, io::Reader},
    camera::visibility::VisibilitySystems,
    ecs::system::SystemParam,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

/// Samples along each side of a new terrain: one a meter at its first size.
#[doc(hidden)]
pub const RESOLUTION: u32 = 257;

/// The most samples along a side a terrain file may have.
#[doc(hidden)]
pub const MAX_RESOLUTION: u32 = 1025;

/// Starts a `.terrain` file, followed by [`FORMAT`].
#[doc(hidden)]
pub const MAGIC: &[u8; 4] = b"BTRN";
#[doc(hidden)]
pub const FORMAT: u32 = 1;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<TerrainData>()
            .register_asset_loader(TerrainLoader)
            // Before bounds are worked out, so a painted terrain's mesh is
            // hit where it now is by the next frame's brush.
            .add_systems(
                PostUpdate,
                (load, build)
                    .chain()
                    .before(VisibilitySystems::CalculateBounds),
            )
            .add_observer(drop_mesh);
    }
}

/// Ground the scene file keeps: how big it is and the colors of its
/// surface layers. Its heights and where each layer shows are in the
/// [`TerrainFile`] next to it.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::terrain"]
#[reflect(Component, Default)]
pub struct Terrain {
    /// Meters along X and along Z, centered on the entity.
    pub size: f32,
    /// The colors of at most four surface layers. A new terrain is covered
    /// by the first.
    pub layers: Vec<Color>,
}

impl Default for Terrain {
    fn default() -> Self {
        Self {
            size: 256.0,
            layers: vec![
                // Grass, dirt, stone and sand.
                Color::srgb(0.42, 0.62, 0.30),
                Color::srgb(0.55, 0.42, 0.28),
                Color::srgb(0.52, 0.52, 0.54),
                Color::srgb(0.85, 0.78, 0.56),
            ],
        }
    }
}

/// Tints the ground of every terrain round it: within `radius` meters of
/// it the ground's color turns toward `color` by `strength`, 0 to 1, in full
/// over the inner half and fading out to the edge. As a blight round a
/// nest, or snow round a peak; taking it away gives the ground its colors
/// back.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::terrain"]
#[reflect(Component, Default)]
pub struct TerrainTint {
    pub radius: f32,
    pub color: Color,
    pub strength: f32,
}

impl Default for TerrainTint {
    fn default() -> Self {
        Self {
            radius: 10.0,
            color: Color::srgb(0.45, 0.45, 0.45),
            strength: 0.8,
        }
    }
}

/// A tint where a terrain's mesh is built: its middle in the terrain's own
/// X and Z, and its numbers.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalTint {
    pub at: Vec2,
    pub radius: f32,
    pub color: LinearRgba,
    pub strength: f32,
}

impl LocalTint {
    /// How much of its color shows at `point`, 0 to 1.
    fn share(&self, point: Vec2) -> f32 {
        if self.radius <= 0.0 {
            return 0.0;
        }
        let near = 1.0 - point.distance(self.at) / self.radius;
        (near * 2.0).clamp(0.0, 1.0) * self.strength.clamp(0.0, 1.0)
    }
}

/// The `.terrain` file with a terrain's heights and where each layer
/// shows, relative to the asset folder.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::terrain"]
#[reflect(Component, Default)]
pub struct TerrainFile(pub String);

/// The heights and layers a terrain shows: its [`TerrainFile`] as loaded,
/// or as painted since. Not saved; the file's path is.
#[derive(Component, Clone, Debug)]
pub struct TerrainHandle {
    /// The file this was loaded from, or written to when it was made.
    pub path: String,
    pub handle: Handle<TerrainData>,
}

/// A terrain's heights and where each surface layer shows, sampled on a
/// square grid `resolution` samples a side: row by row from -Z to +Z, each
/// row from -X to +X.
#[derive(Asset, TypePath, Clone, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::terrain"]
pub struct TerrainData {
    pub resolution: u32,
    /// Meters above the terrain's origin.
    pub heights: Vec<f32>,
    /// How much of each of four layers shows, out of 255, summing to 255.
    pub weights: Vec<[u8; 4]>,
}

impl TerrainData {
    /// Flat ground at height 0, covered by the first layer.
    pub fn flat(resolution: u32) -> Self {
        let count = (resolution * resolution) as usize;
        Self {
            resolution,
            heights: vec![0.0; count],
            weights: vec![[255, 0, 0, 0]; count],
        }
    }

    /// The file's bytes: the magic, the format and the resolution as
    /// little-endian u32s, then each height as a little-endian f32, then
    /// each sample's four layer weights.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(12 + self.heights.len() * 8);
        bytes.extend(MAGIC);
        bytes.extend(FORMAT.to_le_bytes());
        bytes.extend(self.resolution.to_le_bytes());
        for height in &self.heights {
            bytes.extend(height.to_le_bytes());
        }
        for weights in &self.weights {
            bytes.extend(weights);
        }
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let word = |at: usize| {
            bytes
                .get(at..at + 4)
                .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
                .ok_or("the file ends early")
        };
        if bytes.get(..4) != Some(MAGIC) {
            return Err("not a terrain file".into());
        }
        if word(4)? != FORMAT {
            return Err(format!("terrain format {} is not {FORMAT}", word(4)?));
        }
        let resolution = word(8)?;
        if !(2..=MAX_RESOLUTION).contains(&resolution) {
            return Err(format!(
                "a terrain is 2 to {MAX_RESOLUTION} samples a side, not {resolution}"
            ));
        }
        let count = (resolution * resolution) as usize;
        let body = &bytes[12..];
        if body.len() != count * 8 {
            return Err(format!(
                "{} bytes of samples where {resolution} a side take {}",
                body.len(),
                count * 8
            ));
        }
        let (heights, weights) = body.split_at(count * 4);
        Ok(Self {
            resolution,
            heights: heights
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&bytes| f32::from_le_bytes(bytes))
                .collect(),
            weights: weights.as_chunks::<4>().0.to_vec(),
        })
    }

    /// Meters between samples on a terrain `size` wide.
    #[doc(hidden)]
    pub fn step(&self, size: f32) -> f32 {
        size / (self.resolution - 1) as f32
    }

    #[doc(hidden)]
    pub fn index(&self, x: u32, z: u32) -> usize {
        (z * self.resolution + x) as usize
    }

    /// Where sample `x`, `z` is in the terrain's space, X and Z.
    #[doc(hidden)]
    pub fn position(&self, size: f32, x: u32, z: u32) -> Vec2 {
        Vec2::new(x as f32, z as f32) * self.step(size) - size / 2.0
    }

    /// The height at `point`, X and Z in the space of a terrain `size`
    /// wide, between the samples around it; none off the terrain.
    pub fn height(&self, size: f32, point: Vec2) -> Option<f32> {
        self.between(size, point, |index| self.heights[index])
    }

    /// How much surface `layer` covers the ground at `point`, 0 to 1, X
    /// and Z in the space of a terrain `size` wide; none off the terrain or
    /// past the fourth layer.
    pub fn cover(&self, size: f32, point: Vec2, layer: usize) -> Option<f32> {
        if layer >= 4 {
            return None;
        }
        self.between(size, point, |index| {
            self.weights[index][layer] as f32 / 255.0
        })
    }

    /// `value` of the samples around `point`, blended by how near each is.
    fn between(&self, size: f32, point: Vec2, value: impl Fn(usize) -> f32) -> Option<f32> {
        let grid = (point + size / 2.0) / self.step(size);
        let last = (self.resolution - 1) as f32;
        if !(0.0..=last).contains(&grid.x) || !(0.0..=last).contains(&grid.y) {
            return None;
        }
        let x = (grid.x as u32).min(self.resolution - 2);
        let z = (grid.y as u32).min(self.resolution - 2);
        let (across, down) = (grid.x - x as f32, grid.y - z as f32);
        let value = |x, z| value(self.index(x, z));
        let back = value(x, z).lerp(value(x + 1, z), across);
        let front = value(x, z + 1).lerp(value(x + 1, z + 1), across);
        Some(back.lerp(front, down))
    }

    /// The ground as a mesh, each vertex colored by the layers that show
    /// there and turned toward the colors of the `tints` round it.
    // ponytail: one mesh rebuilt whole after every change, fine up to the
    // largest resolution; split it into chunks if painting big ones lags.
    #[doc(hidden)]
    pub fn mesh(&self, terrain: &Terrain, tints: &[LocalTint]) -> Mesh {
        let n = self.resolution;
        let step = self.step(terrain.size);
        let colors: Vec<LinearRgba> = (0..4)
            .map(|layer| {
                terrain
                    .layers
                    .get(layer)
                    .or(terrain.layers.first())
                    .map_or(LinearRgba::WHITE, |color| color.to_linear())
            })
            .collect();
        let height = |x: u32, z: u32| self.heights[self.index(x.min(n - 1), z.min(n - 1))];
        let count = (n * n) as usize;
        let (mut positions, mut normals, mut tints_out, mut uvs) = (
            Vec::with_capacity(count),
            Vec::with_capacity(count),
            Vec::with_capacity(count),
            Vec::with_capacity(count),
        );
        for z in 0..n {
            for x in 0..n {
                let at = self.position(terrain.size, x, z);
                positions.push([at.x, height(x, z), at.y]);
                let slope = Vec3::new(
                    height(x.saturating_sub(1), z) - height(x + 1, z),
                    2.0 * step,
                    height(x, z.saturating_sub(1)) - height(x, z + 1),
                );
                normals.push(slope.normalize().to_array());
                let weights = self.weights[self.index(x, z)];
                let tint = (0..4).fold(Vec3::ZERO, |tint, layer| {
                    let color = colors[layer];
                    tint + Vec3::new(color.red, color.green, color.blue) * weights[layer] as f32
                        / 255.0
                });
                let tint = tints.iter().fold(tint, |tint, over| {
                    tint.lerp(Vec3::new(over.color.red, over.color.green, over.color.blue), over.share(at))
                });
                tints_out.push([tint.x, tint.y, tint.z, 1.0]);
                uvs.push([x as f32 / (n - 1) as f32, z as f32 / (n - 1) as f32]);
            }
        }
        let mut indices = Vec::with_capacity(((n - 1) * (n - 1) * 6) as usize);
        for z in 0..n - 1 {
            for x in 0..n - 1 {
                let [a, b, c, d] = [(x, z), (x + 1, z), (x, z + 1), (x + 1, z + 1)]
                    .map(|(x, z)| self.index(x, z) as u32);
                // Counterclockwise seen from above.
                indices.extend([a, c, b, b, c, d]);
            }
        }
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, tints_out)
        .with_inserted_indices(Indices::U32(indices))
    }
}

#[derive(TypePath)]
#[type_path = "besfa_editor_plugin::terrain"]
struct TerrainLoader;

impl AssetLoader for TerrainLoader {
    type Asset = TerrainData;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        _: &mut LoadContext<'_>,
    ) -> Result<TerrainData, io::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        TerrainData::from_bytes(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn extensions(&self) -> &[&str] {
        &["terrain"]
    }
}

/// Loads each terrain's file when it gets one, or another. A terrain made
/// in the editor already holds what it wrote to its file.
fn load(
    mut commands: Commands,
    terrains: Query<(Entity, &TerrainFile, Option<&TerrainHandle>), Changed<TerrainFile>>,
    server: Res<AssetServer>,
) {
    for (entity, file, held) in &terrains {
        if held.is_none_or(|held| held.path != file.0) {
            commands.entity(entity).insert(TerrainHandle {
                path: file.0.clone(),
                handle: server.load(file.0.clone()),
            });
        }
    }
}

/// Builds each terrain's mesh from its file once the file has loaded, and
/// again whenever the terrain or its file changes; and its material once.
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "a system's parameters: terrains, what they were given and the tints"
)]
fn build(
    mut commands: Commands,
    terrains: Query<(
        Entity,
        Ref<Terrain>,
        Ref<TerrainHandle>,
        &Transform,
        Has<Mesh3d>,
        Has<MeshMaterial3d<StandardMaterial>>,
    )>,
    tinting: Query<(&TerrainTint, &GlobalTransform)>,
    mut tinted: Local<Vec<(TerrainTint, Vec3)>>,
    mut events: MessageReader<AssetEvent<TerrainData>>,
    files: Res<Assets<TerrainData>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let changed: Vec<AssetId<TerrainData>> = events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .collect();
    // Tints that came, went, moved or changed rebuild every terrain.
    let tints: Vec<(TerrainTint, Vec3)> = tinting
        .iter()
        .map(|(tint, at)| (tint.clone(), at.translation()))
        .collect();
    let retint = *tinted != tints;
    if retint {
        *tinted = tints.clone();
    }
    for (entity, terrain, file, place, meshed, painted) in &terrains {
        let fresh = terrain.is_changed()
            || file.is_changed()
            || changed.contains(&file.handle.id())
            || retint;
        if meshed && !fresh {
            continue;
        }
        let Some(data) = files.get(&file.handle) else {
            continue;
        };
        let mut entity = commands.entity(entity);
        let local: Vec<LocalTint> = tints
            .iter()
            .map(|(tint, at)| LocalTint {
                at: self::local(place, *at),
                radius: tint.radius / place.scale.x.max(f32::EPSILON),
                color: tint.color.to_linear(),
                strength: tint.strength,
            })
            .collect();
        entity.insert(Mesh3d(meshes.add(data.mesh(&terrain, &local))));
        if !painted {
            // The vertices carry the layers' colors.
            entity.insert(MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 1.0,
                ..default()
            })));
        }
    }
}

fn drop_mesh(remove: On<Remove<Terrain>>, mut commands: Commands) {
    if let Ok(mut entity) = commands.get_entity(remove.entity) {
        entity.try_remove::<(Mesh3d, MeshMaterial3d<StandardMaterial>)>();
    }
}

/// The height of the ground of `terrain` at world `x`, `z`, if it covers
/// that point; `transform` places the terrain in the world.
// ponytail: terrains are only turned about Y; a tilted one gives the
// height straight above its own grid.
#[doc(hidden)]
pub fn surface(
    terrain: &Terrain,
    data: &TerrainData,
    transform: &Transform,
    x: f32,
    z: f32,
) -> Option<f32> {
    let point = Vec3::new(x, transform.translation.y, z);
    data.height(terrain.size, local(transform, point))
        .map(|height| transform.translation.y + height * transform.scale.y)
}

/// The height of the ground, for game code: what walks on the terrain asks
/// it where the ground is.
#[derive(SystemParam)]
pub struct TerrainHeight<'w, 's> {
    terrains: Query<
        'w,
        's,
        (
            &'static Terrain,
            &'static TerrainHandle,
            &'static GlobalTransform,
        ),
    >,
    files: Res<'w, Assets<TerrainData>>,
}

impl TerrainHeight<'_, '_> {
    /// The height of the highest terrain at world `x`, `z`, or none where
    /// no terrain is or its file has not loaded yet.
    pub fn at(&self, x: f32, z: f32) -> Option<f32> {
        self.terrains
            .iter()
            .filter_map(|(terrain, file, global)| {
                let data = self.files.get(&file.handle)?;
                surface(terrain, data, &global.compute_transform(), x, z)
            })
            .reduce(f32::max)
    }
}

/// `point`, in the world, in the space of a terrain placed by `transform`,
/// X and Z.
#[doc(hidden)]
pub fn local(transform: &Transform, point: Vec3) -> Vec2 {
    (transform.rotation.inverse() * (point - transform.translation) / transform.scale).xz()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_back_what_it_writes() {
        let mut data = TerrainData::flat(3);
        data.heights[4] = 1.5;
        data.weights[2] = [55, 200, 0, 0];
        let bytes = data.to_bytes();
        assert_eq!(bytes.len(), 12 + 9 * 8);
        assert_eq!(TerrainData::from_bytes(&bytes), Ok(data));
        assert!(TerrainData::from_bytes(&bytes[..20]).is_err());
        assert!(TerrainData::from_bytes(b"nope").is_err());
    }

    #[test]
    fn heights_between_samples_and_none_off_the_terrain() {
        let mut data = TerrainData::flat(3);
        // The middle sample of a terrain 2 m wide is its origin.
        data.heights[4] = 2.0;
        assert_eq!(data.height(2.0, Vec2::ZERO), Some(2.0));
        assert_eq!(data.height(2.0, Vec2::new(0.5, 0.0)), Some(1.0));
        assert_eq!(data.height(2.0, Vec2::new(-1.0, -1.0)), Some(0.0));
        assert_eq!(data.height(2.0, Vec2::new(1.5, 0.0)), None);
    }

    #[test]
    fn builds_a_mesh_facing_up_with_the_layer_colors() {
        let terrain = Terrain::default();
        let mesh = TerrainData::flat(3).mesh(&terrain, &[]);
        assert_eq!(mesh.count_vertices(), 9);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals");
        };
        assert!(normals.iter().all(|normal| *normal == [0.0, 1.0, 0.0]));
        let grass = terrain.layers[0].to_linear();
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colors");
        };
        assert_eq!(colors[0], [grass.red, grass.green, grass.blue, 1.0]);
        // A tint over one corner turns it, and not the far one.
        let black = LocalTint {
            at: Vec2::new(-128.0, -128.0),
            radius: 50.0,
            color: LinearRgba::BLACK,
            strength: 1.0,
        };
        let tinted = TerrainData::flat(3).mesh(&terrain, &[black]);
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) = tinted.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colors");
        };
        assert_eq!(colors[0], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(colors[8], [grass.red, grass.green, grass.blue, 1.0]);
    }
}
