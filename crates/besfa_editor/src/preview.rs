//! The model preview tab: a glTF model from the Assets tab on its own in
//! the viewport, with its clips, parts, light and views, screenshots of it,
//! and a part's components, which pose it. The game loads, poses and
//! lights it; this side keeps the controls and tells the game when they
//! change. The clip's clock runs on both sides by the same rule, so the time
//! shown is never reported.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::Instant,
};

use besfa_protocol::{
    ClipInfo, Command, ComponentData, EntityInfo, LightPreset, ModelStats, Overlays, PreviewModel,
    View, clip_time,
};
use eframe::egui::{self, Button, Color32, DragValue, RichText, ScrollArea, Ui};
use serde_json::{Value, json};

use crate::{
    inspector::{Action, Inspected, Inspector, TRANSFORM, euler_to_quat, quat_to_euler},
    panels::{ACCENT, ERROR, MUTED, TEXT, panel_header, section, tag},
    project,
    review::{Change, Edits, ROOT},
    scene::{Scene, tree_rows},
};

const VIEWS: [(View, &str, &str); 6] = [
    (
        View::Front,
        "Front",
        "Look at the model's front, its -Z side",
    ),
    (View::Back, "Back", "Look at the model's back"),
    (View::Left, "Left", "Look at the model's left side"),
    (
        View::Right,
        "Right",
        "Look at the model's right side, its +X side",
    ),
    (View::Top, "Top", "Look down on the model"),
    (
        View::ThreeQuarter,
        "¾",
        "Look at the model's front from above and to its right",
    ),
];

/// Where screenshots go, in the project.
const SCREENSHOTS: &str = "screenshots";

const LIGHTS: [(LightPreset, &str); 3] = [
    (LightPreset::Studio, "Studio"),
    (LightPreset::Daylight, "Daylight"),
    (LightPreset::Night, "Night"),
];

const SPEEDS: [f32; 4] = [0.25, 0.5, 1.0, 2.0];

/// What the game reported once the model loaded.
struct Loaded {
    model: PreviewModel,
    /// Each part's path of names from the model's root, which a part keeps
    /// when the model loads again and its id changes.
    paths: HashMap<u64, String>,
}

pub struct ModelPreview {
    /// Relative to the asset folder.
    pub path: String,
    project: PathBuf,
    loaded: Option<Loaded>,
    clip: Option<usize>,
    /// No clip: the model as it loaded, to pose by hand.
    rest: bool,
    playing: bool,
    /// The chosen clip's length, apart from `loaded`, which the panel
    /// takes while it shows.
    duration: f32,
    /// The clip's time at `since`.
    time: f32,
    since: Instant,
    speed: f32,
    looping: bool,
    /// Parts by path; see [`Loaded::paths`].
    hidden: HashSet<String>,
    solo: Option<String>,
    light: LightPreset,
    view: View,
    overlays: Overlays,
    /// The number of the last screenshot asked for.
    shots: u32,
    /// The last screenshot's path, or why it was not saved.
    shot: Option<Result<String, String>>,
    /// The part whose components show, by path; [`ROOT`] for the root.
    selected: Option<String>,
    /// The selected part's components, as the game reported them.
    components: Vec<ComponentData>,
    inspector: Inspector,
    edits: Edits,
    /// The name field of Save pose.
    pose_name: String,
    /// Make the panel's changes again once the model is back in rest pose.
    replay: bool,
}

impl ModelPreview {
    /// Previews the model at `path` of the project in the `project` folder.
    pub fn new(path: String, project: PathBuf) -> Self {
        Self {
            path,
            project,
            loaded: None,
            clip: None,
            rest: false,
            playing: true,
            duration: 0.0,
            time: 0.0,
            since: Instant::now(),
            speed: 1.0,
            looping: true,
            hidden: HashSet::new(),
            solo: None,
            light: LightPreset::default(),
            view: View::default(),
            overlays: Overlays::default(),
            shots: 0,
            shot: None,
            selected: None,
            components: Vec::new(),
            inspector: Inspector::default(),
            edits: Edits::default(),
            pose_name: String::new(),
            replay: false,
        }
    }

    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// What shows the model the way this tab has it, in a game that may not
    /// have it loaded. The game reports it again, which sends the rest.
    pub fn commands(&mut self) -> Vec<Command> {
        self.loaded = None;
        vec![
            Command::Preview {
                path: Some(self.path.clone()),
            },
            Command::PreviewLight { light: self.light },
            Command::PreviewView { view: self.view },
            Command::PreviewOverlays(self.overlays),
        ]
    }

