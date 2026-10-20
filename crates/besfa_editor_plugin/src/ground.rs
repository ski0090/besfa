//! Putting things on the ground: placing them where the scene view points,
//! with a see-through ghost while an asset is dragged there, dropping them
//! onto the surface below them, and scattering many over an area, which can
//! be erased again without touching what was placed by hand.

use std::collections::HashSet;

use besfa_protocol::{Button, Move, PointerEvent, Report, ScatterBrush, ScatterModel};
use bevy::{
    camera::visibility::RenderLayers,
    ecs::{query::QueryFilter, system::SystemState},
    picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings, RayCastVisibility},
    prelude::*,
    reflect::{TypeRegistry, serde::TypedReflectSerializer},
    world_serialization::{DynamicWorldRoot, WorldAssetRoot},
};
use serde_json::Value;

pub use besfa_components::ground::{ScatterGroup, Scattered};

use crate::{
    edit::{self, in_world, is_scene_entity, relative, subtree},
    inspect::send_report,
    preview::{Isolated, bounds},
    scene::{MeshShape, SceneEntity},
    scene_view::{EditorCamera, EditorOnly, SceneView, plane_point},
    terrain::{self, Terrain},
};

/// The most things one scatter places.
const MAX_SCATTER: u32 = 10_000;

/// Where a ray looking for the ground below a scattered thing starts.
const SKY: f32 = 10_000.0;

/// Finds the surface under points: terrain from its data, anything else by
/// a ray straight down at the meshes not left out.
struct Ground {
    rays: SystemState<MeshRayCast<'static, 'static>>,
    /// Never a surface: the editor's own things, terrains, which are asked
    /// by their data instead, and whatever else the caller leaves out.
    skip: HashSet<Entity>,
}

impl Ground {
    fn new(world: &mut World, mut skip: HashSet<Entity>) -> Self {
        skip.extend(under::<Or<(With<EditorOnly>, With<Terrain>)>>(world));
        Self {
            rays: SystemState::new(world),
            skip,
        }
    }

    /// The top of the highest mesh at world `x`, `z` below `top`, and its
    /// normal there.
    fn mesh(&mut self, world: &mut World, x: f32, z: f32, top: f32) -> Option<(f32, Vec3)> {
        let mut ray_cast = self.rays.get_mut(world).ok()?;
        let skip = &self.skip;
        let keep = |entity: Entity| !skip.contains(&entity);
        // Visible rather than in view: the ground may be far off screen.
        let settings = MeshRayCastSettings::default()
            .with_visibility(RayCastVisibility::Visible)
            .with_filter(&keep);
        let ray = Ray3d::new(Vec3::new(x, top, z), Dir3::NEG_Y);
        ray_cast
            .cast_ray(ray, &settings)
            .first()
            .map(|(_, hit)| (hit.point.y, hit.normal.normalize_or(Vec3::Y)))
    }
}

/// The entities `F` matches and everything under them.
fn under<F: QueryFilter>(world: &mut World) -> HashSet<Entity> {
    let mut query = world.query_filtered::<Entity, F>();
    let world = &*world;
    query
        .iter(world)
        .flat_map(|entity| subtree(world, entity))
        .collect()
}

/// Where the scene view shows a surface at `pointer`, in viewport pixels:
/// the nearest mesh there, else the ground at height 0, else, looking above
/// the horizon, the view's focus. None without the scene view.
pub(crate) fn surface_at(world: &mut World, pointer: Vec2) -> Option<Vec3> {
    let (camera, eye, focus) = world
        .query::<(&Camera, &GlobalTransform, &EditorCamera)>()
        .single(world)
        .ok()
        .map(|(camera, eye, view)| (camera.clone(), *eye, view.focus))?;
    let ray = camera.viewport_to_world(&eye, pointer).ok()?;
    let skip = under::<With<EditorOnly>>(world);
    let keep = |entity: Entity| !skip.contains(&entity);
    let settings = MeshRayCastSettings::default().with_filter(&keep);
    let mut rays = SystemState::<MeshRayCast>::new(world);
    let hit = rays
        .get_mut(world)
        .ok()?
        .cast_ray(ray, &settings)
        .first()
        .map(|(_, hit)| hit.point);
    let ground = plane_point(Vec3::ZERO, Vec3::Y, ray)
        .filter(|point| (point - ray.origin).dot(*ray.direction) > 0.0);
    Some(hit.or(ground).unwrap_or(focus))
}

