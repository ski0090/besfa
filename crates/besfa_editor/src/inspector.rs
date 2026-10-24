//! The selected entity and the game's systems. Simply, what there is to
//! edit: the name, the components the scene file keeps with Transform
//! first, the game's own components, and the game's own systems. In detail,
//! every component and system grouped by crate, with what Bevy computes.

use std::collections::BTreeMap;

use besfa_protocol::{ComponentData, ComponentType, EntityInfo, Shapes, SystemInfo, crate_of};
use eframe::egui::{self, Id, Label, RichText, ScrollArea, TextEdit, Ui};
use serde_json::{Value, json};

use crate::{
    panels::{MUTED, TEXT, panel_header, section, tag},
    terrain::{TERRAIN, TerrainTools},
};

pub const TRANSFORM: &str = "bevy_transform::components::transform::Transform";
const NAME: &str = "bevy_ecs::name::Name";

/// Fields Bevy recomputes while the game runs, out of the simple view.
// ponytail: the template's camera; add types as they show more.
const RUNTIME_FIELDS: &[(&str, &[&str])] = &[("bevy_camera::camera::Camera", &["computed"])];

/// What the inspector asks of the game.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// A component's type path and its whole new value.
    Set(String, Value),
    Insert(String),
    Remove(String),
    /// Writes the placed prefab of this id back to its file.
    ApplyPrefab(u64),
}

/// What the inspector shows: the scene's selected entity, or a part of the
/// previewed model, with what the game reported about it.
pub struct Inspected<'a> {
    pub selected: Option<u64>,
    pub entity: Option<&'a EntityInfo>,
    /// The placed file the entity is part of, if any.
    pub placement: Option<&'a EntityInfo>,
    pub components: &'a [ComponentData],
    pub systems: &'a [SystemInfo],
    /// Components the game can add with a default value.
    pub addable: &'a [ComponentType],
    /// The game's own crate, listed before Bevy's.
    pub crate_name: Option<&'a str>,
}

#[derive(Default)]
pub struct Inspector {
    details: bool,
    /// The Add component search, while it is open.
    adding: Option<String>,
    /// The brush for a selected terrain.
    pub terrain: TerrainTools,
    /// The game's plugin takes the terrain brush, in edit mode.
    pub can_sculpt: bool,
}

impl Inspector {
    pub fn ui(&mut self, ui: &mut Ui, scene: &Inspected, can_apply_prefab: bool) -> Option<Action> {
        panel_header(ui, "Inspector", |ui| self.details_button(ui));
        ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.add_space(8.0);
                let name = self.name(ui, scene);
                let terrain = scene
                    .components
                    .iter()
                    .find(|component| component.path == TERRAIN && scene.selected.is_some())
                    .and_then(|terrain| self.terrain.ui(ui, terrain, self.can_sculpt))
                    .map(|value| Action::Set(TERRAIN.into(), value));
                self.components(ui, scene, can_apply_prefab)
                    .or(terrain)
                    .or(name)
            })
            .inner
    }

    /// Switches between what there is to edit and every detail.
    pub fn details_button(&mut self, ui: &mut Ui) {
        let (icon, tip) = if self.details {
            ("⏶", "Hide details")
        } else {
            ("⏷", "Show details")
        };
        if ui.small_button(icon).on_hover_text(tip).clicked() {
            self.details = !self.details;
        }
    }

    /// Simply, the name's field; otherwise the entity's label.
    pub fn name(&mut self, ui: &mut Ui, scene: &Inspected) -> Option<Action> {
        let (selected, entity) = (scene.selected, scene.entity);
        match self
            .name_component(scene)
            .and_then(|name| name.value.as_ref())
        {
            Some(value) => value_editor(ui, Id::new((selected, NAME)), value, false, &[])
                .map(|value| Action::Set(NAME.into(), value)),
            None => {
                let text = match (selected, entity) {
                    (None, _) => "Select an entity in the Hierarchy".into(),
                    (Some(id), None) => format!("Entity {id} is gone"),
                    (_, Some(entity)) => entity.label(),
                };
                let color = if entity.is_some() { TEXT } else { MUTED };
                ui.label(RichText::new(text).strong().color(color));
                None
            }
        }
    }

    /// The `Name` component when [`Self::name`] edits it.
    fn name_component<'a>(&self, scene: &Inspected<'a>) -> Option<&'a ComponentData> {
        scene
            .components
            .iter()
            .find(|component| component.path == NAME && component.editable())
            .filter(|_| !self.details)
    }

    /// Where the entity comes from, its components and the game's systems.
    pub fn components(
        &mut self,
        ui: &mut Ui,
        scene: &Inspected,
        can_apply_prefab: bool,
    ) -> Option<Action> {
        let mut action = None;
        let (selected, entity) = (scene.selected, scene.entity);
        let name = self.name_component(scene);
        if let Some(entity) = entity {
            placement(ui, scene, entity, can_apply_prefab, &mut action);
        }
        if selected.is_some() {
            section(ui, "Components");
            let mut tile = |ui: &mut Ui, component: &ComponentData| {
                if let Some(change) = component_tile(ui, selected, component, self.details) {
                    action = Some(change);
                }
            };
            if self.details {
                // Groups that hold something the scene file keeps start open.
                let expand = |components: &[&ComponentData]| components.iter().any(|c| c.saved);
                crate_groups(
                    ui,
                    "components",
                    scene.components,
                    |c| c.crate_name(),
                    scene.crate_name,
                    expand,
                    &mut tile,
                );
            } else {
                for component in editable(scene, name) {
                    tile(ui, component);
                }
            }
            if entity.is_some() && ui.button("➕ Add component").clicked() {
                self.adding = Some(String::new());
            }
        }
        let system_row = |ui: &mut Ui, system: &SystemInfo| {
            let name = system
                .name
                .strip_prefix(&format!("{}::", crate_of(&system.name)));
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new(&system.schedule).small().color(MUTED));
                ui.label(
                    RichText::new(name.unwrap_or(&system.name))
                        .small()
                        .color(TEXT),
                );
            });
        };
        if self.details {
            section(ui, "Systems");
            crate_groups(
                ui,
                "systems",
                scene.systems,
                |s| crate_of(&s.name),
                scene.crate_name,
                |_| false,
                system_row,
            );
        } else {
            let game = |system: &&SystemInfo| Some(crate_of(&system.name)) == scene.crate_name;
            let systems: Vec<_> = scene.systems.iter().filter(game).collect();
            if !systems.is_empty() {
                section(ui, "Systems");
                for system in systems {
                    system_row(ui, system);
                }
            }
        }
        if let Some(path) = self.add_component(ui.ctx(), scene) {
            action = Some(Action::Insert(path));
        }
        action
    }

    /// Picks a component type the entity does not have by name; returns its
    /// type path.
    fn add_component(&mut self, ctx: &egui::Context, scene: &Inspected) -> Option<String> {
        let query = self.adding.as_mut()?;
        let mut picked = None;
        let mut cancel = false;
        let modal = egui::Modal::new(Id::new("add_component")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading("Add component");
            let search = ui.add(
                TextEdit::singleline(query)
                    .hint_text("Search components")
                    .desired_width(f32::INFINITY),
            );
            if !search.has_focus() && ui.memory(|m| m.focused().is_none()) {
                search.request_focus();
            }
            let lowercase = query.to_lowercase();
            let present = |path: &str| {
                scene
                    .components
                    .iter()
                    .any(|component| component.path == path)
            };
            let types: Vec<&ComponentType> = scene
                .addable
                .iter()
                .filter(|t| !present(&t.path) && t.path.to_lowercase().contains(&lowercase))
                .collect();
            ScrollArea::vertical()
                .max_height(360.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for t in types {
                        let text = format!("{}\n{}", t.name, crate_of(&t.path));
                        if ui
                            .selectable_label(false, RichText::new(text).color(TEXT))
                            .clicked()
                        {
                            picked = Some(t.path.clone());
                        }
                    }
                });
            cancel = ui.button("Cancel").clicked();
        });
        if picked.is_some() || cancel || modal.should_close() {
            self.adding = None;
        }
        picked
    }
}