    /// Takes the game's report on the model, keeping the choices that still
    /// apply, and returns what puts the game's model in step with them.
    pub fn loaded(&mut self, model: PreviewModel) -> Vec<Command> {
        // Part ids are the game's entities, new each time the model loads;
        // their paths stay unless the file renamed them.
        let paths = part_paths(&model.parts);
        let known: HashSet<&String> = paths.values().collect();
        self.hidden.retain(|path| known.contains(path));
        self.solo = self.solo.take().filter(|path| known.contains(path));
        let clips = &model.clips;
        if self.clip.is_none_or(|clip| clip >= clips.len()) {
            self.clip = (!clips.is_empty()).then_some(0);
            self.rebase(0.0);
        }
        self.duration = self
            .clip
            .and_then(|clip| clips.get(clip))
            .map_or(0.0, |clip| clip.duration);
        self.loaded = Some(Loaded { model, paths });
        self.components.clear();
        let loaded = self.loaded.as_ref().unwrap();
        let mut commands = vec![self.animation(), self.parts(loaded)];
        // The model is new: what the panel changed applies again, and the
        // part shown is picked by name.
        commands.extend(self.edits.replay(|path| self.id(path)));
        if self
            .selected
            .as_deref()
            .and_then(|path| self.id(path))
            .is_none()
        {
            self.selected = None;
        }
        let id = self.selected.as_deref().and_then(|path| self.id(path));
        commands.push(Command::PreviewSelect { id });
        commands
    }

    /// The id the game gave the part at `path` this time the model loaded.
    fn id(&self, path: &str) -> Option<u64> {
        let loaded = self.loaded.as_ref()?;
        if path == ROOT {
            return loaded.model.root;
        }
        loaded
            .paths
            .iter()
            .find_map(|(&id, part)| (part == path).then_some(id))
    }

    /// Takes the game's report on an entity if it is the selected part's,
    /// unless it was written before the latest change of `changes` sent.
    pub fn entity_report(
        &mut self,
        id: u64,
        reported_changes: Option<u64>,
        components: Vec<ComponentData>,
        changes: u64,
    ) {
        let selected = self.selected.as_deref().and_then(|path| self.id(path));
        if selected == Some(id) && reported_changes.unwrap_or(changes) >= changes {
            self.components = components;
        }
    }

    /// Shows a `set` of the selected part's component at once, so a change
    /// made before the game reports back starts from it.
    pub fn sent(&mut self, command: &Command) {
        let selected = self.selected.as_deref().and_then(|path| self.id(path));
        if let Command::Set {
            id,
            component,
            value,
        } = command
            && Some(*id) == selected
        {
            for reported in &mut self.components {
                if reported.path == *component {
                    reported.value = Some(value.clone()).filter(|value| !value.is_null());
                }
            }
        }
    }

    /// The selected part's reported value of `component`.
    fn current(&self, component: &str) -> Option<Value> {
        let reported = self.components.iter().find(|c| c.path == component);
        reported.and_then(|reported| reported.value.clone())
    }

    /// Changes a component of the selected part, keeping the change so it
    /// applies whenever the model loads.
    fn change(&mut self, action: Action) -> Vec<Command> {
        let Some(path) = self.selected.clone() else {
            return Vec::new();
        };
        let Some(id) = self.id(&path) else {
            return Vec::new();
        };
        let (component, change) = match action {
            Action::Set(component, value) => (component, Change::Set(value)),
            Action::Insert(component) => (component, Change::Insert),
            Action::Remove(component) => (component, Change::Remove),
            Action::ApplyPrefab(_) => return Vec::new(),
        };
        let before = self.current(&component);
        self.edits.record(&path, &component, before, change.clone());
        let command = match change {
            Change::Set(value) => Command::Set {
                id,
                component,
                value,
            },
            Change::Insert => Command::Insert { id, component },
            Change::Remove => Command::Remove { id, component },
        };
        vec![command]
    }

