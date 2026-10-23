//! The Animation tab: the selected model's animation state machine, which
//! the game plays on the model's clips. Its states and transitions are one
//! component the panel sets as a whole, so undo, saving and `besfa_mcp`'s
//! `set` treat it like any other component.
// ponytail: lists, not a node graph; draw states as boxes once machines
// grow past what a list shows at a glance.

use eframe::egui::{self, ComboBox, Id, RichText, Ui};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    inspector::{Action, Inspected, commit_field, format_number, number_field},
    panels::{ACCENT, MUTED, section, tag},
};

pub const MACHINE: &str = "besfa_editor_plugin::animation::AnimationStateMachine";
const PARAMS: &str = "besfa_editor_plugin::animation::AnimationParams";
const CLIPS: &str = "besfa_editor_plugin::animation::AnimationClips";

/// What a transition's `from` names for any state.
const ANY: &str = "*";

const TESTS: [&str; 5] = ["Greater", "Less", "True", "False", "Trigger"];

/// The plugin's `AnimationStateMachine`, in the shape the game reports it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
struct Machine {
    states: Vec<State>,
    transitions: Vec<Transition>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
struct State {
    name: String,
    clip: String,
    once: bool,
    speed: f32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            name: String::new(),
            clip: String::new(),
            once: false,
            speed: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
struct Transition {
    from: String,
    to: String,
    conditions: Vec<Condition>,
    blend: f32,
    exit_time: f32,
}

impl Default for Transition {
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
struct Condition {
    param: String,
    test: String,
    value: f32,
}

impl Default for Condition {
    fn default() -> Self {
        Self {
            param: String::new(),
            test: TESTS[0].into(),
            value: 0.0,
        }
    }
}

impl Machine {
    /// Renames a state and what its transitions say.
    fn rename(&mut self, index: usize, name: String) {
        let old = std::mem::replace(&mut self.states[index].name, name.clone());
        for transition in &mut self.transitions {
            for end in [&mut transition.from, &mut transition.to] {
                if *end == old {
                    *end = name.clone();
                }
            }
        }
    }