/// Puts the new `entity` standing on the surface the scene view shows at
/// `pointer`: a shape the plugin builds, which sits on its middle, rests
/// on it, and anything else stands on its origin.
pub(crate) fn place_at(world: &mut World, entity: Entity, pointer: Vec2) {
    let Some(point) = surface_at(world, pointer) else {
        return;
    };
    let lift = match world.get::<MeshShape>(entity) {
        Some(MeshShape::Cuboid { size }) => size.y / 2.0,
        Some(MeshShape::Sphere { radius }) => *radius,
        _ => 0.0,
    };
    if let Some(mut transform) = world.get_mut::<Transform>(entity) {
        transform.translation = point + Vec3::Y * lift;
    }
}

/// The asset an `instantiate` with a pointer would place, shown see-through
/// where it would go while the editor drags it over the scene view.
#[derive(Component)]
pub(crate) struct Ghost(String);

/// A ghost's part whose material was made see-through.
#[derive(Component)]
pub(crate) struct SeeThrough;

/// Shows the asset at `path` as the ghost at `pointer`, loading it unless it
/// is the one shown; without a path the ghost goes.
pub(crate) fn ghost(world: &mut World, path: Option<&str>, pointer: Vec2) -> Result<(), String> {
    let shown = world
        .query::<(Entity, &Ghost)>()
        .iter(world)
        .map(|(entity, ghost)| (entity, ghost.0.clone()))
        .next();
    let entity = match (shown, path) {
        (Some((entity, shown)), Some(path)) if shown == path => entity,
        (shown, path) => {
            if let Some((entity, _)) = shown {
                world.despawn(entity);
            }
            let Some(path) = path else {
                return Ok(());
            };
            let entity = edit::show_asset(world, path)?;
            world.entity_mut(entity).insert((
                Ghost(path.into()),
                EditorOnly,
                // In the scene view, but out of the Hierarchy and the rays.
                Isolated,
                RenderLayers::default(),
            ));
            entity
        }
    };
    if let Some(point) = surface_at(world, pointer) {
        world.get_mut::<Transform>(entity).unwrap().translation = point;
    }
    Ok(())
}

/// Makes the ghost's parts see-through as they spawn. Only standard
/// materials fade; a part with a material of the game's own shows solid.
pub(crate) fn see_through(
    mut commands: Commands,
    ghosts: Query<Entity, With<Ghost>>,
    children: Query<&Children>,
    mut parts: Query<&mut MeshMaterial3d<StandardMaterial>, Without<SeeThrough>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for ghost in &ghosts {
        for part in children.iter_descendants(ghost) {
            let Ok(mut material) = parts.get_mut(part) else {
                continue;
            };
            let Some(mut faded) = materials.get(&material.0).cloned() else {
                continue;
            };
            faded.base_color = faded.base_color.with_alpha(faded.base_color.alpha() * 0.5);
            faded.alpha_mode = AlphaMode::Blend;
            material.0 = materials.add(faded);
            commands.entity(part).insert(SeeThrough);
        }
    }
}

/// The highest terrain's height at world `x`, `z`, and its normal there.
fn terrain_at(world: &mut World, x: f32, z: f32) -> Option<(f32, Vec3)> {
    const STEP: f32 = 0.5;
    let around = [
        [x, z],
        [x - STEP, z],
        [x + STEP, z],
        [x, z - STEP],
        [x, z + STEP],
    ];
    let heights = terrain::heights(world, &around);
    let height = heights[0]?;
    let at = |index: usize| heights[index].unwrap_or(height);
    let normal = Vec3::new(at(1) - at(2), 2.0 * STEP, at(3) - at(4)).normalize();
    Some((height, normal))
}

/// Drops each of `ids` onto the surface below it, or each thing in it for
/// a group; returns what moved and why the rest did not.
pub(crate) fn drop_to_surface(world: &mut World, ids: &[u64]) -> (Vec<Move>, Vec<String>) {
    let (mut moves, mut failures) = (Vec::new(), Vec::new());
    let mut things = Vec::new();
    for &id in ids {
        match Entity::try_from_bits(id).filter(|&entity| is_scene_entity(world, entity)) {
            Some(entity) => things.extend(things_in(world, entity)),
            None => failures.push(format!("{id} is not a scene entity.")),
        }
    }
    for entity in things {
        match drop_one(world, entity) {
            Some(Some(change)) => moves.push(change),
            Some(None) => {}
            None => {
                let name = world
                    .get::<Name>(entity)
                    .map_or_else(|| entity.to_bits().to_string(), ToString::to_string);
                failures.push(format!("No surface below {name}."));
            }
        }
    }
    (moves, failures)
}

