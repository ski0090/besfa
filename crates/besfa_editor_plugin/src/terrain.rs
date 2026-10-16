//! The terrain brushes and writing terrain files: the editor side of
//! `besfa_components::terrain`. Terrain is ground of a given size whose heights and surface layers are
//! painted with brushes. The scene file keeps a terrain's size and layer
//! colors; its heights and where each layer shows are a `.terrain` file of
//! their own in the asset folder, which saving the scene writes back.
//!
//! Games ask the ground's height with [`TerrainHeight`].

use std::collections::{HashMap, HashSet};

use besfa_protocol::{Button, PointerEvent, Report, TerrainBrush, TerrainTool};
use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings},
    prelude::*,
};

pub use besfa_components::terrain::*;

use crate::{
    edit::in_world,
    inspect::{Selected, send_report},
    scene::asset_file,
    scene_view::{EditorCamera, HandleGizmos, SceneView},
};

/// [`TerrainHeight::at`] for each of `points`, from transforms composed
/// afresh, since a command may move a terrain in the frame it asks.
pub(crate) fn heights(world: &mut World, points: &[[f32; 2]]) -> Vec<Option<f32>> {
    let mut query = world.query::<(Entity, &Terrain, &TerrainHandle)>();
    let world = &*world;
    let files = world.resource::<Assets<TerrainData>>();
    let terrains: Vec<_> = query
        .iter(world)
        .filter_map(|(entity, terrain, file)| {
            Some((terrain, files.get(&file.handle)?, in_world(world, entity)))
        })
        .collect();
    points
        .iter()
        .map(|&[x, z]| {
            terrains
                .iter()
                .filter_map(|(terrain, data, transform)| surface(terrain, data, transform, x, z))
                .reduce(f32::max)
        })
        .collect()
}

/// Flat ground for a new terrain under a free name in `terrain/`, held in
/// memory, ready to paint at once; saving the scene writes the file, so an
/// unsaved terrain leaves none behind.
pub(crate) fn new_file(world: &mut World) -> (TerrainFile, TerrainHandle) {
    let taken: HashSet<String> = world
        .query_filtered::<&TerrainFile, Allow<Disabled>>()
        .iter(world)
        .map(|file| file.0.clone())
        .collect();
    let path = (1..)
        .map(|n| format!("terrain/terrain-{n}.terrain"))
        .find(|path| !taken.contains(path) && !asset_file(path).exists())
        .unwrap();
    let handle = world
        .resource_mut::<Assets<TerrainData>>()
        .add(TerrainData::flat(RESOLUTION));
    (TerrainFile(path.clone()), TerrainHandle { path, handle })
}

fn write(path: &str, data: &TerrainData) -> Result<(), String> {
    let file = asset_file(path);
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|error| format!("could not write {path}: {error}"))?;
    }
    std::fs::write(&file, data.to_bytes())
        .map_err(|error| format!("could not write {path}: {error}"))
}

/// A terrain painted since its file was last written.
#[derive(Component)]
pub(crate) struct Unsaved;

/// Writes the files of the terrains painted since they were last saved.
pub(crate) fn save(world: &mut World) -> Result<(), String> {
    let mut query = world.query_filtered::<(Entity, &TerrainHandle), With<Unsaved>>();
    let painted: Vec<(Entity, TerrainHandle)> = query
        .iter(world)
        .map(|(entity, file)| (entity, file.clone()))
        .collect();
    for (entity, TerrainHandle { path, handle }) in painted {
        let files = world.resource::<Assets<TerrainData>>();
        let data = files
            .get(&handle)
            .ok_or_else(|| format!("{path} has not loaded"))?;
        write(&path, data)?;
        world.entity_mut(entity).remove::<Unsaved>();
    }
    Ok(())
}

/// The terrain brushes' strokes, kept for undo, in edit mode.
pub(crate) struct TerrainBrushPlugin;

impl Plugin for TerrainBrushPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Strokes>();
    }
}