/// What there is to edit, Transform first: the components the scene file
/// keeps, and the game's own, where an unreflected one is tagged so. The
/// `name` edited in the header is left out.
fn editable<'a>(scene: &Inspected<'a>, name: Option<&ComponentData>) -> Vec<&'a ComponentData> {
    let mut shown: Vec<_> = scene
        .components
        .iter()
        .filter(|c| !name.is_some_and(|name| name.path == c.path))
        .filter(|c| c.saved && c.value.is_some() || Some(c.crate_name()) == scene.crate_name)
        .collect();
    shown.sort_by_key(|c| c.path != TRANSFORM);
    shown
}

/// Where the entity comes from when Save scene does not keep it, and a way
/// to keep changes made inside a placed prefab.
fn placement(
    ui: &mut Ui,
    scene: &Inspected,
    entity: &EntityInfo,
    can_apply: bool,
    action: &mut Option<Action>,
) {
    let placed = scene.placement;
    if !entity.scene {
        tag(
            ui,
            &match placed.and_then(|p| p.asset.as_deref()) {
                // The game keeps changes to a model's parts on the model.
                Some(asset) if asset.ends_with(".glb") || asset.ends_with(".gltf") => {
                    format!("Part of {asset}; changes to it are saved on the placed model.")
                }
                Some(asset) => format!("Part of {asset}; Save scene skips it."),
                None => "Spawned by the game while it runs; Save scene skips it.".into(),
            },
        );
    }
    if let Some(placed) = placed {
        let asset = placed.asset.as_deref().unwrap_or_default();
        // The scene placed in itself is saved with Save scene.
        if can_apply
            && asset.ends_with(".scn.ron")
            && asset != "scenes/main.scn.ron"
            && ui.button("⮉ Apply to prefab").clicked()
        {
            *action = Some(Action::ApplyPrefab(placed.id));
        }
    }
}

/// One collapsible group per crate, the game's crate first. The game's crate
/// and groups `expand` picks start open.
fn crate_groups<T>(
    ui: &mut Ui,
    section: &str,
    items: &[T],
    crate_of: impl Fn(&T) -> &str,
    game: Option<&str>,
    expand: impl Fn(&[&T]) -> bool,
    mut show: impl FnMut(&mut Ui, &T),
) {
    let mut groups: BTreeMap<&str, Vec<&T>> = BTreeMap::new();
    for item in items {
        groups.entry(crate_of(item)).or_default().push(item);
    }
    let mut crates: Vec<_> = groups.into_iter().collect();
    crates.sort_by_key(|(name, _)| Some(*name) != game);
    for (name, items) in crates {
        egui::CollapsingHeader::new(
            RichText::new(format!("{name} ({})", items.len()))
                .small()
                .color(TEXT),
        )
        // The same crate can show under components and systems.
        .id_salt((section, name))
        .default_open(Some(name) == game || expand(&items))
        .show(ui, |ui| {
            for item in items {
                show(ui, item);
            }
        });
    }
}