/// `entity`, or for a group, the things in it: a group has no mesh or
/// asset of its own and holds scene entities.
fn things_in(world: &World, entity: Entity) -> Vec<Entity> {
    let shows = world.get::<Mesh3d>(entity).is_some()
        || world.get::<WorldAssetRoot>(entity).is_some()
        || world.get::<DynamicWorldRoot>(entity).is_some();
    let held: Vec<Entity> = world
        .get::<Children>(entity)
        .map(|children| {
            children
                .iter()
                .filter(|&child| is_scene_entity(world, child))
                .collect()
        })
        .unwrap_or_default();
    if shows || held.is_empty() {
        vec![entity]
    } else {
        held.into_iter()
            .flat_map(|child| things_in(world, child))
            .collect()
    }
}

/// Moves `entity` so its bottom rests on the surface below it; none when
/// there is no surface, and no move when it already rests there.
fn drop_one(world: &mut World, entity: Entity) -> Option<Option<Move>> {
    let placed = in_world(world, entity);
    // How far its origin is above its bottom, from its meshes' bounds.
    let lift = world
        .get::<GlobalTransform>(entity)
        .zip(bounds(world, &subtree(world, entity)))
        .map_or(0.0, |(global, (low, _))| global.translation().y - low.y);
    let bottom = placed.translation.y - lift;
    let (x, z) = (placed.translation.x, placed.translation.z);
    let terrain = terrain_at(world, x, z).map(|(height, _)| height);
    // From above its bottom, or above the ground that buried it.
    let top = bottom.max(terrain.unwrap_or(f32::NEG_INFINITY)) + 0.05;
    let mut ground = Ground::new(world, subtree(world, entity).into_iter().collect());
    let mesh = ground.mesh(world, x, z, top).map(|(height, _)| height);
    let surface = match (terrain, mesh) {
        (Some(terrain), Some(mesh)) => terrain.max(mesh),
        (one, other) => one.or(other)?,
    };
    let mut moved = placed;
    moved.translation.y = surface + lift;
    let local = match world.get::<ChildOf>(entity).map(ChildOf::parent) {
        Some(parent) => relative(moved, in_world(world, parent)),
        None => moved,
    };
    let before = *world.get::<Transform>(entity)?;
    if before == local {
        return Some(None);
    }
    *world.get_mut::<Transform>(entity)? = local;
    let registry = world.resource::<AppTypeRegistry>().read();
    Some(Some(Move {
        id: entity.to_bits(),
        before: transform_value(&registry, &before),
        after: transform_value(&registry, &local),
    }))
}

/// `transform` as `set` takes it.
pub(crate) fn transform_value(registry: &TypeRegistry, transform: &Transform) -> Value {
    serde_json::to_value(TypedReflectSerializer::new(transform, registry)).unwrap_or(Value::Null)
}

/// Where and how many things `scatter` spreads.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Spread {
    pub(crate) center: Vec2,
    pub(crate) radius: f32,
    pub(crate) count: u32,
    pub(crate) spacing: f32,
    pub(crate) scale: [f32; 2],
    pub(crate) rotate: bool,
    pub(crate) align: bool,
    pub(crate) seed: u64,
}