    fn now(&self) -> f32 {
        let speed = if self.playing { self.speed } else { 0.0 };
        let seconds = self.since.elapsed().as_secs_f32();
        clip_time(self.time, seconds, speed, self.duration, self.looping)
    }

    /// Notes the game's answer to a screenshot.
    pub fn screenshot(&mut self, path: String, error: Option<String>) {
        self.shot = Some(match error {
            None => Ok(path),
            Some(error) => Err(error),
        });
    }

    /// The next free `screenshots/<model>-<n>.png`, counting on from the
    /// last one asked for, which the game may not have written yet.
    fn next_screenshot(&mut self) -> String {
        let name = self.name().to_owned();
        let stem = name
            .rsplit_once('.')
            .map_or(name.as_str(), |(stem, _)| stem);
        let path = |n| format!("{SCREENSHOTS}/{stem}-{n}.png");
        let mut n = self.shots + 1;
        while self.project.join(path(n)).exists() {
            n += 1;
        }
        self.shots = n;
        path(n)
    }

    /// Restarts the clock at `time`, before a control changes how it runs.
    fn rebase(&mut self, time: f32) {
        self.time = time;
        self.since = Instant::now();
    }

    fn animation(&self) -> Command {
        Command::PreviewAnimation {
            clip: self.clip.filter(|_| !self.rest),
            time: self.now(),
            speed: if self.playing { self.speed } else { 0.0 },
            looping: self.looping,
        }
    }

    fn parts(&self, loaded: &Loaded) -> Command {
        let id = |wanted: &String| {
            loaded
                .paths
                .iter()
                .find_map(|(&id, path)| (path == wanted).then_some(id))
        };
        Command::PreviewParts {
            hidden: self.hidden.iter().filter_map(id).collect(),
            solo: self.solo.as_ref().and_then(id),
        }
    }

