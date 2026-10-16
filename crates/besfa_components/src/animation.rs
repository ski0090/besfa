//! Animation state machines. Put [`AnimationStateMachine`] on a placed glTF
//! model's root: its states play the model's named clips, and its
//! transitions move between them when the game's [`AnimationParams`] say
//! so, crossfading on the model's `AnimationPlayer`s. The game writes the
//! parameters and nothing else; the scene file keeps the machine, so the
//! editor's Animation tab and `set` from `besfa_mcp` change it live.
// ponytail: the machine sits inline on each root; a zombie horde shares one
// through a prefab. Move it to a file under assets/ when that stops being
// enough.

use std::{collections::BTreeMap, time::Duration};

use bevy::{app::AnimationSystems, gltf::Gltf, prelude::*, world_serialization::WorldAssetRoot};

/// In [`crate::default_plugins`]; add it on its own to a game that runs
/// without them.
pub struct AnimationStateMachinePlugin;

impl Plugin for AnimationStateMachinePlugin {
    fn build(&self, app: &mut App) {
        // After the game's Update has written the parameters, before the
        // players advance.
        app.add_systems(PostUpdate, (build, run).chain().before(AnimationSystems))
            .add_observer(stopped);
    }
}

/// States and the transitions between them. The first state is where the
/// machine starts.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::animation"]
#[reflect(Component, Default)]
#[require(AnimationParams)]
pub struct AnimationStateMachine {
    pub states: Vec<AnimationState>,
    pub transitions: Vec<AnimationTransition>,
}

// `#[reflect(Default)]` fills each field left out from `Default`: `set`
// panics on a list item it cannot build from what it was given.
#[derive(Reflect, Clone, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::animation"]
#[reflect(Default)]
pub struct AnimationState {
    pub name: String,
    /// The model's named glTF animation.
    pub clip: String,
    /// Plays once and holds its last pose, instead of looping.
    pub once: bool,
    pub speed: f32,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            name: String::new(),
            clip: String::new(),
            once: false,
            speed: 1.0,
        }
    }
}

/// Taken, first in the list first, when every condition holds and the
/// exit time has passed.
#[derive(Reflect, Clone, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::animation"]
#[reflect(Default)]
pub struct AnimationTransition {
    /// A state's name, or `*` for any state but `to`.
    pub from: String,
    pub to: String,
    pub conditions: Vec<AnimationCondition>,
    /// Crossfade seconds.
    pub blend: f32,
    /// How much of `from`'s clip must have played first, 1 for once
    /// through; 0 for any time.
    pub exit_time: f32,
}

impl Default for AnimationTransition {
    fn default() -> Self {
        Self {
            from: String::new(),
            to: String::new(),
            conditions: Vec::new(),
            blend: 0.2,
            exit_time: 0.0,
        }
    }
}

#[derive(Reflect, Clone, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::animation"]
#[reflect(Default)]
pub struct AnimationCondition {
    pub param: String,
    pub test: ConditionTest,
    /// What `Greater` and `Less` compare with.
    pub value: f32,
}

#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[type_path = "besfa_editor_plugin::animation"]
#[reflect(Default)]
pub enum ConditionTest {
    /// A float parameter is above the value.
    #[default]
    Greater,
    Less,
    /// A bool parameter is set, or not.
    True,
    False,
    /// The trigger was pulled; taking the transition lets it go.
    Trigger,
}

/// What the game tells the machine. Parameters it never wrote read as 0
/// and false.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::animation"]
#[reflect(Component, Default)]
pub struct AnimationParams {
    pub floats: BTreeMap<String, f32>,
    pub bools: BTreeMap<String, bool>,
    /// Pulled until a transition that tests them is taken.
    pub triggers: Vec<String>,
    /// The state playing, which the machine writes; write a state's name
    /// to go there at once.
    pub state: String,
}

/// The named clips of the model the machine plays, for the editor to pick
/// from; computed, so the scene file leaves it out.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::animation"]
#[reflect(Component)]
pub struct AnimationClips(pub Vec<String>);

/// The graph the machine plays through, built from its model's clips.
#[derive(Component)]
struct Running {
    /// Holds the file, so its clips stay loaded.
    gltf: Handle<Gltf>,
    /// None until the model has loaded and spawned its players.
    graph: Option<Handle<AnimationGraph>>,
    /// Each state's clip node and length, when the model has the clip.
    nodes: Vec<Option<(AnimationNodeIndex, f32)>>,
    players: Vec<Entity>,
    current: Option<usize>,
}