    /// Takes a state away with the transitions to and from it.
    fn remove(&mut self, index: usize) {
        let name = self.states.remove(index).name;
        self.transitions
            .retain(|transition| transition.from != name && transition.to != name);
    }
}

/// The tab for the selected entity; the change to make to it, if any.
pub fn ui(ui: &mut Ui, scene: &Inspected) -> Option<Action> {
    let Some(selected) = scene.selected.filter(|_| scene.entity.is_some()) else {
        tag(ui, "Select a placed model in the Hierarchy.");
        return None;
    };
    let component = |path| {
        scene
            .components
            .iter()
            .find(|component| component.path == path)
            .and_then(|component| component.value.as_ref())
    };
    let Some(value) = component(MACHINE) else {
        if !scene.addable.iter().any(|t| t.path == MACHINE) {
            tag(
                ui,
                "Update besfa_editor_plugin in the game to play animation state machines.",
            );
            return None;
        }
        tag(
            ui,
            "Put a state machine on a placed glTF model to play its clips.",
        );
        return ui
            .button("➕ Add state machine")
            .clicked()
            .then(|| Action::Insert(MACHINE.into()));
    };
    let Ok(machine) = serde_json::from_value::<Machine>(value.clone()) else {
        tag(
            ui,
            "This state machine has a shape the editor does not know.",
        );
        return None;
    };
    let clips: Vec<String> = component(CLIPS)
        .and_then(|clips| serde_json::from_value(clips.clone()).ok())
        .unwrap_or_default();
    if let Some(params) = component(PARAMS) {
        playing(ui, params);
    }
    let mut edited = machine.clone();
    let id = Id::new(("animation", selected));
    states(ui, id.with("states"), &mut edited, &clips);
    transitions(ui, id.with("transitions"), &mut edited);
    (edited != machine).then(|| Action::Set(MACHINE.into(), serde_json::to_value(edited).unwrap()))
}

/// The state playing and what the game says, which change as it plays.
fn playing(ui: &mut Ui, params: &Value) {
    let state = params["state"].as_str().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Playing").small().color(MUTED));
        ui.label(RichText::new(state).strong().color(ACCENT));
    });
    let mut said: Vec<String> = Vec::new();
    for (kind, values) in [("floats", &params["floats"]), ("bools", &params["bools"])] {
        for (name, value) in values.as_object().into_iter().flatten() {
            let value = match value {
                Value::Number(number) if kind == "floats" => format_number(number),
                other => other.to_string(),
            };
            said.push(format!("{name} {value}"));
        }
    }
    for trigger in params["triggers"].as_array().into_iter().flatten() {
        said.push(format!("{} pulled", trigger.as_str().unwrap_or_default()));
    }
    if !said.is_empty() {
        tag(ui, &said.join("  ·  "));
    }
}

fn states(ui: &mut Ui, id: Id, machine: &mut Machine, clips: &[String]) {
    section(ui, "States");
    if clips.is_empty() {
        tag(ui, "The model's clips show once it has loaded.");
    }
    let mut removed = None;
    let mut first = None;
    let mut renamed = None;
    for index in 0..machine.states.len() {
        let id = id.with(index);
        let state = &mut machine.states[index];
        ui.group(|ui| {
            ui.horizontal(|ui| {
                if index == 0 {
                    ui.label(RichText::new("▶").color(ACCENT))
                        .on_hover_text("Plays first");
                } else if ui.small_button("⏶").on_hover_text("Play first").clicked() {
                    first = Some(index);
                }
                if ui.small_button("🗙").on_hover_text("Remove").clicked() {
                    removed = Some(index);
                }
                if let Some(name) = commit_field(ui, id.with("name"), &state.name, Some("Name ")) {
                    renamed = Some((index, name));
                }
            });
            ui.horizontal(|ui| {
                ComboBox::from_id_salt(id.with("clip"))
                    .selected_text(state.clip.clone())
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for clip in clips {
                            ui.selectable_value(&mut state.clip, clip.clone(), clip);
                        }
                    })
                    .response
                    .on_hover_text("The model's clip it plays");
                ui.checkbox(&mut state.once, "Once")
                    .on_hover_text("Plays once and holds its last pose");
                number(ui, id.with("speed"), &mut state.speed, Some("×"));
            });
        });
    }
    if let Some((index, name)) = renamed {
        machine.rename(index, name);
    }
    if let Some(index) = first {
        let state = machine.states.remove(index);
        machine.states.insert(0, state);
    }
    if let Some(index) = removed {
        machine.remove(index);
    }
    if ui.button("➕ Add state").clicked() {
        let taken = |name: &str| machine.states.iter().any(|state| state.name == name);
        // An unused clip names it, else a number.
        let clip = clips.iter().find(|clip| !taken(clip)).cloned();
        let name = clip.clone().unwrap_or_else(|| {
            (1..)
                .map(|n| format!("State {n}"))
                .find(|name| !taken(name))
                .unwrap()
        });
        machine.states.push(State {
            name,
            clip: clip.or_else(|| clips.first().cloned()).unwrap_or_default(),
            ..State::default()
        });
    }
}

