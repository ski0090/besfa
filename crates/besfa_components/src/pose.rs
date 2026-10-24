//! Turning and moving a model's bones over the pose its animation gives:
//! an arm raised as a blow winds up, a chest leaning back from a hit, legs
//! bent to sit, with no clip for each. A game puts [`PoseOffsets`] on the
//! model's root and sets its layers each frame; the bones are found by
//! name, and a bone no clip moves, as the armature, does not drift, as it
//! is given back its own pose once nothing offsets it.
//!
//! A creature with no walking clip, as a four-legged prop, can be given a
//! [`Gait`] instead: its parts swing to and fro as it goes, step by step
//! with the ground it covers.

use std::collections::{BTreeMap, HashMap};

use std::f32::consts::TAU;

use bevy::{app::AnimationSystems, prelude::*};

/// Applies [`PoseOffsets`] after the animations and before the meshes
/// follow the bones.
pub struct PosePlugin;

impl Plugin for PosePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (swing_gaits, apply_offsets)
                .chain()
                .after(AnimationSystems)
                .before(TransformSystems::Propagate),
        );
    }
}

/// One bone's offset over its animated pose: turned about `axis`, in the
/// model root's own space, by `angle` radians, and moved by `shift` in its
/// parent's space.
#[derive(Reflect, Clone, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::pose"]
#[reflect(Default)]
pub struct PoseOffset {
    /// The bone's name in the model, as `J_Bip_L_UpperArm`.
    pub bone: String,
    pub axis: Vec3,
    pub angle: f32,
    pub shift: Vec3,
}

impl PoseOffset {
    /// `bone` turned about `axis` of the model by `angle` radians.
    pub fn turn(bone: &str, axis: Vec3, angle: f32) -> Self {
        Self {
            bone: bone.into(),
            axis,
            angle,
            shift: Vec3::ZERO,
        }
    }

    /// `bone` moved by `shift` in its parent's space.
    pub fn shift(bone: &str, shift: Vec3) -> Self {
        Self {
            bone: bone.into(),
            shift,
            ..default()
        }
    }
}

/// On a model's root: offsets over its animated pose, in named layers so
/// that what sets one does not undo another. Layers apply in their names'
/// order, and offsets to the same bone add up.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::pose"]
#[reflect(Component, Default)]
pub struct PoseOffsets {
    pub layers: BTreeMap<String, Vec<PoseOffset>>,
}

impl PoseOffsets {
    /// Sets `layer` to `offsets`, taking it away when they are none.
    pub fn set(&mut self, layer: &str, offsets: Vec<PoseOffset>) {
        if offsets.is_empty() {
            self.layers.remove(layer);
        } else if self.layers.get(layer) != Some(&offsets) {
            self.layers.insert(layer.into(), offsets);
        }
    }
}

/// The layer of [`PoseOffsets`] a [`Gait`] writes.
pub const GAIT: &str = "gait";

/// One part a [`Gait`] swings: turned about `axis` of the model, at most by
/// `angle` radians either way, furthest forward at `phase` of the step
/// (0 to 1).
#[derive(Reflect, Clone, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::pose"]
#[reflect(Default)]
pub struct Swing {
    pub bone: String,
    pub axis: Vec3,
    pub angle: f32,
    pub phase: f32,
}

/// On a model's root: swings its parts to and fro as it goes, one whole
/// step every `stride` meters it covers, and eases them still as it stops.
/// The legs of a four-legged prop, swung from the hip, walk it; the pairs
/// across from each other half a step apart trot.
#[derive(Component, Reflect, Clone, Debug, Default)]
#[type_path = "besfa_editor_plugin::pose"]
#[reflect(Component, Default)]
#[require(PoseOffsets)]
pub struct Gait {
    pub swings: Vec<Swing>,
    pub stride: f32,
    /// Where in its step it is, 0 to 1.
    #[reflect(ignore)]
    step: f32,
    /// How far into its stride it is, 0 standing to 1 going.
    #[reflect(ignore)]
    going: f32,
    #[reflect(ignore)]
    last: Option<Vec3>,
}

impl Gait {
    /// A step every `stride` meters, swinging `swings`.
    pub fn new(stride: f32, swings: Vec<Swing>) -> Self {
        Self {
            swings,
            stride,
            ..default()
        }
    }