/// Paints a whole stroke from a command on `entity`, with `points`, world
/// X and Z, `radius` in meters and `strength` as [`TerrainTool`]s take it;
/// returns the stroke's number.
pub(crate) fn stroke(
    world: &mut World,
    entity: Entity,
    tool: TerrainTool,
    points: &[[f32; 2]],
    radius: f32,
    strength: f32,
    layer: Option<usize>,
) -> Result<u64, String> {
    let (terrain, file, before) = terrain_of(world, entity)?;
    if points.is_empty() {
        return Err("a stroke needs a point".into());
    }
    let transform = in_world(world, entity);
    let brush = Brush::new(
        TerrainBrush {
            tool,
            radius,
            strength,
            layer,
        },
        &terrain,
        &transform,
    )?;
    let points: Vec<Vec2> = points
        .iter()
        .map(|&[x, z]| local(&transform, Vec3::new(x, 0.0, z)))
        .collect();
    let mut painting = Stroke::begin(before, terrain.size, brush, points[0]);
    let mut files = world.resource_mut::<Assets<TerrainData>>();
    let mut data = files.get_mut(file).unwrap();
    for &point in &points {
        painting.to(&mut data, point);
    }
    let changed = painting.finish(&data);
    drop(data);
    world.entity_mut(entity).insert(Unsaved);
    Ok(world.resource_mut::<Strokes>().keep(entity, file, changed))
}

/// Brings all of terrain `entity` to `height`, meters in the world, and
/// covers it with `layer`, or does one of the two, as one stroke; returns
/// its number.
pub(crate) fn fill(
    world: &mut World,
    entity: Entity,
    height: Option<f32>,
    layer: Option<usize>,
) -> Result<u64, String> {
    let (terrain, file, before) = terrain_of(world, entity)?;
    let layers = terrain.layers.len().min(4);
    if height.is_none() && layer.is_none() {
        return Err("fill needs a height, a layer or both".into());
    }
    if layer.is_some_and(|layer| layer >= layers) {
        return Err(format!("the layer is below {layers}"));
    }
    let transform = in_world(world, entity);
    let mut files = world.resource_mut::<Assets<TerrainData>>();
    let mut data = files.get_mut(file).unwrap();
    if let Some(height) = height {
        let local = (height - transform.translation.y) / transform.scale.y;
        data.heights.fill(local);
    }
    if let Some(layer) = layer {
        let mut weights = [0; 4];
        weights[layer] = 255;
        data.weights.fill(weights);
    }
    let last = UVec2::splat(data.resolution - 1);
    let changed = (
        Region::take(&before, UVec2::ZERO, last),
        Region::take(&data, UVec2::ZERO, last),
    );
    drop(data);
    world.entity_mut(entity).insert(Unsaved);
    Ok(world
        .resource_mut::<Strokes>()
        .keep(entity, file, Some(changed)))
}

/// Terrain `entity`, its file, and what the file holds before a stroke
/// changes it; or why there is none.
fn terrain_of(
    world: &World,
    entity: Entity,
) -> Result<(Terrain, AssetId<TerrainData>, TerrainData), String> {
    let id = entity.to_bits();
    let (terrain, file) = world
        .get_entity(entity)
        .ok()
        .and_then(|entity| {
            Some((
                entity.get::<Terrain>()?.clone(),
                entity.get::<TerrainHandle>()?.handle.id(),
            ))
        })
        .ok_or_else(|| format!("{id} is not a terrain"))?;
    let before = world.resource::<Assets<TerrainData>>().get(file).cloned();
    let before = before.ok_or_else(|| format!("the file of terrain {id} has not loaded"))?;
    Ok((terrain, file, before))
}