fn component_tile(
    ui: &mut Ui,
    selected: Option<u64>,
    component: &ComponentData,
    details: bool,
) -> Option<Action> {
    let mut action = None;
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        let color = if component.saved { TEXT } else { MUTED };
        ui.label(RichText::new(&component.name).strong().color(color));
        if details {
            if let Some(required_by) = &component.required_by {
                tag(ui, &format!("required by {required_by}"));
            }
            if !component.saved {
                tag(ui, "computed");
            }
            if !component.mutable {
                tag(ui, "immutable");
            }
        }
        if component.value.is_none() {
            tag(ui, "no Reflect");
        }
        if component.saved {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("🗙")
                    .on_hover_text("Remove component")
                    .clicked()
                {
                    action = Some(Action::Remove(component.path.clone()));
                }
            });
        }
    });
    let value = component.value.as_ref()?;
    if component.editable() {
        let hidden = RUNTIME_FIELDS
            .iter()
            .find(|(path, _)| !details && *path == component.path)
            .map_or(&[][..], |(_, fields)| fields);
        // A new selection starts with fresh fields.
        let id = Id::new((selected, &component.path));
        if let Some(value) =
            value_editor_with(ui, id, value, component.path == TRANSFORM, hidden, &component.shapes)
        {
            action = Some(Action::Set(component.path.clone(), value));
        }
    } else {
        let text = match value {
            Value::Object(_) | Value::Array(_) => serde_json::to_string_pretty(value).unwrap(),
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        read_only(ui, &text, MUTED);
    }
    action
}

/// Edits a component's reflected JSON value: numbers, text and flags become
/// fields, nested structs indent under their field name. Returns the whole
/// new value once a field commits, since the game replaces the component
/// with it. With `euler_rotation`, the `rotation` quaternion shows as Euler
/// angles in degrees; `hidden` top-level fields are left out of view and
/// still sent back.
pub fn value_editor(
    ui: &mut Ui,
    id: Id,
    value: &Value,
    euler_rotation: bool,
    hidden: &[&str],
) -> Option<Value> {
    value_editor_with(ui, id, value, euler_rotation, hidden, &Shapes::default())
}

/// [`value_editor`], with what the value's type says of its fields: the
/// variants to pick text fields from, what an Option that is None and a new
/// list item start as.
pub fn value_editor_with(
    ui: &mut Ui,
    id: Id,
    value: &Value,
    euler_rotation: bool,
    hidden: &[&str],
    shapes: &Shapes,
) -> Option<Value> {
    let mut fields = Fields {
        id,
        euler_rotation,
        hidden,
        shapes,
        change: None,
    };
    fields.node(ui, value, &mut Vec::new(), None);
    let (path, leaf) = fields.change?;
    Some(replace(value, &path, leaf))
}

struct Fields<'a> {
    id: Id,
    euler_rotation: bool,
    /// What the value's type says of its fields, by their paths.
    shapes: &'a Shapes,
    hidden: &'a [&'a str],
    /// The field that committed: its path and new value.
    change: Option<(Vec<String>, Value)>,
}