    /// Four legs swung about the model's sideways axis from their hips,
    /// `[front left, front right, back left, back right]`, each pair across
    /// from the other half a step apart: a trot.
    pub fn trot(stride: f32, angle: f32, hips: [&str; 4]) -> Self {
        let swing = |bone: &str, phase| Swing {
            bone: bone.into(),
            axis: Vec3::X,
            angle,
            phase,
        };
        Self::new(
            stride,
            vec![swing(hips[0], 0.0), swing(hips[1], 0.5), swing(hips[2], 0.5), swing(hips[3], 0.0)],
        )
    }
}

/// How fast a gait eases in and out of its stride, per second.
const EASE: f32 = 6.0;

fn swing_gaits(time: Res<Time>, mut gaits: Query<(&mut Gait, &GlobalTransform, &mut PoseOffsets)>) {
    for (mut gait, place, mut offsets) in &mut gaits {
        let at = place.translation();
        let covered = gait.last.map_or(0.0, |last| (at - last).with_y(0.0).length());
        gait.last = Some(at);
        // More than a stride at once is a jump to a new place, not a step.
        let covered = if covered < gait.stride { covered } else { 0.0 };
        let target = if covered > 1e-4 { 1.0 } else { 0.0 };
        let ease = (time.delta_secs() * EASE).min(1.0);
        gait.going += (target - gait.going) * ease;
        if gait.stride > 0.0 {
            gait.step = (gait.step + covered / gait.stride).fract();
        }
        let swings = if gait.going < 0.01 {
            Vec::new()
        } else {
            gait.swings
                .iter()
                .map(|swing| {
                    let reach = (TAU * (gait.step - swing.phase)).cos();
                    PoseOffset::turn(&swing.bone, swing.axis, swing.angle * gait.going * reach)
                })
                .collect()
        };
        offsets.set(GAIT, swings);
    }
}

/// What the offsets of a root found and did: its bones by name, and each
/// bone offset last frame with what was written to it and the pose under.
#[derive(Component, Default)]
struct Posed {
    bones: HashMap<String, Entity>,
    written: HashMap<Entity, (Transform, Transform)>,
}