/// Paints the selected terrain with the scene view's brush. The left
/// button's presses become strokes instead of selections; the other
/// buttons still turn and move the view. A ring shows the brush on the
/// ground under the pointer.
#[allow(clippy::too_many_arguments, reason = "a system's parameters")]
pub(crate) fn paint(
    mut view: ResMut<SceneView>,
    selected: Res<Selected>,
    camera: Query<(&Camera, &GlobalTransform), With<EditorCamera>>,
    terrains: Query<(&Terrain, &TerrainHandle, &GlobalTransform)>,
    mut files: ResMut<Assets<TerrainData>>,
    mut strokes: ResMut<Strokes>,
    mut ray_cast: MeshRayCast,
    mut ring: Gizmos<HandleGizmos>,
    mut commands: Commands,
    // The stroke painting while the left button is down: on which terrain
    // and file.
    mut painting: Local<Option<(Entity, AssetId<TerrainData>, Stroke)>>,
) {
    let target = selected.0.filter(|&entity| terrains.contains(entity));
    let (Some(entity), Some(asked), Ok((camera, eye))) = (target, view.brush, camera.single())
    else {
        view.brush_at = None;
        // Cut short: the brush was put away or the terrain deselected.
        if let Some((entity, file, stroke)) = painting.take() {
            let changed = files.get(file).and_then(|data| stroke.finish(data));
            report_painted(&mut commands, entity, strokes.keep(entity, file, changed));
        }
        return;
    };
    let (terrain, held, global) = terrains.get(entity).unwrap();
    let transform = global.compute_transform();
    let file = held.handle.id();
    let Ok(brush) = Brush::new(asked, terrain, &transform) else {
        return;
    };
    let mut hit = |pointer: Vec2| {
        let ray = camera.viewport_to_world(eye, pointer).ok()?;
        let only = |hit: Entity| hit == entity;
        let settings = MeshRayCastSettings::default().with_filter(&only);
        ray_cast
            .cast_ray(ray, &settings)
            .first()
            .map(|(_, hit)| hit.point)
    };
    let mut kept = Vec::new();
    for event in std::mem::take(&mut view.events) {
        match event {
            PointerEvent::Down {
                x,
                y,
                button: Button::Left,
            } => {
                view.brush_at = hit(Vec2::new(x, y));
                if let (Some(point), Some(before)) = (view.brush_at, files.get(file).cloned()) {
                    let point = local(&transform, point);
                    let mut stroke = Stroke::begin(before, terrain.size, brush, point);
                    stroke.to(&mut files.get_mut(file).unwrap(), point);
                    *painting = Some((entity, file, stroke));
                }
            }
            PointerEvent::Move { x, y } => {
                view.brush_at = hit(Vec2::new(x, y));
                if let (Some(point), Some((_, file, stroke))) = (view.brush_at, &mut *painting)
                    && let Some(mut data) = files.get_mut(*file)
                {
                    stroke.to(&mut data, local(&transform, point));
                }
                // Turning the view still follows the pointer.
                kept.push(event);
            }
            PointerEvent::Up {
                button: Button::Left,
                ..
            } => {
                if let Some((entity, file, stroke)) = painting.take() {
                    let changed = files.get(file).and_then(|data| stroke.finish(data));
                    report_painted(&mut commands, entity, strokes.keep(entity, file, changed));
                }
            }
            event => kept.push(event),
        }
    }
    view.events = kept;
    if let (Some(at), Some(data)) = (view.brush_at, files.get(file)) {
        draw_ring(&mut ring, data, terrain, &transform, at, asked.radius);
    }
}

/// Marks a painted terrain unsaved and tells the editor the stroke's
/// number, for undo.
fn report_painted(commands: &mut Commands, entity: Entity, stroke: u64) {
    commands.entity(entity).insert(Unsaved);
    send_report(&Report::TerrainPainted {
        id: entity.to_bits(),
        stroke,
    });
}

/// The brush's edge around world point `at`, `radius` wide, following the
/// ground a little above it.
fn draw_ring(
    ring: &mut Gizmos<HandleGizmos>,
    data: &TerrainData,
    terrain: &Terrain,
    transform: &Transform,
    at: Vec3,
    radius: f32,
) {
    let points = (0..=48).map(|step| {
        let angle = step as f32 / 48.0 * std::f32::consts::TAU;
        let point = at + Vec3::new(angle.cos(), 0.0, angle.sin()) * radius;
        let height = surface(terrain, data, transform, point.x, point.z).unwrap_or(at.y);
        Vec3::new(point.x, height + 0.05, point.z)
    });
    ring.linestrip(points, Color::srgb(1.0, 0.85, 0.2));
}

/// Puts the ground stroke `number` changed back as it was before it, or as
/// it left it.
pub(crate) fn restore(world: &mut World, number: u64, before: bool) -> Result<(), String> {
    let strokes = world.resource::<Strokes>();
    let painted = strokes
        .kept
        .get(&number)
        .ok_or_else(|| format!("no stroke {number} to restore"))?;
    let (entity, file) = (painted.entity, painted.file);
    let (from, to) = &painted.changed;
    let region = if before { from.clone() } else { to.clone() };
    let mut files = world.resource_mut::<Assets<TerrainData>>();
    let mut data = files
        .get_mut(file)
        .ok_or_else(|| format!("the file stroke {number} painted is gone"))?;
    region.put(&mut data);
    drop(data);
    if let Ok(mut entity) = world.get_entity_mut(entity) {
        entity.insert(Unsaved);
    }
    Ok(())
}