/// Builds each machine's graph once its model has spawned, and again when
/// the machine changes or the model spawns anew, as after a reimport.
#[allow(
    clippy::type_complexity,
    clippy::too_many_arguments,
    reason = "a system's parameters"
)]
fn build(
    mut commands: Commands,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    clips: Res<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut machines: Query<(
        Entity,
        Ref<AnimationStateMachine>,
        Ref<WorldAssetRoot>,
        Option<&mut Running>,
        Option<&AnimationClips>,
    )>,
    children: Query<&Children>,
    players: Query<Option<&AnimationGraphHandle>, With<AnimationPlayer>>,
) {
    for (root, machine, model, running, shown) in &mut machines {
        let Some(mut running) = running.filter(|_| !model.is_changed()) else {
            let Some(path) = assets.get_path(model.0.id()) else {
                continue;
            };
            commands.entity(root).insert(Running {
                gltf: assets.load(path.without_label().into_owned()),
                graph: None,
                nodes: Vec::new(),
                players: Vec::new(),
                current: None,
            });
            continue;
        };
        let respawned = running.players.iter().any(|&player| {
            players
                .get(player)
                .ok()
                .flatten()
                .is_none_or(|graph| Some(&graph.0) != running.graph.as_ref())
        });
        if running.graph.is_some() && !respawned && !machine.is_changed() {
            continue;
        }
        let Some(gltf) = gltfs.get(&running.gltf) else {
            continue;
        };
        let found: Vec<Entity> = children
            .iter_descendants(root)
            .filter(|&entity| players.contains(entity))
            .collect();
        if found.is_empty() {
            continue;
        }
        let names: Vec<String> = gltf
            .animations
            .iter()
            .filter_map(|handle| {
                let (name, _) = gltf
                    .named_animations
                    .iter()
                    .find(|(_, named)| *named == handle)?;
                Some(name.to_string())
            })
            .collect();
        if shown.is_none_or(|shown| shown.0 != names) {
            commands.entity(root).insert(AnimationClips(names.clone()));
        }
        let mut graph = AnimationGraph::new();
        let graph_root = graph.root;
        running.nodes = machine
            .states
            .iter()
            .map(|state| {
                let Some(clip) = gltf.named_animations.get(state.clip.as_str()) else {
                    warn!(
                        "Animation state {} plays clip {:?}, which the model does not have; it has {names:?}",
                        state.name, state.clip
                    );
                    return None;
                };
                let length = clips.get(clip).map_or(0.0, AnimationClip::duration);
                Some((graph.add_clip(clip.clone(), 1.0, graph_root), length))
            })
            .collect();
        let graph = graphs.add(graph);
        for &player in &found {
            commands.entity(player).insert((
                AnimationGraphHandle(graph.clone()),
                AnimationTransitions::new(),
            ));
        }
        running.graph = Some(graph);
        running.players = found;
        // Starts again where the parameters say, the state it was in.
        running.current = None;
    }
}

/// The machine was taken off: what it computed goes with it.
fn stopped(remove: On<Remove<AnimationStateMachine>>, mut commands: Commands) {
    commands
        .entity(remove.entity)
        .try_remove::<(Running, AnimationClips)>();
}

/// Takes the transition the parameters call for and plays its state.
fn run(
    mut machines: Query<(&AnimationStateMachine, &mut AnimationParams, &mut Running)>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (machine, mut params, mut running) in &mut machines {
        if running.graph.is_none() || machine.states.is_empty() {
            continue;
        }
        let named = machine
            .states
            .iter()
            .position(|state| state.name == params.state);
        let (target, transition) = match (running.current, named) {
            (current, Some(named)) if current != Some(named) => (named, None),
            (None, _) => (0, None),
            (Some(current), _) => {
                // A state without its clip is over at once.
                let progress = running.nodes.get(current).copied().flatten().map_or(
                    f32::INFINITY,
                    |(node, length)| {
                        running
                            .players
                            .first()
                            .and_then(|&player| players.get(player).ok())
                            .and_then(|(player, _)| player.animation(node))
                            .map_or(0.0, |active| {
                                active.completions() as f32
                                    + if length > 0.0 {
                                        active.seek_time() / length
                                    } else {
                                        1.0
                                    }
                            })
                    },
                );
                match next(machine, &params, current, progress) {
                    Some(transition) => {
                        let to = machine.states.iter().position(|s| s.name == transition.to);
                        (to.unwrap(), Some(transition))
                    }
                    // Cleared or misnamed from outside: say what plays.
                    None => {
                        let name = &machine.states[current].name;
                        if params.state != *name {
                            params.state = name.clone();
                        }
                        continue;
                    }
                }
            }
        };
        let state = &machine.states[target];
        if let Some(Some((node, _))) = running.nodes.get(target) {
            let fade = Duration::from_secs_f32(transition.map_or(0.0, |t| t.blend.max(0.0)));
            for &player in &running.players {
                let Ok((mut player, mut transitions)) = players.get_mut(player) else {
                    continue;
                };
                let active = transitions.play(&mut player, *node, fade);
                // Fading out when it was asked for again, which play leaves.
                active.set_weight(1.0).set_speed(state.speed);
                if !state.once {
                    active.repeat();
                }
            }
        }
        if let Some(transition) = transition {
            for condition in &transition.conditions {
                if condition.test == ConditionTest::Trigger {
                    params
                        .triggers
                        .retain(|trigger| *trigger != condition.param);
                }
            }
        }
        if params.state != state.name {
            params.state = state.name.clone();
        }
        running.current = Some(target);
    }
}