impl Fields<'_> {
    fn node(&mut self, ui: &mut Ui, node: &Value, path: &mut Vec<String>, label: Option<&str>) {
        // An Option: None offers ➕ to give it a value, a value 🗙 to take it.
        let shapes = self.shapes;
        if let Some(start) = shapes.options.get(&shape(path, shapes)) {
            if node.is_null() {
                return row(ui, label, |ui| {
                    read_only(ui, "None", MUTED);
                    if ui.small_button("➕").on_hover_text("Give it a value").clicked() {
                        self.change = Some((path.clone(), start.clone()));
                    }
                });
            }
            let mut clear = false;
            row(ui, label, |ui| {
                clear = ui.small_button("🗙").on_hover_text("Make it None").clicked();
            });
            ui.indent((label, path.len()), |ui| self.content(ui, node, path, None));
            if clear {
                self.change = Some((path.clone(), Value::Null));
            }
            return;
        }
        self.content(ui, node, path, label);
    }

    fn content(&mut self, ui: &mut Ui, node: &Value, path: &mut Vec<String>, label: Option<&str>) {
        // A Bevy color picks from a color button.
        if let Some((space, channels)) = color_of(node) {
            return row(ui, label, |ui| {
                let srgb = space == "Srgba";
                let mut linear = channels;
                if srgb {
                    for channel in &mut linear[..3] {
                        *channel = egui::ecolor::linear_from_gamma(*channel);
                    }
                }
                if ui.color_edit_button_rgba_unmultiplied(&mut linear).changed() {
                    self.change = Some((path.clone(), color_value(space, linear, srgb)));
                }
            });
        }
        let map = match node {
            Value::Object(map) => map,
            Value::Array(items) if !is_vector(items) => {
                return self.list(ui, items, path, label.unwrap_or("items"));
            }
            _ => return row(ui, label, |ui| self.leaf(ui, node, path)),
        };
        if let Some(start) = self.shapes.maps.get(&shape(path, self.shapes)) {
            return self.entries(ui, map, path, label.unwrap_or("entries"), start);
        }
        let mut children = |ui: &mut Ui| {
            for (key, value) in map {
                if !path.is_empty() || !self.hidden.contains(&key.as_str()) {
                    path.push(key.clone());
                    self.node(ui, value, path, Some(key));
                    path.pop();
                }
            }
        };
        match label {
            None => children(ui),
            Some(label) => {
                row(ui, Some(label), |_| {});
                ui.indent(label, children);
            }
        }
    }

    /// A list: each item under its number with a 🗙 that takes it out, and a
    /// ➕ that adds a copy of the last item at the end, or an empty one,
    /// which the game fills with the item type's defaults. An array of a
    /// fixed length has neither. An Option item takes or loses its value as
    /// an Option field does.
    fn list(&mut self, ui: &mut Ui, items: &[Value], path: &mut Vec<String>, label: &str) {
        let shapes = self.shapes;
        let fixed = shapes.fixed.contains(&shape(path, shapes));
        let mut changed = None;
        row(ui, Some(label), |ui| {
            if !fixed && ui.small_button("➕").on_hover_text("Add an item").clicked() {
                let mut grown = items.to_vec();
                // A copy of the last, or what the type starts one as.
                let item = items
                    .last()
                    .or_else(|| shapes.items.get(&shape(path, shapes)))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                grown.push(item);
                changed = Some(grown);
            }
        });
        ui.indent((label, path.len()), |ui| {
            for (index, item) in items.iter().enumerate() {
                path.push(index.to_string());
                let name = format!("#{index}");
                let mut remove = false;
                let option = shapes.options.contains_key(&shape(path, shapes));
                let mut remove_button = |ui: &mut Ui| {
                    remove = !fixed
                        && ui
                            .small_button("🗙")
                            .on_hover_text("Remove this item")
                            .clicked();
                };
                match item {
                    _ if option && fixed => self.node(ui, item, path, Some(&name)),
                    _ if option => {
                        row(ui, Some(&name), remove_button);
                        ui.indent(&name, |ui| self.node(ui, item, path, None));
                    }
                    Value::Object(_) => {
                        row(ui, Some(&name), remove_button);
                        ui.indent(&name, |ui| self.node(ui, item, path, None));
                    }
                    Value::Array(inner) if !is_vector(inner) => {
                        row(ui, Some(&name), remove_button);
                        ui.indent(&name, |ui| self.node(ui, item, path, Some("items")));
                    }
                    _ => row(ui, Some(&name), |ui| {
                        self.leaf(ui, item, path);
                        remove_button(ui);
                    }),
                }
                if remove {
                    let mut shrunk = items.to_vec();
                    shrunk.remove(index);
                    changed = Some(shrunk);
                }
                path.pop();
            }
        });
        if let Some(list) = changed {
            self.change = Some((path.clone(), Value::Array(list)));
        }
    }

    /// A map keyed by text: each entry under its key with a 🗙 that takes it
    /// out, and a key field with a ➕ that adds an entry by that key, which
    /// starts as the value type's default. A type with none takes no new
    /// entries.
    fn entries(
        &mut self,
        ui: &mut Ui,
        map: &serde_json::Map<String, Value>,
        path: &mut Vec<String>,
        label: &str,
        start: &Value,
    ) {
        let id = self.id.with(&path[..]).with("new key");
        let mut changed = None;
        row(ui, Some(label), |ui| {
            if start.is_null() {
                return;
            }
            let mut key: String = ui.data_mut(|data| data.get_temp(id)).unwrap_or_default();
            ui.add(egui::TextEdit::singleline(&mut key).hint_text("key").desired_width(80.0));
            let fresh = !key.is_empty() && !map.contains_key(&key);
            if ui
                .add_enabled(fresh, egui::Button::new("➕").small())
                .on_hover_text("Add an entry by this key")
                .clicked()
            {
                let mut grown = map.clone();
                grown.insert(std::mem::take(&mut key), start.clone());
                changed = Some(grown);
            }
            ui.data_mut(|data| data.insert_temp(id, key));
        });
        ui.indent((label, path.len()), |ui| {
            for (key, value) in map {
                path.push(key.clone());
                let mut remove = false;
                let mut remove_button = |ui: &mut Ui| {
                    remove = ui.small_button("🗙").on_hover_text("Remove this entry").clicked();
                };
                match value {
                    Value::Object(_) => {
                        row(ui, Some(key), remove_button);
                        ui.indent(key, |ui| self.node(ui, value, path, None));
                    }
                    Value::Array(inner) if !is_vector(inner) => {
                        row(ui, Some(key), remove_button);
                        ui.indent(key, |ui| self.node(ui, value, path, Some("items")));
                    }
                    _ => row(ui, Some(key), |ui| {
                        self.leaf(ui, value, path);
                        remove_button(ui);
                    }),
                }
                if remove {
                    let mut shrunk = map.clone();
                    shrunk.remove(key);
                    changed = Some(shrunk);
                }
                path.pop();
            }
        });
        if let Some(map) = changed {
            self.change = Some((path.clone(), Value::Object(map)));
        }
    }

    fn leaf(&mut self, ui: &mut Ui, node: &Value, path: &[String]) {
        let id = self.id.with(path);
        let euler_rotation = self.euler_rotation && path == ["rotation"];
        let mut change = |leaf: Value| self.change = Some((path.to_vec(), leaf));
        match node {
            Value::Number(number) => {
                if let Some(value) = number_field(ui, id, number, None) {
                    change(value);
                }
            }
            Value::Bool(checked) => {
                let mut checked = *checked;
                if ui.checkbox(&mut checked, "").changed() {
                    change(json!(checked));
                }
            }
            Value::String(text) => {
                // An enum of plain variants picks one from a list.
                if let Some(variants) = self.shapes.choices.get(&shape(path, self.shapes)) {
                    let mut picked = text.clone();
                    egui::ComboBox::from_id_salt(id)
                        .selected_text(text.as_str())
                        .show_ui(ui, |ui| {
                            for variant in variants {
                                ui.selectable_value(&mut picked, variant.clone(), variant);
                            }
                        });
                    if picked != *text {
                        change(json!(picked));
                    }
                } else if let Some(text) = commit_field(ui, id, text, None) {
                    change(json!(text));
                }
            }
            Value::Array(items)
                if (2..=4).contains(&items.len()) && items.iter().all(Value::is_number) =>
            {
                let numbers: Vec<_> = items.iter().filter_map(Value::as_f64).collect();
                if euler_rotation && numbers.len() == 4 {
                    let quat = [numbers[0], numbers[1], numbers[2], numbers[3]];
                    let angles = quat_to_euler(quat).map(|angle| json!(angle));
                    if let Some(v) = vector(ui, id, &angles, &["X", "Y", "Z"]) {
                        change(json!(euler_to_quat([v[0], v[1], v[2]])));
                    }
                } else if let Some(values) = vector(ui, id, items, &["x", "y", "z", "w"]) {
                    change(json!(values));
                }
            }
            Value::Null => read_only(ui, "None", TEXT),
            other => read_only(ui, &other.to_string(), TEXT),
        }
    }
}