    /// The tab's side panel; returns what to tell the game. The `scene`
    /// says what components the game can add.
    pub fn ui(&mut self, ui: &mut Ui, scene: &Scene) -> Vec<Command> {
        let mut send = Vec::new();
        let mut show_all = false;
        panel_header(ui, "Model", |ui| {
            show_all = (!self.hidden.is_empty() || self.solo.is_some())
                && ui
                    .small_button("👁")
                    .on_hover_text("Show all parts")
                    .clicked();
        });
        if show_all {
            self.hidden.clear();
            self.solo = None;
            send.extend(self.loaded.as_ref().map(|loaded| self.parts(loaded)));
        }
        // Put back below; taken so the controls can change the rest.
        let Some(loaded) = self.loaded.take() else {
            ui.label(RichText::new(self.name()).strong().color(TEXT));
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(12.0));
                tag(ui, "Loading");
            });
            return send;
        };
        let mut loaded_back = None;
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let model = &loaded.model;
            ui.label(RichText::new(self.name()).strong().color(TEXT));
            if let Some(error) = &model.error {
                ui.label(RichText::new(error).color(ERROR));
                loaded_back = Some(loaded);
                return;
            }
            if let Some([width, height, depth]) = model.size {
                tag(ui, &format!("{width:.2} × {height:.2} × {depth:.2} m"));
            }
            if let Some(stats) = &model.stats {
                stats_ui(ui, stats, model.joints.len());
            }
            self.animation_ui(ui, &model.clips, &mut send);
            self.view_ui(ui, &mut send);
            self.overlays_ui(ui, model, &mut send);
            section(ui, "Light");
            ui.horizontal(|ui| {
                for (light, label) in LIGHTS {
                    if ui.selectable_label(self.light == light, label).clicked() {
                        self.light = light;
                        send.push(Command::PreviewLight { light });
                    }
                }
            });
            section(ui, "Parts");
            self.parts_ui(ui, &loaded, &mut send);
            // Back, for what goes by part path.
            self.loaded = Some(loaded);
            if std::mem::take(&mut self.replay) {
                send.extend(self.edits.replay(|path| self.id(path)));
            }
            self.selected_ui(ui, scene, &mut send);
            self.poses_ui(ui, &mut send);
        });
        // An error report returns early.
        if let Some(loaded) = loaded_back {
            self.loaded = Some(loaded);
        }
        send
    }

    /// The selected part's name, its transform as drag fields for posing,
    /// and its components.
    fn selected_ui(&mut self, ui: &mut Ui, scene: &Scene, send: &mut Vec<Command>) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        let loaded = self.loaded.as_ref().unwrap();
        let id = self.id(&path);
        // The root is not a part; it goes by the model's name.
        let entity = match (id, path == ROOT) {
            (Some(id), true) => Some(EntityInfo {
                id,
                name: Some(format!("{} (root)", self.name())),
                parent: None,
                scene: true,
                asset: None,
            }),
            (id, _) => id
                .and_then(|id| loaded.model.parts.iter().find(|part| part.id == id))
                .cloned(),
        };
        ui.horizontal(|ui| {
            section(ui, "Selected");
            self.inspector.details_button(ui);
        });
        let inspected = Inspected {
            selected: id,
            entity: entity.as_ref(),
            placement: None,
            components: &self.components,
            systems: &[],
            addable: &scene.addable,
            crate_name: scene.crate_name.as_deref(),
        };
        let name = self.inspector.name(ui, &inspected);
        let pose = self.pose_ui(ui);
        let action = self.inspector.components(ui, &inspected, false).or(name);
        if let Some(value) = pose {
            send.extend(self.change(Action::Set(TRANSFORM.into(), value)));
        }
        if let Some(action) = action {
            send.extend(self.change(action));
        }
    }

    /// Drag fields for the selected part's transform; the new transform
    /// while one is dragged.
    fn pose_ui(&self, ui: &mut Ui) -> Option<Value> {
        let value = self.current(TRANSFORM)?;
        let read = |field: &str| -> Option<Vec<f64>> {
            value
                .get(field)?
                .as_array()?
                .iter()
                .map(Value::as_f64)
                .collect()
        };
        let mut translation: [f64; 3] = read("translation")?.try_into().ok()?;
        let rotation: [f64; 4] = read("rotation")?.try_into().ok()?;
        let mut scale: [f64; 3] = read("scale")?.try_into().ok()?;
        // Plus zero turns -0.0 into 0.0 for the field.
        let mut angles = quat_to_euler(rotation).map(|angle| angle + 0.0);
        let mut changed = false;
        let mut row =
            |ui: &mut Ui, label: &str, values: &mut [f64; 3], speed: f64, suffix: &str| {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [64.0, 18.0],
                        egui::Label::new(RichText::new(label).small().color(MUTED)),
                    );
                    for value in values {
                        changed |= ui
                            .add(
                                DragValue::new(value)
                                    .speed(speed)
                                    .max_decimals(3)
                                    .suffix(suffix),
                            )
                            .changed();
                    }
                });
            };
        row(ui, "Position", &mut translation, 0.005, "");
        row(ui, "Rotation", &mut angles, 0.5, "°");
        row(ui, "Scale", &mut scale, 0.005, "");
        if !changed {
            return None;
        }
        let mut value = value.clone();
        value["translation"] = json!(translation);
        value["rotation"] = json!(euler_to_quat(angles));
        value["scale"] = json!(scale);
        Some(value)
    }

    /// Named poses of the parts' transforms, and taking changes back.
    fn poses_ui(&mut self, ui: &mut Ui, send: &mut Vec<Command>) {
        section(ui, "Poses");
        let mut edits = std::mem::take(&mut self.edits);
        let mut pose_name = std::mem::take(&mut self.pose_name);
        let id = |path: &str| self.id(path);
        ui.horizontal(|ui| {
            if ui
                .button("Rest")
                .on_hover_text("Put every part back the way the model loaded")
                .clicked()
            {
                send.extend(edits.rest(id));
            }
            ui.add(
                egui::TextEdit::singleline(&mut pose_name)
                    .hint_text("Pose name")
                    .desired_width(110.0),
            );
            let name = pose_name.trim().to_owned();
            if ui
                .add_enabled(!name.is_empty(), Button::new("Save pose"))
                .on_hover_text("Keep the parts' changed transforms under this name")
                .clicked()
            {
                edits.save_pose(&name);
            }
        });
        let mut apply = None;
        let mut delete = None;
        for (index, pose) in edits.poses.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .small_button("▶")
                    .on_hover_text("Pose the model")
                    .clicked()
                {
                    apply = Some(index);
                }
                ui.label(RichText::new(&pose.name).color(TEXT));
                tag(ui, &format!("{} parts", pose.transforms.len()));
                if ui
                    .small_button("🗙")
                    .on_hover_text("Forget the pose")
                    .clicked()
                {
                    delete = Some(index);
                }
            });
        }
        if let Some(index) = apply {
            send.extend(edits.apply_pose(index, id));
        }
        if let Some(index) = delete {
            edits.poses.remove(index);
        }
        if !edits.changes.is_empty() {
            let count = edits.changes.len();
            if ui
                .button(format!(
                    "Revert {count} change{}",
                    if count == 1 { "" } else { "s" }
                ))
                .on_hover_text("Take back every change the panel made to the model")
                .clicked()
            {
                send.extend(edits.revert_all(id));
            }
        }
        self.edits = edits;
        self.pose_name = pose_name;
    }

    fn animation_ui(&mut self, ui: &mut Ui, clips: &[ClipInfo], send: &mut Vec<Command>) {
        section(ui, "Animation");
        let Some(clip) = self.clip.and_then(|clip| clips.get(clip)) else {
            tag(ui, "No clips");
            return;
        };
        let mut changed = false;
        const REST: &str = "None (rest pose)";
        egui::ComboBox::from_id_salt("preview_clip")
            .selected_text(if self.rest { REST } else { &clip.name })
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                if ui.selectable_label(self.rest, REST).clicked() && !self.rest {
                    self.rest = true;
                    // The rest pose undoes poses too.
                    self.replay = true;
                    changed = true;
                }
                for (index, clip) in clips.iter().enumerate() {
                    if ui
                        .selectable_label(!self.rest && self.clip == Some(index), &clip.name)
                        .clicked()
                    {
                        self.clip = Some(index);
                        self.rest = false;
                        self.duration = clip.duration;
                        self.rebase(0.0);
                        changed = true;
                    }
                }
            });
        if self.rest {
            tag(ui, "The model as it loaded, to pose by hand.");
            if changed {
                send.push(self.animation());
            }
            return;
        }
        let duration = clip.duration;
        let mut time = self.now();
        ui.horizontal(|ui| {
            let (icon, tip) = if self.playing {
                ("⏸", "Pause")
            } else {
                ("▶", "Play")
            };
            if ui.button(icon).on_hover_text(tip).clicked() {
                self.rebase(time);
                self.playing = !self.playing;
                changed = true;
            }
            if ui.button("⏮").on_hover_text("Back to the start").clicked() {
                self.rebase(0.0);
                time = 0.0;
                changed = true;
            }
            ui.label(
                RichText::new(format!("{time:.2} / {duration:.2} s"))
                    .small()
                    .color(MUTED),
            );
        });
        ui.spacing_mut().slider_width = ui.available_width();
        let slider = egui::Slider::new(&mut time, 0.0..=duration).show_value(false);
        // Scrubbing holds the clip where it is dropped.
        if ui.add(slider).changed() {
            self.playing = false;
            self.rebase(time);
            changed = true;
        }
        ui.horizontal(|ui| {
            for speed in SPEEDS {
                if ui
                    .selectable_label(self.speed == speed, format!("{speed}×"))
                    .clicked()
                {
                    self.rebase(time);
                    self.speed = speed;
                    changed = true;
                }
            }
            if ui.checkbox(&mut self.looping, "Loop").changed() {
                self.rebase(time);
                changed = true;
            }
        });
        if changed {
            send.push(self.animation());
        }
    }

    fn view_ui(&mut self, ui: &mut Ui, send: &mut Vec<Command>) {
        section(ui, "View");
        ui.horizontal_wrapped(|ui| {
            for (view, label, tip) in VIEWS {
                if ui.button(label).on_hover_text(tip).clicked() {
                    self.view = view;
                    send.push(Command::PreviewView { view });
                }
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("📷 Screenshot")
                .on_hover_text(format!("Save what the viewport shows to {SCREENSHOTS}/"))
                .clicked()
            {
                let path = self.next_screenshot();
                send.push(Command::PreviewScreenshot { path });
            }
            match &self.shot {
                Some(Ok(path)) => {
                    if ui
                        .small_button("🗁")
                        .on_hover_text("Open the screenshots folder")
                        .clicked()
                    {
                        project::reveal(&self.project.join(SCREENSHOTS).to_string_lossy());
                    }
                    tag(ui, path);
                }
                Some(Err(error)) => {
                    ui.label(RichText::new(error).small().color(ERROR));
                }
                None => {}
            }
        });
    }

    fn overlays_ui(&mut self, ui: &mut Ui, model: &PreviewModel, send: &mut Vec<Command>) {
        section(ui, "Overlays");
        let before = self.overlays;
        ui.horizontal_wrapped(|ui| {
            let skeleton = format!("Skeleton ({})", model.joints.len());
            ui.add_enabled(
                !model.joints.is_empty(),
                egui::Checkbox::new(&mut self.overlays.skeleton, skeleton),
            );
            let markers = format!("Markers ({})", model.markers.len());
            ui.add_enabled(
                !model.markers.is_empty(),
                egui::Checkbox::new(&mut self.overlays.markers, markers),
            )
            .on_hover_text(
                "Parts that mark a place: leaves without a mesh, like Grip or ShotPoint",
            );
            ui.checkbox(&mut self.overlays.bounds, "Bounds");
        });
        if self.overlays != before {
            send.push(Command::PreviewOverlays(self.overlays));
        }
        if model.markers.is_empty() {
            return;
        }
        let name = |id| {
            model
                .parts
                .iter()
                .find(|part| part.id == id)
                .map_or_else(String::new, EntityInfo::label)
        };
        egui::CollapsingHeader::new(RichText::new("Marker positions").small().color(MUTED))
            .id_salt("preview_markers")
            .show(ui, |ui| {
                for marker in &model.markers {
                    let [x, y, z] = marker.position;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name(marker.id)).small().color(TEXT));
                        tag(ui, &format!("{x:.3}, {y:.3}, {z:.3}"));
                    });
                }
            });
    }

    fn parts_ui(&mut self, ui: &mut Ui, loaded: &Loaded, send: &mut Vec<Command>) {
        let model = &loaded.model;
        let markers: HashSet<u64> = model.markers.iter().map(|marker| marker.id).collect();
        let joints: HashSet<u64> = model.joints.iter().copied().collect();
        let mut changed = false;
        let mut pick = None;
        let changed_parts: HashSet<&str> = self
            .edits
            .changes
            .keys()
            .map(|(path, _)| path.as_str())
            .collect();
        // The root takes components the way a placed model's root does.
        let root = Some(ROOT.to_owned());
        let root_label = RichText::new(format!("⛶ {}", self.name())).color(TEXT);
        if ui
            .selectable_label(self.selected == root, root_label)
            .on_hover_text("The entity the model spawns under: add the game's components here")
            .clicked()
        {
            pick = Some((self.selected != root).then_some(ROOT.to_owned()));
        }
        if model.parts.is_empty() {
            tag(ui, "No parts");
        }
        let rows = tree_rows(&model.parts);
        // Long part lists scroll on their own, so what follows stays near.
        ScrollArea::vertical()
            .id_salt("preview_parts")
            .max_height(260.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (part, depth) in rows {
                    let path = &loaded.paths[&part.id];
                    let hidden = self.hidden.contains(path);
                    ui.horizontal(|ui| {
                        ui.add_space(depth as f32 * 16.0);
                        let eye = RichText::new("👁").color(if hidden { MUTED } else { TEXT });
                        if ui
                            .add(Button::new(eye).small().frame(false))
                            .on_hover_text(if hidden { "Show" } else { "Hide" })
                            .clicked()
                        {
                            if hidden {
                                self.hidden.remove(path);
                            } else {
                                self.hidden.insert(path.clone());
                            }
                            changed = true;
                        }
                        let solo = self.solo.as_ref() == Some(path);
                        if ui
                            .add(Button::selectable(solo, "S").small())
                            .on_hover_text("Show only this part")
                            .clicked()
                        {
                            self.solo = (!solo).then(|| path.clone());
                            changed = true;
                        }
                        let mut label = RichText::new(part.label()).color(TEXT);
                        if hidden {
                            label = label.strikethrough().color(MUTED);
                        }
                        if changed_parts.contains(path.as_str()) {
                            label = label.color(ACCENT);
                        }
                        let selected = self.selected.as_ref() == Some(path);
                        if ui
                            .selectable_label(selected, label)
                            .on_hover_text("Show its components and pose it")
                            .clicked()
                        {
                            pick = Some((!selected).then(|| path.clone()));
                        }
                        if markers.contains(&part.id) {
                            tag(ui, "marker");
                        } else if joints.contains(&part.id) {
                            tag(ui, "joint");
                        }
                    });
                }
            });
        if changed {
            send.push(self.parts(loaded));
        }
        if let Some(path) = pick {
            send.push(self.select_in(loaded, path));
        }
    }

    /// Shows the components of the part at `path`, while `loaded` is out
    /// of `self`.
    fn select_in(&mut self, loaded: &Loaded, path: Option<String>) -> Command {
        let id = path.as_deref().and_then(|path| {
            if path == ROOT {
                loaded.model.root
            } else {
                loaded
                    .paths
                    .iter()
                    .find_map(|(&id, part)| (part == path).then_some(id))
            }
        });
        self.selected = path;
        self.components.clear();
        Command::PreviewSelect { id }
    }
}