/// The first transition out of state `current` whose conditions hold,
/// with `progress` of its clip played (2.5 is halfway through the third
/// loop).
fn next<'a>(
    machine: &'a AnimationStateMachine,
    params: &AnimationParams,
    current: usize,
    progress: f32,
) -> Option<&'a AnimationTransition> {
    let name = &machine.states[current].name;
    machine.transitions.iter().find(|transition| {
        let from = transition.from == *name || (transition.from == "*" && transition.to != *name);
        from && machine
            .states
            .iter()
            .any(|state| state.name == transition.to)
            && progress >= transition.exit_time
            && transition.conditions.iter().all(|condition| {
                let float = || params.floats.get(&condition.param).copied().unwrap_or(0.0);
                let bool = || params.bools.get(&condition.param).copied().unwrap_or(false);
                match condition.test {
                    ConditionTest::Greater => float() > condition.value,
                    ConditionTest::Less => float() < condition.value,
                    ConditionTest::True => bool(),
                    ConditionTest::False => !bool(),
                    ConditionTest::Trigger => params.triggers.contains(&condition.param),
                }
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str) -> AnimationState {
        AnimationState {
            name: name.into(),
            clip: name.into(),
            ..default()
        }
    }

    fn when(param: &str, test: ConditionTest, value: f32) -> AnimationCondition {
        AnimationCondition {
            param: param.into(),
            test,
            value,
        }
    }

    fn go(from: &str, to: &str, conditions: Vec<AnimationCondition>) -> AnimationTransition {
        AnimationTransition {
            from: from.into(),
            to: to.into(),
            conditions,
            ..default()
        }
    }

    #[test]
    fn takes_the_first_transition_whose_conditions_hold() {
        use ConditionTest::*;
        let machine = AnimationStateMachine {
            states: vec![state("idle"), state("walk"), state("run"), state("jump")],
            transitions: vec![
                go("idle", "walk", vec![when("speed", Greater, 0.1)]),
                go(
                    "walk",
                    "run",
                    vec![when("speed", Greater, 0.1), when("running", True, 0.0)],
                ),
                go("walk", "idle", vec![when("speed", Less, 0.1)]),
                go("*", "jump", vec![when("jump", Trigger, 0.0)]),
                AnimationTransition {
                    exit_time: 1.0,
                    ..go("jump", "idle", vec![])
                },
            ],
        };
        let to = |params: &AnimationParams, current, progress| {
            next(&machine, params, current, progress).map(|t| t.to.as_str())
        };
        let mut params = AnimationParams::default();
        assert_eq!(to(&params, 0, 0.0), None, "standing still");
        params.floats.insert("speed".into(), 2.0);
        assert_eq!(to(&params, 0, 0.0), Some("walk"));
        assert_eq!(to(&params, 1, 0.0), None, "walking, not running");
        params.bools.insert("running".into(), true);
        assert_eq!(to(&params, 1, 0.0), Some("run"));
        params.triggers.push("jump".into());
        assert_eq!(to(&params, 2, 0.0), Some("jump"), "from any state");
        assert_eq!(
            to(&params, 3, 0.5),
            None,
            "not again from itself, nor before the end"
        );
        assert_eq!(to(&params, 3, 1.0), Some("idle"));
    }

    #[test]
    fn plays_states_and_lets_a_taken_trigger_go() {
        let mut app = App::new();
        app.add_systems(Update, run);
        let machine = AnimationStateMachine {
            states: vec![state("idle"), state("jump")],
            transitions: vec![go(
                "*",
                "jump",
                vec![when("jump", ConditionTest::Trigger, 0.0)],
            )],
        };
        // No players: the states' clips are missing, so each is over at once.
        let character = app
            .world_mut()
            .spawn((
                machine,
                Running {
                    gltf: Handle::default(),
                    graph: Some(Handle::default()),
                    nodes: vec![None, None],
                    players: Vec::new(),
                    current: None,
                },
            ))
            .id();
        let params = |app: &App| {
            app.world()
                .get::<AnimationParams>(character)
                .unwrap()
                .clone()
        };

        app.update();
        assert_eq!(params(&app).state, "idle", "starts in the first state");
        app.world_mut()
            .get_mut::<AnimationParams>(character)
            .unwrap()
            .triggers
            .push("jump".into());
        app.update();
        assert_eq!(params(&app).state, "jump");
        assert!(params(&app).triggers.is_empty(), "the trigger was let go");

        app.world_mut()
            .get_mut::<AnimationParams>(character)
            .unwrap()
            .state = "idle".into();
        app.update();
        assert_eq!(params(&app).state, "idle", "went where it was told");
        app.world_mut()
            .get_mut::<AnimationParams>(character)
            .unwrap()
            .state = "nope".into();
        app.update();
        assert_eq!(params(&app).state, "idle", "an unknown state is put right");
    }
}