/// One field per component; returns the whole new vector, as floats.
fn vector(ui: &mut Ui, id: Id, items: &[Value], labels: &[&str]) -> Option<Vec<f64>> {
    let mut changed = None;
    ui.horizontal(|ui| {
        let gap = ui.spacing().item_spacing.x;
        let width = (ui.available_width() - gap * (items.len() - 1) as f32) / items.len() as f32;
        for (i, number) in items.iter().filter_map(Value::as_number).enumerate() {
            ui.allocate_ui(egui::vec2(width, ui.spacing().interact_size.y), |ui| {
                if let Some(value) = number_field(ui, id.with(i), number, Some(labels[i])) {
                    let mut values: Vec<f64> = items.iter().filter_map(Value::as_f64).collect();
                    values[i] = value.as_f64().unwrap_or_default();
                    changed = Some(values);
                }
            });
        }
    });
    changed
}

/// A number field: an integer stays an integer, a float takes whatever is
/// typed. Text that is no such number puts the old value back.
pub(crate) fn number_field(
    ui: &mut Ui,
    id: Id,
    number: &serde_json::Number,
    label: Option<&str>,
) -> Option<Value> {
    let shown = format_number(number);
    let text = commit_field(ui, id, &shown, label)?;
    let typed = text.trim().parse::<f64>().ok().filter(|n| n.is_finite())?;
    let parsed = if number.is_f64() {
        serde_json::Number::from_f64(typed)?
    } else if typed.fract() != 0.0 {
        return None;
    } else if typed < 0.0 {
        (typed as i64).into()
    } else {
        (typed as u64).into()
    };
    (format_number(&parsed) != shown).then_some(Value::Number(parsed))
}

/// Three decimals, without trailing zeros: what fits in a field.
pub fn format_number(number: &serde_json::Number) -> String {
    match number.as_f64().filter(|_| number.is_f64()) {
        Some(float) => {
            let text = format!("{float:.3}");
            let text = text.trim_end_matches('0').trim_end_matches('.');
            if text == "-0" {
                "0".into()
            } else {
                text.into()
            }
        }
        None => number.to_string(),
    }
}

/// A text field that commits when it loses focus, Enter included, and shows
/// `value`, the game's newest, while nobody is typing in it. Returns the
/// typed text when it differs from `value`.
pub fn commit_field(ui: &mut Ui, id: Id, value: &str, label: Option<&str>) -> Option<String> {
    let typing = ui.data(|data| data.get_temp::<String>(id));
    let was_typing = typing.is_some();
    let mut text = typing.unwrap_or_else(|| value.to_owned());
    let mut field = TextEdit::singleline(&mut text)
        .id(id)
        .font(egui::TextStyle::Monospace)
        .desired_width(f32::INFINITY);
    if let Some(label) = label {
        field = field.prefix(RichText::new(label).small().color(MUTED));
    }
    if ui.add(field).has_focus() {
        ui.data_mut(|data| data.insert_temp(id, text));
        return None;
    }
    ui.data_mut(|data| data.remove_temp::<String>(id));
    (was_typing && text != value).then_some(text)
}