/// Places what [`plan`] decides from `spread`'s seed on the ground, each
/// under the scene's Scatter group; returns them.
pub(crate) fn scatter(
    world: &mut World,
    models: &[ScatterModel],
    spread: Spread,
) -> Result<Vec<u64>, String> {
    if models.is_empty() {
        return Err("scatter needs a model".into());
    }
    for model in models {
        let path = &model.path;
        if !(path.ends_with(".glb") || path.ends_with(".gltf") || path.ends_with(".scn.ron")) {
            return Err(format!("{path} is not a glTF model or a scene file"));
        }
        if !(model.weight >= 0.0 && model.weight.is_finite()) {
            return Err(format!("{path}'s weight {} is not 0 or more", model.weight));
        }
    }
    let Spread {
        radius,
        count,
        spacing,
        scale: [small, big],
        ..
    } = spread;
    if models.iter().all(|model| model.weight == 0.0) {
        return Err("every model's weight is 0".into());
    }
    if !(radius > 0.0 && radius.is_finite() && spacing >= 0.0 && spacing.is_finite()) {
        return Err(format!(
            "a circle {radius} m wide with things {spacing} m apart holds nothing"
        ));
    }
    if count > MAX_SCATTER {
        return Err(format!("{count} things is more than {MAX_SCATTER}"));
    }
    if !(small > 0.0 && small <= big && big.is_finite()) {
        return Err(format!("sizes from {small} to {big} are not sizes"));
    }
    let scattered = scattered(world);
    let taken: Vec<Vec2> = scattered.iter().map(|&(_, at)| at).collect();
    let weights: Vec<f32> = models.iter().map(|model| model.weight).collect();
    let planned = plan(&weights, spread, &taken);
    let group = scatter_group(world);
    let skip = under::<With<Scattered>>(world);
    let mut ground = Ground::new(world, skip);
    let mut added = Vec::new();
    for thing in planned {
        let (x, z) = (thing.at.x, thing.at.y);
        let (height, normal) = terrain_at(world, x, z)
            .or_else(|| ground.mesh(world, x, z, SKY))
            .unwrap_or((0.0, Vec3::Y));
        let entity = edit::instantiate(world, &models[thing.model].path)?;
        let turn = Quat::from_rotation_y(thing.yaw.to_radians());
        let rotation = if spread.align {
            Quat::from_rotation_arc(Vec3::Y, normal) * turn
        } else {
            turn
        };
        world.entity_mut(entity).insert((
            Scattered,
            Transform {
                translation: Vec3::new(x, height, z),
                rotation,
                scale: Vec3::splat(thing.scale),
            },
        ));
        edit::set_parent(world, entity, Some(group))?;
        added.push(entity.to_bits());
    }
    Ok(added)
}

/// Deletes the scattered things within `radius` of `center`, world X and
/// Z, the way the editor deletes, so undoing restores them; returns them.
pub(crate) fn erase(world: &mut World, center: Vec2, radius: f32) -> Vec<u64> {
    let mut removed = Vec::new();
    for (entity, at) in scattered(world) {
        if at.distance(center) <= radius && edit::delete(world, entity).is_ok() {
            removed.push(entity.to_bits());
        }
    }
    removed
}

/// The scattered things in the scene and where they stand, world X and Z.
fn scattered(world: &mut World) -> Vec<(Entity, Vec2)> {
    let things: Vec<Entity> = world
        .query_filtered::<Entity, With<Scattered>>()
        .iter(world)
        .collect();
    things
        .into_iter()
        .map(|entity| (entity, in_world(world, entity).translation.xz()))
        .collect()
}

/// The scene's Scatter group, made at the origin the first time.
fn scatter_group(world: &mut World) -> Entity {
    let found = world
        .query_filtered::<Entity, (With<ScatterGroup>, With<SceneEntity>)>()
        .iter(world)
        .next();
    found.unwrap_or_else(|| {
        world
            .spawn((
                SceneEntity,
                ScatterGroup,
                Name::new("Scatter"),
                Transform::default(),
                Visibility::default(),
            ))
            .id()
    })
}

/// The scene view's scatter stroke while the left button is down: where
/// the last dab was, and what the stroke added and removed so far.
#[derive(Default)]
pub(crate) struct Scattering {
    stroke: Option<(Vec2, Vec<u64>, Vec<u64>)>,
    /// Numbers each dab, for its seed.
    dabs: u64,
}