/// Drops what is kept to undo stroke `number`.
pub(crate) fn forget(world: &mut World, number: u64) {
    world.resource_mut::<Strokes>().kept.remove(&number);
}

/// The strokes painted this edit session, by number, for undo and redo.
#[derive(Resource, Default)]
pub(crate) struct Strokes {
    next: u64,
    kept: HashMap<u64, Painted>,
}

impl Strokes {
    /// Numbers a finished stroke and keeps what it `changed` on `file`.
    fn keep(
        &mut self,
        entity: Entity,
        file: AssetId<TerrainData>,
        changed: Option<(Region, Region)>,
    ) -> u64 {
        self.next += 1;
        if let Some(changed) = changed {
            self.kept.insert(
                self.next,
                Painted {
                    entity,
                    file,
                    changed,
                },
            );
        }
        self.next
    }
}

struct Painted {
    entity: Entity,
    file: AssetId<TerrainData>,
    /// The samples the stroke changed, before and after it.
    changed: (Region, Region),
}

/// A rectangle of samples, from `from` to `to` inclusive.
#[derive(Clone, Debug, PartialEq)]
struct Region {
    from: UVec2,
    to: UVec2,
    heights: Vec<f32>,
    weights: Vec<[u8; 4]>,
}

impl Region {
    fn take(data: &TerrainData, from: UVec2, to: UVec2) -> Self {
        let mut region = Self {
            from,
            to,
            heights: Vec::new(),
            weights: Vec::new(),
        };
        for z in from.y..=to.y {
            for x in from.x..=to.x {
                region.heights.push(data.heights[data.index(x, z)]);
                region.weights.push(data.weights[data.index(x, z)]);
            }
        }
        region
    }

    fn put(&self, data: &mut TerrainData) {
        let mut next = 0;
        for z in self.from.y..=self.to.y {
            for x in self.from.x..=self.to.x {
                let index = data.index(x, z);
                data.heights[index] = self.heights[next];
                data.weights[index] = self.weights[next];
                next += 1;
            }
        }
    }
}

/// What a stroke paints with, in the terrain's own space.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Brush {
    pub(crate) tool: TerrainTool,
    pub(crate) radius: f32,
    pub(crate) strength: f32,
    pub(crate) layer: usize,
}

impl Brush {
    /// `asked`, in meters in the world, on `terrain` placed by `transform`;
    /// or why it cannot paint.
    fn new(asked: TerrainBrush, terrain: &Terrain, transform: &Transform) -> Result<Self, String> {
        let TerrainBrush {
            tool,
            radius,
            strength,
            layer,
        } = asked;
        if !(radius > 0.0 && radius.is_finite() && strength.is_finite()) {
            return Err(format!(
                "a brush {radius} m wide with strength {strength} paints nothing"
            ));
        }
        let layers = terrain.layers.len().min(4);
        let layer = match (tool, layer) {
            (TerrainTool::Paint, Some(layer)) if layer < layers => layer,
            (TerrainTool::Paint, _) => return Err(format!("paint needs a layer below {layers}")),
            _ => 0,
        };
        Ok(Self {
            tool,
            radius: radius / transform.scale.x,
            strength: match tool {
                TerrainTool::Raise | TerrainTool::Lower => strength / transform.scale.y,
                _ => strength,
            },
            layer,
        })
    }
}

/// A stroke being painted: the ground as it was when it began, and how
/// far the brush reached each sample so far. Each sample takes the brush
/// where it reached most, worked out from the ground before the stroke,
/// so going over a place again in one stroke changes nothing more.
pub(crate) struct Stroke {
    before: TerrainData,
    size: f32,
    brush: Brush,
    reach: Vec<f32>,
    /// Flatten's height: the ground's where the stroke began.
    level: f32,
    /// Where the brush is.
    last: Vec2,
    /// The first and last samples changed.
    touched: Option<(UVec2, UVec2)>,
}