/// Monospace text that wraps anywhere: computed values like matrices have
/// no spaces to wrap at, and would widen the panel.
fn read_only(ui: &mut Ui, text: &str, color: egui::Color32) {
    let format = egui::TextFormat {
        font_id: egui::TextStyle::Monospace.resolve(ui.style()),
        color,
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::single_section(text.to_owned(), format);
    job.wrap.break_anywhere = true;
    ui.add(Label::new(job).wrap());
}

/// A field's name in a fixed column, then the field.
fn row(ui: &mut Ui, label: Option<&str>, add: impl FnOnce(&mut Ui)) {
    let Some(label) = label else {
        return add(ui);
    };
    ui.horizontal(|ui| {
        let size = egui::vec2(84.0, ui.spacing().interact_size.y);
        ui.allocate_ui_with_layout(
            size,
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(size.x);
                ui.add(Label::new(RichText::new(label).small().color(MUTED)).truncate());
            },
        );
        add(ui);
    });
}

/// The path of a field in its type, list items and map entries as `*`:
/// what [`Shapes`] keys by.
fn shape(path: &[String], shapes: &Shapes) -> String {
    let mut keys: Vec<&str> = Vec::new();
    for key in path {
        let entry = key.parse::<usize>().is_ok() || shapes.maps.contains_key(&keys.join("/"));
        keys.push(if entry { "*" } else { key });
    }
    keys.join("/")
}

/// A Bevy color, `{"Srgba": {"red": 1.0, "green": …, "blue": …, "alpha": …}}`
/// or the same in `LinearRgba`: its space and channels.
fn color_of(node: &Value) -> Option<(&str, [f32; 4])> {
    let map = node.as_object().filter(|map| map.len() == 1)?;
    let (space, channels) = map.iter().next()?;
    if !matches!(space.as_str(), "Srgba" | "LinearRgba") {
        return None;
    }
    let channel = |name: &str| channels.get(name)?.as_f64().map(|value| value as f32);
    Some((
        space,
        [channel("red")?, channel("green")?, channel("blue")?, channel("alpha")?],
    ))
}

/// The color `linear` picked, back in `space`, gamma encoded when `srgb`.
fn color_value(space: &str, mut linear: [f32; 4], srgb: bool) -> Value {
    if srgb {
        for channel in &mut linear[..3] {
            *channel = egui::ecolor::gamma_from_linear(*channel);
        }
    }
    let [red, green, blue, alpha] = linear;
    json!({ space: { "red": red, "green": green, "blue": blue, "alpha": alpha } })
}

/// Two to four numbers, which edit as one vector's fields rather than as a
/// list.
fn is_vector(items: &[Value]) -> bool {
    (2..=4).contains(&items.len()) && items.iter().all(Value::is_number)
}

/// A copy of `node` with the value at `path`, keys of nested objects and
/// indices of lists, replaced by `leaf`.
fn replace(node: &Value, path: &[String], leaf: Value) -> Value {
    let mut copy = node.clone();
    *path.iter().fold(&mut copy, |node, key| match node {
        Value::Array(items) => &mut items[key.parse::<usize>().expect("a list index")],
        _ => &mut node[key.as_str()],
    }) = leaf;
    copy
}

/// A quaternion `[x, y, z, w]` as Euler angles in degrees `[x, y, z]`:
/// Bevy's `EulerRot::YXZ`, yaw about Y, then pitch about X, then roll about
/// Z, the order Bevy's own examples use.
pub fn quat_to_euler([x, y, z, w]: [f64; 4]) -> [f64; 3] {
    // The rotation matrix entries the angles come from.
    let m02 = 2.0 * (x * z + w * y);
    let m22 = 1.0 - 2.0 * (x * x + y * y);
    let m12 = 2.0 * (y * z - w * x);
    let m10 = 2.0 * (x * y + w * z);
    let m11 = 1.0 - 2.0 * (x * x + z * z);
    [
        (-m12).clamp(-1.0, 1.0).asin(),
        m02.atan2(m22),
        m10.atan2(m11),
    ]
    .map(f64::to_degrees)
}

/// The quaternion `[x, y, z, w]` of [`quat_to_euler`]'s angles.
pub fn euler_to_quat(degrees: [f64; 3]) -> [f64; 4] {
    let [pitch, yaw, roll] = degrees.map(|angle| angle.to_radians() / 2.0);
    let multiply = |[ax, ay, az, aw]: [f64; 4], [bx, by, bz, bw]: [f64; 4]| {
        [
            aw * bx + ax * bw + ay * bz - az * by,
            aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw,
            aw * bw - ax * bx - ay * by - az * bz,
        ]
    };
    multiply(
        multiply(
            [0.0, yaw.sin(), 0.0, yaw.cos()],
            [pitch.sin(), 0.0, 0.0, pitch.cos()],
        ),
        [0.0, 0.0, roll.sin(), roll.cos()],
    )
}

#[cfg(test)]
mod tests {
    use egui_kittest::{Harness, kittest::Queryable};

    use super::*;

    #[test]
    fn adds_copies_and_removes_the_items_of_a_list() {
        let recipes = json!({"secs": 1.0, "recipes": [
            {"item": "axe", "needs": [{"item": "twig", "count": 3}]},
            {"item": "hammer", "needs": []},
        ]});
        let editor = |value: Value| {
            Harness::builder().build_ui_state(
                move |ui, changed: &mut Option<Value>| {
                    if let Some(value) = value_editor(ui, Id::new("recipes"), &value, false, &[]) {
                        *changed = Some(value);
                    }
                },
                None,
            )
        };
        let mut harness = editor(recipes.clone());
        harness.run();
        // The recipes' ➕ comes first, then the first recipe's needs'.
        harness.get_all_by_label("➕").next().unwrap().click();
        harness.run();
        let grown = harness.state().clone().unwrap();
        assert_eq!(grown["recipes"].as_array().unwrap().len(), 3);
        assert_eq!(grown["recipes"][2]["item"], "hammer", "a copy of the last");

        let mut harness = editor(recipes.clone());
        harness.run();
        harness.get_all_by_label("🗙").next().unwrap().click();
        harness.run();
        let shrunk = harness.state().clone().unwrap();
        assert_eq!(shrunk["recipes"], json!([{"item": "hammer", "needs": []}]));

        // An item's field edits in place.
        let changed = replace(&recipes, &["recipes".into(), "0".into(), "needs".into(), "0".into(), "count".into()], json!(5));
        assert_eq!(changed["recipes"][0]["needs"][0]["count"], 5);
        assert_eq!(changed["recipes"][1], recipes["recipes"][1]);
    }