fn transitions(ui: &mut Ui, id: Id, machine: &mut Machine) {
    section(ui, "Transitions");
    tag(
        ui,
        "Taken first in the list first, once every condition holds.",
    );
    let names: Vec<String> = machine
        .states
        .iter()
        .map(|state| state.name.clone())
        .collect();
    let mut removed = None;
    for (index, transition) in machine.transitions.iter_mut().enumerate() {
        let id = id.with(index);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                if ui.small_button("🗙").on_hover_text("Remove").clicked() {
                    removed = Some(index);
                }
                let from = if transition.from == ANY {
                    "Any"
                } else {
                    &transition.from
                };
                ComboBox::from_id_salt(id.with("from"))
                    .selected_text(from.to_string())
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut transition.from, ANY.into(), "Any")
                            .on_hover_text("Any state but the one it goes to");
                        for name in &names {
                            ui.selectable_value(&mut transition.from, name.clone(), name);
                        }
                    });
                ui.label("→");
                ComboBox::from_id_salt(id.with("to"))
                    .selected_text(transition.to.clone())
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for name in &names {
                            ui.selectable_value(&mut transition.to, name.clone(), name);
                        }
                    });
            });
            ui.horizontal(|ui| {
                number(
                    ui,
                    id.with("blend"),
                    &mut transition.blend,
                    Some("Blend s "),
                )
                .on_hover_text("Crossfade seconds");
                number(
                    ui,
                    id.with("exit"),
                    &mut transition.exit_time,
                    Some("Exit "),
                )
                .on_hover_text(
                    "How much of the clip plays first, 1 for once through; 0 for any time",
                );
            });
            conditions(ui, id.with("conditions"), &mut transition.conditions);
        });
    }
    if let Some(index) = removed {
        machine.transitions.remove(index);
    }
    if ui.button("➕ Add transition").clicked() {
        let end = |index: usize| {
            names
                .get(index)
                .or(names.first())
                .cloned()
                .unwrap_or_default()
        };
        machine.transitions.push(Transition {
            from: end(0),
            to: end(1),
            ..Transition::default()
        });
    }
}

fn conditions(ui: &mut Ui, id: Id, conditions: &mut Vec<Condition>) {
    let mut removed = None;
    for (index, condition) in conditions.iter_mut().enumerate() {
        let id = id.with(index);
        ui.horizontal(|ui| {
            if ui.small_button("🗙").on_hover_text("Remove").clicked() {
                removed = Some(index);
            }
            ComboBox::from_id_salt(id.with("test"))
                .selected_text(condition.test.clone())
                .width(64.0)
                .show_ui(ui, |ui| {
                    for test in TESTS {
                        ui.selectable_value(&mut condition.test, test.into(), test);
                    }
                });
            let compares = matches!(condition.test.as_str(), "Greater" | "Less");
            ui.allocate_ui(
                egui::vec2(if compares { 90.0 } else { 160.0 }, 20.0),
                |ui| {
                    if let Some(param) = commit_field(ui, id.with("param"), &condition.param, None)
                    {
                        condition.param = param;
                    }
                },
            );
            if compares {
                number(ui, id.with("value"), &mut condition.value, None);
            }
        });
    }
    if let Some(index) = removed {
        conditions.remove(index);
    }
    if ui.small_button("➕ Condition").clicked() {
        conditions.push(Condition::default());
    }
}

/// A number field that commits when it loses focus.
fn number(ui: &mut Ui, id: Id, value: &mut f32, label: Option<&str>) -> egui::Response {
    let number = serde_json::Number::from_f64(*value as f64).unwrap_or(0.into());
    ui.allocate_ui(egui::vec2(72.0, 20.0), |ui| {
        if let Some(typed) = number_field(ui, id, &number, label).and_then(|v| v.as_f64()) {
            *value = typed as f32;
        }
    })
    .response
}

#[cfg(test)]
mod tests {
    use besfa_protocol::{ComponentData, ComponentType, EntityInfo};
    use egui_kittest::{Harness, kittest::Queryable};
    use serde_json::json;

    use super::*;

    fn component(path: &str, value: Value) -> ComponentData {
        ComponentData {
            name: path.rsplit("::").next().unwrap().into(),
            path: path.into(),
            mutable: true,
            required_by: None,
            saved: true,
            value: Some(value),
            choices: Default::default(),
        }
    }