impl Stroke {
    pub(crate) fn begin(before: TerrainData, size: f32, brush: Brush, start: Vec2) -> Self {
        Self {
            level: before.height(size, start).unwrap_or(0.0),
            reach: vec![0.0; before.heights.len()],
            before,
            size,
            brush,
            last: start,
            touched: None,
        }
    }

    /// Moves the brush to `to`, in the terrain's space, painting the line
    /// there, and writes what changed into `data`. Each sample near the
    /// line takes the brush as it reaches it from the nearest point on the
    /// line, so a line is even along its length rather than rippled by
    /// separate dabs.
    pub(crate) fn to(&mut self, data: &mut TerrainData, to: Vec2) {
        let from = std::mem::replace(&mut self.last, to);
        let (size, radius) = (self.size, self.brush.radius);
        let step = self.before.step(size);
        let last = self.before.resolution - 1;
        let sample = |at: f32| ((at + size / 2.0) / step).clamp(0.0, last as f32);
        let (low, high) = (from.min(to) - radius, from.max(to) + radius);
        let first = UVec2::new(sample(low.x).floor() as u32, sample(low.y).floor() as u32);
        let end = UVec2::new(sample(high.x).ceil() as u32, sample(high.y).ceil() as u32);
        let line = to - from;
        for z in first.y..=end.y {
            for x in first.x..=end.x {
                let at = self.before.position(size, x, z);
                let along = if line.length_squared() > 0.0 {
                    ((at - from).dot(line) / line.length_squared()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let distance = at.distance(from + line * along);
                if distance >= radius {
                    continue;
                }
                let t = 1.0 - distance / radius;
                let reach = t * t * (3.0 - 2.0 * t);
                let index = self.before.index(x, z);
                if reach <= self.reach[index] {
                    continue;
                }
                self.reach[index] = reach;
                self.paint(data, x, z, reach);
                let sample = UVec2::new(x, z);
                self.touched = Some(match self.touched {
                    Some((low, high)) => (low.min(sample), high.max(sample)),
                    None => (sample, sample),
                });
            }
        }
    }

    /// Sample `x`, `z` as the brush leaves it where it reached `reach`.
    fn paint(&self, data: &mut TerrainData, x: u32, z: u32, reach: f32) {
        let index = self.before.index(x, z);
        let height = self.before.heights[index];
        let amount = (self.brush.strength * reach).clamp(0.0, 1.0);
        match self.brush.tool {
            TerrainTool::Raise => data.heights[index] = height + self.brush.strength * reach,
            TerrainTool::Lower => data.heights[index] = height - self.brush.strength * reach,
            TerrainTool::Flatten => data.heights[index] = height + (self.level - height) * amount,
            TerrainTool::Smooth => {
                data.heights[index] = height + (self.blurred(x, z) - height) * amount;
            }
            TerrainTool::Paint => {
                data.weights[index] = toward(self.before.weights[index], self.brush.layer, amount);
            }
        }
    }

    /// The mean height before the stroke around sample `x`, `z`, over a
    /// window a quarter of the brush wide.
    fn blurred(&self, x: u32, z: u32) -> f32 {
        let reach = ((self.brush.radius / 4.0) / self.before.step(self.size))
            .round()
            .max(1.0) as u32;
        let last = self.before.resolution - 1;
        let (mut sum, mut count) = (0.0, 0.0);
        for z in z.saturating_sub(reach)..=(z + reach).min(last) {
            for x in x.saturating_sub(reach)..=(x + reach).min(last) {
                sum += self.before.heights[self.before.index(x, z)];
                count += 1.0;
            }
        }
        sum / count
    }

    /// The samples the stroke changed, as they were and as they are in
    /// `data`; none if it touched nothing.
    fn finish(self, data: &TerrainData) -> Option<(Region, Region)> {
        let (from, to) = self.touched?;
        Some((
            Region::take(&self.before, from, to),
            Region::take(data, from, to),
        ))
    }
}

/// `weights` moved `amount`, 0 to 1, of the way to all `layer`, still
/// summing to 255.
fn toward(weights: [u8; 4], layer: usize, amount: f32) -> [u8; 4] {
    let shares = std::array::from_fn::<f32, 4, _>(|other| {
        let share = weights[other] as f32 / 255.0;
        if other == layer {
            share + (1.0 - share) * amount
        } else {
            share * (1.0 - amount)
        }
    });
    let mut bytes = shares.map(|share| (share * 255.0).round() as u8);
    // Rounding may leave the sum off by a little; the largest share takes it.
    let sum: i32 = bytes.iter().map(|&byte| byte as i32).sum();
    let largest = (0..4)
        .max_by(|&a, &b| shares[a].total_cmp(&shares[b]))
        .unwrap();
    bytes[largest] = (bytes[largest] as i32 + 255 - sum).clamp(0, 255) as u8;
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brush(tool: TerrainTool, radius: f32, strength: f32) -> Brush {
        Brush {
            tool,
            radius,
            strength,
            layer: 1,
        }
    }

    /// Paints `points` on flat ground 10 m wide with a sample a meter.
    fn painted(
        data: &TerrainData,
        brush: Brush,
        points: &[Vec2],
    ) -> (TerrainData, Option<(Region, Region)>) {
        let mut after = data.clone();
        let mut stroke = Stroke::begin(data.clone(), 10.0, brush, points[0]);
        for &point in points {
            stroke.to(&mut after, point);
        }
        let changed = stroke.finish(&after);
        (after, changed)
    }

    #[test]
    fn raises_a_ridge_of_even_height_along_a_stroke() {
        let flat = TerrainData::flat(11);
        let (after, changed) = painted(
            &flat,
            brush(TerrainTool::Raise, 2.0, 3.0),
            &[Vec2::new(-3.0, 0.0), Vec2::new(3.0, 0.0)],
        );
        // Full strength under the line, however many dabs crossed it.
        for x in [-3.0, 0.0, 3.0] {
            assert!((after.height(10.0, Vec2::new(x, 0.0)).unwrap() - 3.0).abs() < 1e-4);
        }
        assert_eq!(
            after.height(10.0, Vec2::new(0.0, 2.0)),
            Some(0.0),
            "nothing at the brush's edge"
        );
        // Off the samples too, the ridge is even along its length.
        let (askew, _) = painted(
            &flat,
            brush(TerrainTool::Raise, 2.0, 3.0),
            &[Vec2::new(-3.1, 0.3), Vec2::new(3.1, 0.3)],
        );
        let row: Vec<f32> = (2..=8).map(|x| askew.heights[flat.index(x, 5)]).collect();
        assert!(
            row.iter().all(|&height| (height - row[0]).abs() < 1e-5),
            "{row:?}"
        );
        let (from, to) = changed.unwrap();
        assert!(from.heights.iter().all(|&height| height == 0.0));
        let mut undone = after.clone();
        from.put(&mut undone);
        assert_eq!(undone, flat);
        to.put(&mut undone);
        assert_eq!(undone, after);
    }

    #[test]
    fn flattens_toward_where_the_stroke_began_and_smooths_a_bump() {
        let mut ground = TerrainData::flat(11);
        let middle = ground.index(5, 5);
        ground.heights[middle] = 4.0;
        let (flat, _) = painted(
            &ground,
            brush(TerrainTool::Flatten, 3.0, 1.0),
            &[Vec2::new(-3.0, -3.0), Vec2::ZERO],
        );
        assert!(flat.heights[middle].abs() < 1e-4);

        let (smooth, _) = painted(&ground, brush(TerrainTool::Smooth, 4.0, 1.0), &[Vec2::ZERO]);
        assert!(smooth.heights[middle] < 4.0 && smooth.heights[middle] > 0.0);
    }

    #[test]
    fn paints_a_layer_keeping_the_weights_whole() {
        let ground = TerrainData::flat(11);
        let (after, _) = painted(&ground, brush(TerrainTool::Paint, 3.0, 1.0), &[Vec2::ZERO]);
        assert_eq!(after.weights[ground.index(5, 5)], [0, 255, 0, 0]);
        for weights in &after.weights {
            assert_eq!(weights.iter().map(|&w| w as u32).sum::<u32>(), 255);
        }
        assert_eq!(toward([255, 0, 0, 0], 2, 0.5), [128, 0, 127, 0]);
        assert_eq!(toward([0, 0, 0, 255], 3, 1.0), [0, 0, 0, 255]);
    }
}