/// Counts of what the model is made of, and its materials.
fn stats_ui(ui: &mut Ui, stats: &ModelStats, joints: usize) {
    let mut counts = format!(
        "{} triangles · {} vertices · {} mesh{}",
        stats.triangles,
        stats.vertices,
        stats.meshes,
        plural(stats.meshes)
    );
    if joints > 0 {
        counts += &format!(" · {joints} joints");
    }
    tag(ui, &counts);
    let title = format!("Materials ({})", stats.materials.len());
    egui::CollapsingHeader::new(RichText::new(title).small().color(MUTED))
        .id_salt("preview_materials")
        .show(ui, |ui| {
            for material in &stats.materials {
                ui.horizontal(|ui| {
                    let [r, g, b, a] = material.color.map(|c| (c.clamp(0.0, 1.0) * 255.0) as u8);
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        rect,
                        2.0,
                        Color32::from_rgba_unmultiplied(r, g, b, a),
                    );
                    let name = material.name.as_deref().unwrap_or("Unnamed");
                    ui.label(RichText::new(name).small().color(TEXT))
                        .on_hover_text(format!("#{r:02X}{g:02X}{b:02X}, alpha {a}"));
                    let meshes = material.meshes;
                    let mut notes = vec![format!("{meshes} mesh{}", plural(meshes))];
                    notes.extend(
                        material
                            .textures
                            .iter()
                            .map(|texture| texture.replace('_', " ")),
                    );
                    tag(ui, &notes.join(" · "));
                });
            }
        });
}