/// Scatters with the scene view's brush: the left button's presses and
/// drags place, or erase, under the brush instead of selecting, a dab at a
/// time; the other buttons still turn and move the view. Each stroke is
/// reported once, when it ends.
pub(crate) fn scatter_paint(
    world: &mut World,
    mut painting: Local<Scattering>,
    mut rays: Local<SystemState<MeshRayCast<'static, 'static>>>,
) {
    let brush = world
        .get_resource::<SceneView>()
        .and_then(|view| view.scatter.clone());
    let Some(brush) = brush else {
        // Cut short: the brush was put away mid-stroke.
        finish(&mut painting);
        return;
    };
    let mut view = world.resource_mut::<SceneView>();
    let events = std::mem::take(&mut view.events);
    if events.is_empty() {
        return;
    }
    let camera = world
        .query_filtered::<(&Camera, &GlobalTransform), With<EditorCamera>>()
        .single(world)
        .ok()
        .map(|(camera, eye)| (camera.clone(), *eye));
    let Some((camera, eye)) = camera else {
        world.resource_mut::<SceneView>().events = events;
        return;
    };
    // The ground, not what is being scattered on it.
    let skip = under::<Or<(With<Scattered>, With<EditorOnly>)>>(world);
    let mut hit = |world: &mut World, pointer: Vec2| {
        let ray = camera.viewport_to_world(&eye, pointer).ok()?;
        let keep = |entity: Entity| !skip.contains(&entity);
        let settings = MeshRayCastSettings::default().with_filter(&keep);
        let mut ray_cast = rays.get_mut(world).ok()?;
        ray_cast
            .cast_ray(ray, &settings)
            .first()
            .map(|(_, hit)| hit.point)
    };
    let mut kept = Vec::new();
    let mut at = None;
    for event in events {
        match event {
            PointerEvent::Down {
                x,
                y,
                button: Button::Left,
            } => {
                at = hit(world, Vec2::new(x, y));
                if let Some(point) = at {
                    painting.stroke = Some((point.xz(), Vec::new(), Vec::new()));
                    dab(world, &brush, point.xz(), &mut painting);
                }
            }
            PointerEvent::Move { x, y } => {
                at = hit(world, Vec2::new(x, y));
                let last = painting.stroke.as_ref().map(|(last, ..)| *last);
                if let (Some(point), Some(last)) = (at, last)
                    && point.xz().distance(last) >= brush.radius / 2.0
                {
                    dab(world, &brush, point.xz(), &mut painting);
                }
                // Turning the view still follows the pointer.
                kept.push(event);
            }
            PointerEvent::Up {
                button: Button::Left,
                ..
            } => finish(&mut painting),
            event => kept.push(event),
        }
    }
    let mut view = world.resource_mut::<SceneView>();
    view.events = kept;
    if at.is_some() {
        view.brush_at = at;
    }
}

/// One dab of the brush at `center`, world X and Z: erases there, or adds
/// as many as the density misses under the brush, so dabs that overlap
/// fill in rather than pile up.
fn dab(world: &mut World, brush: &ScatterBrush, center: Vec2, painting: &mut Scattering) {
    let Some((last, added, removed)) = painting.stroke.as_mut() else {
        return;
    };
    *last = center;
    if brush.erase {
        removed.extend(erase(world, center, brush.radius));
        return;
    }
    let area = std::f32::consts::PI * brush.radius * brush.radius;
    let wanted = (brush.density.max(0.0) * area / 100.0).ceil() as u32;
    let present = scattered(world)
        .iter()
        .filter(|(_, at)| at.distance(center) <= brush.radius)
        .count() as u32;
    let count = wanted.saturating_sub(present);
    if count == 0 {
        return;
    }
    painting.dabs += 1;
    let spread = Spread {
        center,
        radius: brush.radius,
        count,
        spacing: brush.spacing,
        scale: brush.scale,
        rotate: brush.rotate,
        align: brush.align,
        seed: painting.dabs,
    };
    match scatter(world, &brush.models, spread) {
        Ok(placed) => added.extend(placed),
        Err(error) => warn!("Could not scatter: {error}"),
    }
}

/// Ends the stroke, reporting what it added and removed, once.
fn finish(painting: &mut Scattering) {
    if let Some((_, added, removed)) = painting.stroke.take()
        && !(added.is_empty() && removed.is_empty())
    {
        send_report(&Report::Scattered { added, removed });
    }
}

/// Where scatter puts one thing, decided from the seed alone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Planned {
    /// World X and Z.
    pub(crate) at: Vec2,
    pub(crate) model: usize,
    pub(crate) scale: f32,
    /// Degrees.
    pub(crate) yaw: f32,
}

