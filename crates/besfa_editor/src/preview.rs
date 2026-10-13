//! The model preview tab: a glTF model from the Assets tab on its own in
//! the viewport, with its clips, parts and light. The game loads, poses and
//! lights it; this side keeps the controls and tells the game when they
//! change. The clip's clock runs on both sides by the same rule, so the time
//! shown is never reported.

use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

use besfa_protocol::{ClipInfo, Command, EntityInfo, LightPreset, clip_time};
use eframe::egui::{self, Button, RichText, ScrollArea, Ui};

use crate::{
    panels::{ERROR, MUTED, TEXT, panel_header, section, tag},
    scene::tree_rows,
};

const LIGHTS: [(LightPreset, &str); 3] = [
    (LightPreset::Studio, "Studio"),
    (LightPreset::Daylight, "Daylight"),
    (LightPreset::Night, "Night"),
];

const SPEEDS: [f32; 4] = [0.25, 0.5, 1.0, 2.0];

/// What the game reported once the model loaded.
struct Loaded {
    clips: Vec<ClipInfo>,
    parts: Vec<EntityInfo>,
    /// Each part's path of names from the model's root, which a part keeps
    /// when the model loads again and its id changes.
    paths: HashMap<u64, String>,
    size: Option<[f32; 3]>,
    error: Option<String>,
}

pub struct ModelPreview {
    /// Relative to the asset folder.
    pub path: String,
    loaded: Option<Loaded>,
    clip: Option<usize>,
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
}

impl ModelPreview {
    pub fn new(path: String) -> Self {
        Self {
            path,
            loaded: None,
            clip: None,
            playing: true,
            duration: 0.0,
            time: 0.0,
            since: Instant::now(),
            speed: 1.0,
            looping: true,
            hidden: HashSet::new(),
            solo: None,
            light: LightPreset::default(),
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
        ]
    }

    /// Takes the game's report on the model, keeping the choices that still
    /// apply, and returns what puts the game's model in step with them.
    pub fn loaded(
        &mut self,
        clips: Vec<ClipInfo>,
        parts: Vec<EntityInfo>,
        size: Option<[f32; 3]>,
        error: Option<String>,
    ) -> Vec<Command> {
        // Part ids are the game's entities, new each time the model loads;
        // their paths stay unless the file renamed them.
        let paths = part_paths(&parts);
        let known: HashSet<&String> = paths.values().collect();
        self.hidden.retain(|path| known.contains(path));
        self.solo = self.solo.take().filter(|path| known.contains(path));
        if self.clip.is_none_or(|clip| clip >= clips.len()) {
            self.clip = (!clips.is_empty()).then_some(0);
            self.rebase(0.0);
        }
        self.duration = self
            .clip
            .and_then(|clip| clips.get(clip))
            .map_or(0.0, |clip| clip.duration);
        self.loaded = Some(Loaded {
            clips,
            parts,
            paths,
            size,
            error,
        });
        let parts = self.loaded.as_ref().map(|loaded| self.parts(loaded));
        [Some(self.animation()), parts]
            .into_iter()
            .flatten()
            .collect()
    }

    fn now(&self) -> f32 {
        let speed = if self.playing { self.speed } else { 0.0 };
        let seconds = self.since.elapsed().as_secs_f32();
        clip_time(self.time, seconds, speed, self.duration, self.looping)
    }

    /// Restarts the clock at `time`, before a control changes how it runs.
    fn rebase(&mut self, time: f32) {
        self.time = time;
        self.since = Instant::now();
    }

    fn animation(&self) -> Command {
        Command::PreviewAnimation {
            clip: self.clip,
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

    /// The tab's side panel; returns what to tell the game.
    pub fn ui(&mut self, ui: &mut Ui) -> Vec<Command> {
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
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.label(RichText::new(self.name()).strong().color(TEXT));
            if let Some(error) = &loaded.error {
                ui.label(RichText::new(error).color(ERROR));
                return;
            }
            if let Some([width, height, depth]) = loaded.size {
                tag(ui, &format!("{width:.2} × {height:.2} × {depth:.2} m"));
            }
            self.animation_ui(ui, &loaded.clips, &mut send);
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
        });
        self.loaded = Some(loaded);
        send
    }

    fn animation_ui(&mut self, ui: &mut Ui, clips: &[ClipInfo], send: &mut Vec<Command>) {
        section(ui, "Animation");
        let Some(clip) = self.clip.and_then(|clip| clips.get(clip)) else {
            tag(ui, "No clips");
            return;
        };
        let mut changed = false;
        egui::ComboBox::from_id_salt("preview_clip")
            .selected_text(&clip.name)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for (index, clip) in clips.iter().enumerate() {
                    if ui
                        .selectable_label(self.clip == Some(index), &clip.name)
                        .clicked()
                    {
                        self.clip = Some(index);
                        self.duration = clip.duration;
                        self.rebase(0.0);
                        changed = true;
                    }
                }
            });
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

    fn parts_ui(&mut self, ui: &mut Ui, loaded: &Loaded, send: &mut Vec<Command>) {
        if loaded.parts.is_empty() {
            tag(ui, "No parts");
            return;
        }
        let mut changed = false;
        for (part, depth) in tree_rows(&loaded.parts) {
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
                ui.label(label);
            });
        }
        if changed {
            send.push(self.parts(loaded));
        }
    }
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

    fn clip(name: &str) -> ClipInfo {
        ClipInfo {
            name: name.into(),
            duration: 1.0,
        }
    }

    #[test]
    fn shows_the_clip_time_while_the_panel_holds_the_report() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut preview = ModelPreview::new("models/hero.glb".into());
        preview.loaded(vec![clip("Idle")], vec![], None, None);
        preview.playing = false;
        preview.rebase(0.5);
        let mut harness = Harness::new_ui_state(
            |ui, preview: &mut ModelPreview| {
                preview.ui(ui);
            },
            preview,
        );
        harness.run();
        harness.get_by_label("0.50 / 1.00 s");
    }

    #[test]
    fn keeps_choices_that_still_apply_when_the_model_reloads() {
        let mut preview = ModelPreview::new("models/hero.glb".into());
        assert_eq!(preview.name(), "hero.glb");
        let parts = |body, head, hat| {
            vec![
                part(body, "Body", None),
                part(head, "Head", Some(body)),
                part(hat, "Hat", Some(head)),
            ]
        };
        let sent = preview.loaded(vec![clip("Idle"), clip("Walk")], parts(1, 2, 3), None, None);
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
        let sent = preview.loaded(vec![clip("Idle"), clip("Walk")], parts(4, 5, 6), None, None);
        assert_eq!(preview.clip, Some(1));
        assert_eq!(
            sent[1],
            Command::PreviewParts {
                hidden: vec![6],
                solo: Some(5)
            }
        );

        // Re-exported without the hat.
        let sent = preview.loaded(vec![clip("Idle")], parts(7, 8, 9)[..2].to_vec(), None, None);
        assert!(preview.hidden.is_empty() && preview.solo.as_deref() == Some("Body/Head"));
        assert_eq!(
            sent[1],
            Command::PreviewParts {
                hidden: vec![],
                solo: Some(8)
            }
        );

        preview.loaded(vec![], vec![], None, None);
        assert_eq!(preview.clip, None, "no clips left to play");
    }
}