fn plural(count: u32) -> &'static str {
    if count == 1 { "" } else { "es" }
}

/// Each part's names from the model's root down to it, joined by `/`.
/// glTF exporters keep node names unique, so this names a part across
/// loads; an unnamed part goes by the label of its id.
fn part_paths(parts: &[EntityInfo]) -> HashMap<u64, String> {
    let by_id: HashMap<u64, &EntityInfo> = parts.iter().map(|part| (part.id, part)).collect();
    parts
        .iter()
        .map(|part| {
            let mut names = Vec::new();
            let mut current = Some(part);
            // Bounded, so a parent cycle, a game bug, cannot hang the editor.
            for _ in 0..=parts.len() {
                let Some(part) = current else { break };
                names.push(part.label());
                current = part.parent.and_then(|parent| by_id.get(&parent).copied());
            }
            names.reverse();
            (part.id, names.join("/"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(id: u64, name: &str, parent: Option<u64>) -> EntityInfo {
        EntityInfo {
            id,
            name: Some(name.into()),
            parent,
            scene: true,
            asset: None,
        }
    }

    fn model(clips: Vec<ClipInfo>, parts: Vec<EntityInfo>) -> PreviewModel {
        PreviewModel {
            clips,
            parts,
            ..PreviewModel::default()
        }
    }

    fn clip(name: &str) -> ClipInfo {
        ClipInfo {
            name: name.into(),
            duration: 1.0,
        }
    }

    #[test]
    fn shows_the_clip_time_while_the_panel_holds_the_report() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut preview = ModelPreview::new("models/hero.glb".into(), PathBuf::new());
        preview.loaded(model(vec![clip("Idle")], vec![]));
        preview.playing = false;
        preview.rebase(0.5);
        let mut harness = Harness::new_ui_state(
            |ui, preview: &mut ModelPreview| {
                preview.ui(ui, &Scene::default());
            },
            preview,
        );
        harness.run();
        harness.get_by_label("0.50 / 1.00 s");
    }

    #[test]
    fn keeps_choices_that_still_apply_when_the_model_reloads() {
        let mut preview = ModelPreview::new("models/hero.glb".into(), PathBuf::new());
        assert_eq!(preview.name(), "hero.glb");
        let parts = |body, head, hat| {
            vec![
                part(body, "Body", None),
                part(head, "Head", Some(body)),
                part(hat, "Hat", Some(head)),
            ]
        };
        let sent = preview.loaded(model(vec![clip("Idle"), clip("Walk")], parts(1, 2, 3)));
        assert_eq!(preview.clip, Some(0), "the first clip plays");
        assert!(matches!(
            sent[0],
            Command::PreviewAnimation { clip: Some(0), .. }
        ));

        preview.clip = Some(1);
        preview.hidden.insert("Body/Head/Hat".into());
        preview.solo = Some("Body/Head".into());
        let commands = preview.commands();
        assert!(
            matches!(&commands[0], Command::Preview { path: Some(path) } if path == "models/hero.glb")
        );
        assert!(preview.loaded.is_none(), "waits for the game's report");

        // Loaded again: the same clips and parts under new ids.
        let sent = preview.loaded(model(vec![clip("Idle"), clip("Walk")], parts(4, 5, 6)));
        assert_eq!(preview.clip, Some(1));
        assert_eq!(
            sent[1],
            Command::PreviewParts {
                hidden: vec![6],
                solo: Some(5)
            }
        );

        // Re-exported without the hat.
        let sent = preview.loaded(model(vec![clip("Idle")], parts(7, 8, 9)[..2].to_vec()));
        assert!(preview.hidden.is_empty() && preview.solo.as_deref() == Some("Body/Head"));
        assert_eq!(
            sent[1],
            Command::PreviewParts {
                hidden: vec![],
                solo: Some(8)
            }
        );

        preview.loaded(model(vec![], vec![]));
        assert_eq!(preview.clip, None, "no clips left to play");
    }

    #[test]
    fn numbers_screenshots_after_the_ones_on_disk() {
        let dir = std::env::temp_dir().join(format!("besfa_shots_{}", std::process::id()));
        std::fs::create_dir_all(dir.join(SCREENSHOTS)).unwrap();
        std::fs::write(dir.join("screenshots/hero-1.png"), "").unwrap();
        let mut preview = ModelPreview::new("models/hero.glb".into(), dir.clone());
        assert_eq!(preview.next_screenshot(), "screenshots/hero-2.png");
        // Not written yet, still not taken twice.
        assert_eq!(preview.next_screenshot(), "screenshots/hero-3.png");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