    /// The tab for an entity with `components`, and what it asked last.
    fn tab(components: Vec<ComponentData>) -> Harness<'static, Option<Value>> {
        let entity = EntityInfo {
            id: 7,
            name: Some("Hero".into()),
            parent: None,
            scene: true,
            asset: Some("characters/body1.glb".into()),
            sky: false,
        };
        let addable = vec![ComponentType {
            name: "AnimationStateMachine".into(),
            path: MACHINE.into(),
        }];
        Harness::builder().with_step_dt(1.0 / 60.0).build_ui_state(
            move |ui, asked: &mut Option<Value>| {
                let scene = Inspected {
                    selected: Some(7),
                    entity: Some(&entity),
                    placement: None,
                    components: &components,
                    systems: &[],
                    addable: &addable,
                    crate_name: None,
                };
                match super::ui(ui, &scene) {
                    Some(Action::Set(path, value)) if path == MACHINE => *asked = Some(value),
                    Some(Action::Insert(path)) if path == MACHINE => *asked = Some(json!("insert")),
                    Some(_) => panic!("only the machine changes"),
                    None => {}
                }
            },
            None,
        )
    }

    #[test]
    fn adds_a_machine_then_states_named_by_unused_clips() {
        let mut harness = tab(vec![]);
        harness.run();
        harness.get_by_label("➕ Add state machine").click();
        harness.run();
        assert_eq!(*harness.state(), Some(json!("insert")));

        let mut harness = tab(vec![
            component(
                MACHINE,
                json!({"states": [{"name": "idle", "clip": "idle"}], "transitions": []}),
            ),
            component(CLIPS, json!(["idle", "walk"])),
            component(
                PARAMS,
                json!({"floats": {"speed": 0.0}, "bools": {}, "triggers": [], "state": "idle"}),
            ),
        ]);
        harness.run();
        harness.get_by_label("➕ Add state").click();
        harness.run();
        let machine: Machine = serde_json::from_value(harness.state().clone().unwrap()).unwrap();
        assert_eq!(machine.states[1].name, "walk");
        assert_eq!(machine.states[1].clip, "walk");
        assert_eq!(machine.states[1].speed, 1.0);

        harness.get_by_label("➕ Add transition").click();
        harness.run();
        let machine: Machine = serde_json::from_value(harness.state().clone().unwrap()).unwrap();
        assert_eq!(machine.transitions.len(), 1);
        assert_eq!(
            (
                machine.transitions[0].from.as_str(),
                machine.transitions[0].to.as_str()
            ),
            ("idle", "idle"),
            "the panel's machine has one state, until the game reports the second"
        );
    }

    #[test]
    fn renames_and_removes_states_with_their_transitions() {
        let mut machine: Machine = serde_json::from_value(json!({
            "states": [{"name": "idle"}, {"name": "walk"}, {"name": "jump"}],
            "transitions": [
                {"from": "idle", "to": "walk"},
                {"from": "walk", "to": "idle"},
                {"from": "*", "to": "jump"},
            ],
        }))
        .unwrap();
        machine.rename(1, "stroll".into());
        assert_eq!(machine.transitions[0].to, "stroll");
        assert_eq!(machine.transitions[1].from, "stroll");
        machine.remove(0);
        assert_eq!(machine.states.len(), 2);
        assert_eq!(machine.transitions.len(), 1, "only Any → jump is left");
        assert_eq!(
            machine.transitions[0].blend, 0.2,
            "left out, so the default"
        );
    }

    #[test]
    fn says_to_update_an_old_plugin() {
        let mut harness = Harness::builder().build_ui(|ui| {
            let entity = EntityInfo {
                id: 7,
                name: None,
                parent: None,
                scene: true,
                asset: None,
                sky: false,
            };
            let scene = Inspected {
                selected: Some(7),
                entity: Some(&entity),
                placement: None,
                components: &[],
                systems: &[],
                addable: &[],
                crate_name: None,
            };
            super::ui(ui, &scene);
        });
        harness.run();
        harness.get_by_label(
            "Update besfa_editor_plugin in the game to play animation state machines.",
        );
    }
}