/// Up to `spread.count` places in its circle at least `spacing` from each
/// other and from `taken`, each with a model picked by `weights`, a size
/// and a turn. Every try draws the same five numbers in the same order,
/// whether it is kept or not, so the result depends only on the seed, the
/// circle and the places taken, not on the order they are listed in.
pub(crate) fn plan(weights: &[f32], spread: Spread, taken: &[Vec2]) -> Vec<Planned> {
    let Spread {
        center,
        radius,
        count,
        spacing,
        scale: [small, big],
        rotate,
        ..
    } = spread;
    let total: f32 = weights.iter().sum();
    let mut random = Random(spread.seed);
    let mut planned: Vec<Planned> = Vec::new();
    for _ in 0..count.saturating_mul(30) {
        if planned.len() == count as usize {
            break;
        }
        let [distance, angle, pick, size, turn] = std::array::from_fn(|_| random.unit());
        // The square root spreads them evenly over the circle's area.
        let at =
            center + Vec2::from_angle(angle * std::f32::consts::TAU) * radius * distance.sqrt();
        let near = |other: Vec2| other.distance(at) < spacing;
        if taken.iter().copied().any(near) || planned.iter().any(|thing| near(thing.at)) {
            continue;
        }
        planned.push(Planned {
            at,
            model: weighted(weights, pick * total),
            scale: small + (big - small) * size,
            yaw: if rotate { turn * 360.0 } else { 0.0 },
        });
    }
    planned
}

/// The index `pick`, 0 to the weights' sum, falls on; a weight of 0 is
/// never picked.
fn weighted(weights: &[f32], mut pick: f32) -> usize {
    for (index, &weight) in weights.iter().enumerate() {
        if pick < weight {
            return index;
        }
        pick -= weight;
    }
    // Rounding at the very top: the last that can be picked.
    weights
        .iter()
        .rposition(|&weight| weight > 0.0)
        .unwrap_or(0)
}

/// SplitMix64: numbers that depend on nothing but the seed.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// 0 to 1, never 1.
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spread(count: u32, spacing: f32, seed: u64) -> Spread {
        Spread {
            center: Vec2::new(10.0, -5.0),
            radius: 20.0,
            count,
            spacing,
            scale: [0.5, 2.0],
            rotate: true,
            align: false,
            seed,
        }
    }

    #[test]
    fn draws_the_same_numbers_from_the_same_seed() {
        let mut a = Random(7);
        let mut b = Random(7);
        let first: Vec<u64> = (0..4).map(|_| a.next()).collect();
        assert_eq!(first, (0..4).map(|_| b.next()).collect::<Vec<_>>());
        assert_ne!(first[0], Random(8).next());
        assert!((0..1000).all(|_| (0.0..1.0).contains(&a.unit())));
    }

    #[test]
    fn plans_the_same_from_the_same_seed_within_the_circle_and_apart() {
        let plan_with = |seed| plan(&[1.0, 3.0], spread(40, 2.0, seed), &[]);
        let planned = plan_with(7);
        assert_eq!(planned, plan_with(7));
        assert_ne!(planned, plan_with(8));
        assert_eq!(planned.len(), 40);
        for (index, thing) in planned.iter().enumerate() {
            assert!(thing.at.distance(Vec2::new(10.0, -5.0)) <= 20.0);
            assert!((0.5..=2.0).contains(&thing.scale));
            assert!((0.0..360.0).contains(&thing.yaw));
            for other in &planned[index + 1..] {
                assert!(thing.at.distance(other.at) >= 2.0);
            }
        }
        let common = planned.iter().filter(|thing| thing.model == 1).count();
        assert!(common > 20, "three times as likely: {common} of 40");
    }

    #[test]
    fn keeps_away_from_what_is_taken_and_stops_when_full() {
        let taken = [Vec2::new(10.0, -5.0)];
        let planned = plan(&[1.0], spread(10, 3.0, 1), &taken);
        assert!(
            planned
                .iter()
                .all(|thing| thing.at.distance(taken[0]) >= 3.0)
        );
        // A circle 20 m wide fits far fewer than 1000 things 5 m apart.
        let full = plan(&[1.0], spread(1000, 5.0, 1), &[]);
        assert!(!full.is_empty() && full.len() < 1000);
    }

    #[test]
    fn never_picks_a_weight_of_zero() {
        assert_eq!(weighted(&[0.0, 2.0, 1.0], 0.0), 1);
        assert_eq!(weighted(&[0.0, 2.0, 1.0], 2.5), 2);
        assert_eq!(weighted(&[1.0, 0.0], 1.0), 0, "rounding at the top");
        let planned = plan(&[0.0, 1.0], spread(30, 0.0, 3), &[]);
        assert!(planned.iter().all(|thing| thing.model == 1));
    }
}