    #[test]
    fn shows_bevy_colors_as_a_color_button() {
        let look = json!({"tint": {"Srgba": {"red": 0.5, "green": 0.25, "blue": 1.0, "alpha": 1.0}}, "size": 2.0});
        let mut harness = Harness::builder().build_ui(move |ui| {
            value_editor(ui, Id::new("look"), &look, false, &[]);
        });
        harness.run();
        assert!(harness.query_by_label("red").is_none(), "no channel fields");
        assert!(harness.query_by_label("tint").is_some());

        // Through the picker's linear color and back.
        let color = json!({"Srgba": {"red": 0.5, "green": 0.25, "blue": 1.0, "alpha": 0.8}});
        let (space, channels) = color_of(&color).unwrap();
        let linear = channels.map(egui::ecolor::linear_from_gamma);
        let back = color_value(space, [linear[0], linear[1], linear[2], 0.8], true);
        let red = back["Srgba"]["red"].as_f64().unwrap();
        assert!((red - 0.5).abs() < 1e-4, "{back}");
        assert!(color_of(&json!({"Hsla": {"hue": 1.0}})).is_none());
    }

    #[test]
    fn gives_an_option_a_value_or_takes_it_and_starts_new_items_by_their_type() {
        let shapes = Shapes {
            options: BTreeMap::from([("worn".to_owned(), json!({"item": "", "count": 1}))]),
            items: BTreeMap::from([("runes".to_owned(), json!(""))]),
            ..Default::default()
        };
        let editor = |value: Value, shapes: Shapes| {
            Harness::builder().build_ui_state(
                move |ui, changed: &mut Option<Value>| {
                    if let Some(value) = value_editor_with(ui, Id::new("slot"), &value, false, &[], &shapes) {
                        *changed = Some(value);
                    }
                },
                None,
            )
        };
        let mut harness = editor(json!({"worn": null, "runes": []}), shapes.clone());
        harness.run();
        // The worn slot's ➕ comes before the runes'.
        harness.get_all_by_label("➕").next().unwrap().click();
        harness.run();
        assert_eq!(harness.state().clone().unwrap()["worn"], json!({"item": "", "count": 1}));

        let mut harness = editor(json!({"worn": null, "runes": []}), shapes.clone());
        harness.run();
        harness.get_all_by_label("➕").nth(1).unwrap().click();
        harness.run();
        assert_eq!(harness.state().clone().unwrap()["runes"], json!([""]), "a text, not an object");

        let mut harness = editor(json!({"worn": {"item": "cloth", "count": 1}, "runes": []}), shapes);
        harness.run();
        harness.get_by_label("🗙").click();
        harness.run();
        assert_eq!(harness.state().clone().unwrap()["worn"], Value::Null);
    }

    #[test]
    fn adds_and_removes_the_entries_of_a_map_by_key() {
        let shapes = Shapes {
            maps: BTreeMap::from([("layers".to_owned(), json!([]))]),
            items: BTreeMap::from([("layers/*".to_owned(), json!({"bone": "", "angle": 0.0}))]),
            ..Default::default()
        };
        let pose = json!({"layers": {"lean": [{"bone": "Chest", "angle": 0.3}]}});
        let editor = |value: Value, shapes: Shapes| {
            Harness::builder().build_ui_state(
                move |ui, changed: &mut Option<Value>| {
                    if let Some(value) = value_editor_with(ui, Id::new("pose"), &value, false, &[], &shapes) {
                        *changed = Some(value);
                    }
                },
                None,
            )
        };
        // A typed key: the map's ➕, before the list's, adds an entry by it.
        let mut harness = editor(pose.clone(), shapes.clone());
        let key = Id::new("pose").with(&["layers".to_owned()][..]).with("new key");
        harness.ctx.data_mut(|data| data.insert_temp(key, "seat".to_owned()));
        harness.run();
        harness.get_all_by_label("➕").next().unwrap().click();
        harness.run();
        let grown = harness.state().clone().unwrap();
        assert_eq!(grown["layers"]["seat"], json!([]));
        assert_eq!(grown["layers"]["lean"], pose["layers"]["lean"]);

        let mut harness = editor(pose.clone(), shapes.clone());
        harness.run();
        harness.get_all_by_label("🗙").next().unwrap().click();
        harness.run();
        assert_eq!(harness.state().clone().unwrap()["layers"], json!({}));

        // Its entries' fields by their shape.
        let path = ["layers", "lean", "0", "bone"].map(String::from);
        assert_eq!(shape(&path, &shapes), "layers/*/*/bone");
    }