#[allow(clippy::type_complexity, reason = "a system's parameters")]
fn apply_offsets(
    mut commands: Commands,
    mut roots: Query<(Entity, &GlobalTransform, &PoseOffsets, Option<&mut Posed>)>,
    children: Query<&Children>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut bones: Query<&mut Transform>,
) {
    for (root, place, offsets, posed) in &mut roots {
        let Some(mut posed) = posed else {
            commands.entity(root).insert(Posed::default());
            continue;
        };
        // The pose under the offsets: what the clip wrote this frame, or,
        // where nothing did, what was there before they were.
        let mut bases: HashMap<Entity, Transform> = HashMap::new();
        for (bone, (written, base)) in &posed.written {
            if let Ok(now) = bones.get(*bone) {
                bases.insert(*bone, if *now == *written { *base } else { *now });
            }
        }
        let mut working: HashMap<Entity, Transform> = HashMap::new();
        for offset in offsets.layers.values().flatten() {
            let found = posed.bones.get(&offset.bone).copied().filter(|bone| bones.contains(*bone));
            let Some(bone) = found.or_else(|| {
                children
                    .iter_descendants(root)
                    .find(|bone| names.get(*bone).is_ok_and(|name| name.as_str() == offset.bone))
            }) else {
                continue;
            };
            posed.bones.insert(offset.bone.clone(), bone);
            let Ok(now) = bones.get(bone) else {
                continue;
            };
            let base = *bases.entry(bone).or_insert(*now);
            let pose = working.entry(bone).or_insert(base);
            if offset.angle != 0.0
                && let Some(axis) = (place.rotation() * offset.axis).try_normalize()
            {
                // About the model's axis, in the bone's parent's turn.
                let turn = parents
                    .get(bone)
                    .ok()
                    .and_then(|parent| globals.get(parent.parent()).ok())
                    .map_or(Quat::IDENTITY, |parent| parent.rotation());
                pose.rotation = turn.inverse() * Quat::from_axis_angle(axis, offset.angle) * turn * pose.rotation;
            }
            pose.translation += offset.shift;
        }
        // Bones no longer offset get their own pose back.
        let left: Vec<Entity> = posed.written.keys().filter(|bone| !working.contains_key(*bone)).copied().collect();
        for bone in left {
            posed.written.remove(&bone);
            if let (Some(base), Ok(mut now)) = (bases.get(&bone), bones.get_mut(bone)) {
                *now = *base;
            }
        }
        for (bone, pose) in working {
            if let Ok(mut now) = bones.get_mut(bone) {
                *now = pose;
            }
            posed.written.insert(bone, (pose, bases[&bone]));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;

    fn model(world: &mut World) -> (Entity, Entity) {
        let root = world.spawn((Transform::default(), GlobalTransform::default())).id();
        let arm = world
            .spawn((
                Name::new("Arm"),
                Transform::from_xyz(0.0, 1.0, 0.0),
                GlobalTransform::default(),
                ChildOf(root),
            ))
            .id();
        (root, arm)
    }

    #[test]
    fn turns_and_moves_a_bone_and_gives_back_one_no_clip_moves() {
        let mut app = App::new();
        app.add_systems(Update, apply_offsets);
        let (root, arm) = model(app.world_mut());
        let mut offsets = PoseOffsets::default();
        offsets.set(
            "raise",
            vec![PoseOffset::turn("Arm", Vec3::X, FRAC_PI_2), PoseOffset::shift("Arm", Vec3::Y)],
        );
        app.world_mut().entity_mut(root).insert(offsets);
        // The first frame finds the model.
        app.update();
        app.update();
        let at = |app: &App| *app.world().get::<Transform>(arm).unwrap();
        let raised = at(&app);
        assert!(raised.rotation.abs_diff_eq(Quat::from_rotation_x(FRAC_PI_2), 1e-5));
        assert_eq!(raised.translation, Vec3::new(0.0, 2.0, 0.0));
        // Frame after frame on a bone no clip moves, it does not add up.
        app.update();
        assert_eq!(at(&app), raised);
        // Without the layer, its own pose again.
        app.world_mut().get_mut::<PoseOffsets>(root).unwrap().set("raise", Vec::new());
        app.update();
        assert_eq!(at(&app), Transform::from_xyz(0.0, 1.0, 0.0));
    }

    #[test]
    fn a_gait_swings_its_legs_as_it_goes_and_eases_them_still() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.add_systems(Update, (swing_gaits, apply_offsets).chain());
        let (root, leg) = model(app.world_mut());
        app.world_mut().entity_mut(root).insert(Gait::new(
            1.0,
            vec![Swing {
                bone: "Arm".into(),
                axis: Vec3::X,
                angle: 0.5,
                phase: 0.0,
            }],
        ));
        let go = |app: &mut App, x: f32| {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(100));
            *app.world_mut().get_mut::<GlobalTransform>(root).unwrap() = GlobalTransform::from_xyz(x, 0.0, 0.0);
            app.update();
        };
        let swung = |app: &App| app.world().get::<Transform>(leg).unwrap().rotation.angle_between(Quat::IDENTITY);
        for step in 0..6 {
            go(&mut app, step as f32 * 0.3);
        }
        assert!(swung(&app) > 0.05, "going, the leg swings");
        // Standing, it eases still and the leg is given back its own pose.
        for _ in 0..30 {
            go(&mut app, 1.5);
        }
        assert!(swung(&app) < 1e-4);
        // A jump to a new place is no step.
        go(&mut app, 100.0);
        assert!(swung(&app) < 1e-4);
    }

    #[test]
    fn a_clips_new_pose_is_what_the_offset_goes_over() {
        let mut app = App::new();
        app.add_systems(Update, apply_offsets);
        let (root, arm) = model(app.world_mut());
        let mut offsets = PoseOffsets::default();
        offsets.set("lift", vec![PoseOffset::shift("Arm", Vec3::Y)]);
        app.world_mut().entity_mut(root).insert(offsets);
        app.update();
        app.update();
        // A clip moves the bone: the offset goes over where it moved it.
        app.world_mut().get_mut::<Transform>(arm).unwrap().translation = Vec3::new(0.0, 5.0, 0.0);
        app.update();
        assert_eq!(app.world().get::<Transform>(arm).unwrap().translation, Vec3::new(0.0, 6.0, 0.0));
    }
}