    #[test]
    fn keeps_an_arrays_length_and_gives_its_option_items_values() {
        let red = json!({"Srgba": {"red": 1.0, "green": 0.0, "blue": 0.0, "alpha": 1.0}});
        let shapes = Shapes {
            options: BTreeMap::from([("dyes/*".to_owned(), red.clone())]),
            fixed: ["dyes".to_owned()].into(),
            ..Default::default()
        };
        let value = json!({"dyes": [null, red.clone(), null]});
        let mut harness = Harness::builder().build_ui_state(
            move |ui, changed: &mut Option<Value>| {
                if let Some(value) = value_editor_with(ui, Id::new("look"), &value, false, &[], &shapes) {
                    *changed = Some(value);
                }
            },
            None,
        );
        harness.run();
        assert!(harness.query_by_label("Remove this item").is_none());
        // No ➕ for the array itself: the two are its None items'.
        assert_eq!(harness.get_all_by_label("➕").count(), 2);
        harness.get_all_by_label("➕").next().unwrap().click();
        harness.run();
        assert_eq!(harness.state().clone().unwrap()["dyes"], json!([red, red, null]));
    }

    #[test]
    fn picks_an_enum_variant_from_its_list() {
        let tools = json!({"tools": [{"kind": "Axe"}, {"kind": "Axe"}]});
        let shapes = Shapes {
            choices: BTreeMap::from([("tools/*/kind".to_owned(), vec!["Axe".to_owned(), "Bow".to_owned()])]),
            ..Default::default()
        };
        let mut harness = Harness::builder().build_ui_state(
            move |ui, changed: &mut Option<Value>| {
                if let Some(value) = value_editor_with(ui, Id::new("tools"), &tools, false, &[], &shapes) {
                    *changed = Some(value);
                }
            },
            None,
        );
        harness.run();
        harness.get_all_by_role(egui::accesskit::Role::ComboBox).nth(1).unwrap().click();
        harness.run();
        harness.get_by_label("Bow").click();
        harness.run();
        let picked = harness.state().clone().unwrap();
        assert_eq!(picked, json!({"tools": [{"kind": "Axe"}, {"kind": "Bow"}]}));
    }

    fn close(a: &[f64], b: &[f64], epsilon: f64) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < epsilon)
    }

    #[test]
    fn reads_bevy_rotations_as_yaw_pitch_and_roll_in_degrees() {
        // Quat::from_rotation_y(90°) and Quat::from_rotation_x(90°).
        let half = std::f64::consts::FRAC_1_SQRT_2;
        assert!(close(
            &quat_to_euler([0.0, half, 0.0, half]),
            &[0.0, 90.0, 0.0],
            1e-4
        ));
        assert!((quat_to_euler([half, 0.0, 0.0, half])[0] - 90.0).abs() < 1e-3);
        assert_eq!(quat_to_euler([0.0, 0.0, 0.0, 1.0]), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn turns_angles_back_into_the_same_rotation() {
        for angles in [[10.0, 20.0, 30.0], [-45.0, 170.0, -5.0], [0.0, -90.0, 0.0]] {
            assert!(
                close(&quat_to_euler(euler_to_quat(angles)), &angles, 1e-6),
                "{angles:?}"
            );
        }
        // The template camera's looking_at rotation survives a round trip.
        let camera = [-0.22055435, -0.13167094, -0.030063398, 0.9659786];
        assert!(close(&euler_to_quat(quat_to_euler(camera)), &camera, 1e-6));
    }

    #[test]
    fn shows_three_decimals_and_replaces_one_field() {
        let number = |value: Value| format_number(value.as_number().unwrap());
        assert_eq!(number(json!(1.0)), "1");
        assert_eq!(number(json!(10.5)), "10.5");
        assert_eq!(number(json!(-0.0001)), "0");
        assert_eq!(number(json!(0.12345)), "0.123");
        assert_eq!(number(json!(7)), "7");

        let value = json!({"translation": [0.0, 1.0, 2.0], "inner": {"flag": true, "n": 1}});
        let changed = replace(&value, &["inner".into(), "n".into()], json!(5));
        assert_eq!(
            changed,
            json!({"translation": [0.0, 1.0, 2.0], "inner": {"flag": true, "n": 5}})
        );
        let keys: Vec<_> = changed.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["translation", "inner"], "field order is kept");
    }

    #[test]
    fn commits_a_field_once_when_it_loses_focus() {
        let ctx = egui::Context::default();
        let id = Id::new("component");
        let mut value = json!({"n": 1.0, "flag": true});
        let frame = |events: Vec<egui::Event>, value: &Value| {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let mut changed = None;
            let mut output = ctx.run_ui(input, |ui| {
                changed = value_editor(ui, id, value, false, &[])
            });
            // Nothing renders here, so the font texture goes unused.
            output.textures_delta.clear();
            changed
        };
        let field = id.with(&["n".to_string()][..]);
        ctx.memory_mut(|memory| memory.request_focus(field));
        assert_eq!(frame(vec![], &value), None);
        assert_eq!(
            frame(vec![egui::Event::Text("5".into())], &value),
            None,
            "typing"
        );
        // The game's newer value does not replace what is being typed.
        value["n"] = json!(3.0);
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        let committed = frame(vec![enter], &value);
        assert_eq!(
            committed,
            Some(json!({"n": 15.0, "flag": true})),
            "Enter commits"
        );
        assert_eq!(frame(vec![], &committed.unwrap()), None, "once");
    }
}
