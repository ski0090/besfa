//! The editor's side and bottom panels, the hierarchy, the asset folder and
//! the log, and what panels and pages share.

use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use eframe::egui::{self, Color32, Id, Label, RichText, ScrollArea, Ui};

use crate::{
    process::{Prebuild, PrebuildPhase},
    scene::{Scene, tree_rows},
};

pub const TEXT: Color32 = Color32::from_rgb(0xB3, 0xBB, 0xC8);
pub const MUTED: Color32 = Color32::from_rgb(0x7E, 0x87, 0x95);
pub const ACCENT: Color32 = Color32::from_rgb(0x8C, 0xB4, 0xFF);
pub const WARNING: Color32 = Color32::from_rgb(0xE8, 0xB8, 0x6D);
pub const GOOD: Color32 = Color32::from_rgb(0x7E, 0xC9, 0x8F);
pub const ERROR: Color32 = Color32::from_rgb(0xF2, 0x8B, 0x82);

/// A panel's title with its buttons on the right.
pub fn panel_header(ui: &mut Ui, title: &str, actions: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title.to_uppercase())
                .small()
                .strong()
                .color(MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
    });
    ui.separator();
}

pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).small().strong().color(MUTED));
}

pub fn tag(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(MUTED));
}

/// The prebuild's message with a spinner, check or warning, for a
/// right-to-left row: the message ends up rightmost.
pub fn prebuild_status(ui: &mut Ui, prebuild: &Prebuild) {
    let text = match (prebuild.phase, prebuild.progress) {
        (PrebuildPhase::Running, Some(progress)) => {
            format!("{}% · {}", (progress * 100.0).floor(), prebuild.message)
        }
        _ => prebuild.message.clone(),
    };
    // Truncated, so a long cargo line does not push the rest away.
    ui.add(Label::new(RichText::new(text).small().color(MUTED)).truncate());
    match prebuild.phase {
        PrebuildPhase::Running => ui.add(egui::Spinner::new().size(12.0)),
        PrebuildPhase::Ready => ui.label(RichText::new("✔").color(ACCENT)),
        PrebuildPhase::Failed => ui.label(RichText::new("⚠").color(ERROR)),
    };
}

/// What the add menu spawns, by the `kind` the game takes.
pub const SPAWN_KINDS: [(&str, &str); 6] = [
    ("empty", "Empty"),
    ("cube", "Cube"),
    ("sphere", "Sphere"),
    ("plane", "Plane"),
    ("light", "Light"),
    ("camera", "Camera"),
];

pub enum HierarchyAction {
    Select(Option<u64>),
    Spawn(&'static str),
    Duplicate,
    Delete,
    SavePrefab,
}

/// The game's entities as a tree; clicking one selects it, clicking the
/// selected one clears the selection. The header adds, duplicates and
/// deletes entities, and saves one as a prefab when `can_save_prefab`.
pub fn hierarchy(ui: &mut Ui, scene: &Scene, can_save_prefab: bool) -> Option<HierarchyAction> {
    let mut action = None;
    let selection = scene.selected_entity().is_some();
    panel_header(ui, "Hierarchy", |ui| {
        if can_save_prefab
            && ui
                .add_enabled(selection, egui::Button::new("📦").small())
                .on_hover_text("Save as prefab")
                .clicked()
        {
            action = Some(HierarchyAction::SavePrefab);
        }
        if ui
            .add_enabled(selection, egui::Button::new("🗑").small())
            .on_hover_text("Delete (Delete)")
            .clicked()
        {
            action = Some(HierarchyAction::Delete);
        }
        if ui
            .add_enabled(selection, egui::Button::new("🗐").small())
            .on_hover_text("Duplicate (Ctrl+D)")
            .clicked()
        {
            action = Some(HierarchyAction::Duplicate);
        }
        ui.menu_button("➕", |ui| {
            for (kind, label) in SPAWN_KINDS {
                if ui.button(label).clicked() {
                    action = Some(HierarchyAction::Spawn(kind));
                }
            }
        })
        .response
        .on_hover_text("Add entity");
    });
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (entity, depth) in tree_rows(&scene.entities) {
            let selected = Some(entity.id) == scene.selected;
            // Entities the game spawns while running are not saved.
            let mut text = RichText::new(entity.label());
            text = if entity.scene { text } else { text.italics() };
            text = text.color(if entity.name.is_none() || !entity.scene {
                MUTED
            } else {
                TEXT
            });
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 16.0);
                if ui.selectable_label(selected, text).clicked() {
                    action = Some(HierarchyAction::Select((!selected).then_some(entity.id)));
                }
            });
        }
    });
    action
}

/// Files the scene can show: glTF models, and scene and prefab files.
pub fn can_place(path: &str) -> bool {
    [".glb", ".gltf", ".scn.ron"]
        .iter()
        .any(|extension| path.ends_with(extension))
}

/// One row of the asset tree: a path relative to the asset folder, with
/// forward slashes as asset paths have them.
struct AssetRow {
    path: String,
    name: String,
    depth: usize,
    folder: bool,
}

/// The project's asset folder as a tree that follows changes on disk.
/// Models, scenes and prefabs can be added to the scene with a button or
/// dragged out as their asset path.
pub struct AssetBrowser {
    directory: PathBuf,
    /// None while there is no asset folder.
    rows: Option<Vec<AssetRow>>,
    listed: Option<Instant>,
}

/// How often the asset folder is listed again.
// ponytail: polling; watch the folder (notify) if asset folders grow large.
const ASSET_POLL: Duration = Duration::from_secs(1);

impl AssetBrowser {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            rows: None,
            listed: None,
        }
    }

    /// Returns an asset path to put in the scene, when `can_add`.
    pub fn ui(&mut self, ui: &mut Ui, can_add: bool) -> Option<String> {
        if self
            .listed
            .is_none_or(|listed| listed.elapsed() >= ASSET_POLL)
        {
            self.rows = self.directory.is_dir().then(|| {
                let mut rows = Vec::new();
                list(&self.directory, "", 0, &mut rows);
                rows
            });
            self.listed = Some(Instant::now());
        }
        ui.ctx().request_repaint_after(ASSET_POLL);
        let Some(rows) = &self.rows else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No assets folder").color(MUTED))
            });
            return None;
        };
        let mut add = None;
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for row in rows {
                let placeable = can_add && !row.folder && can_place(&row.path);
                let icon = if row.folder {
                    "🗀"
                } else if placeable {
                    "⛶"
                } else {
                    "🗋"
                };
                let label = RichText::new(format!("{icon} {}", row.name)).color(TEXT);
                ui.horizontal(|ui| {
                    ui.add_space(row.depth as f32 * 16.0);
                    if placeable {
                        ui.dnd_drag_source(Id::new(("asset", &row.path)), row.path.clone(), |ui| {
                            ui.label(label);
                        });
                        if ui
                            .small_button("➕")
                            .on_hover_text("Add to scene")
                            .clicked()
                        {
                            add = Some(row.path.clone());
                        }
                    } else {
                        ui.label(label);
                    }
                });
            }
        });
        add
    }
}

/// `directory`'s folders, then its files, each folder followed by what is in
/// it. Hidden files are left out.
fn list(directory: &Path, prefix: &str, depth: usize, rows: &mut Vec<AssetRow>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries
        .flatten()
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
            )
        })
        .filter(|(name, _)| !name.starts_with('.'))
        .collect();
    entries.sort_by_key(|(name, path)| (!path.is_dir(), name.to_lowercase()));
    for (name, path) in entries {
        let folder = path.is_dir();
        rows.push(AssetRow {
            path: format!("{prefix}{name}"),
            name: name.clone(),
            depth,
            folder,
        });
        if folder {
            list(&path, &format!("{prefix}{name}/"), depth + 1, rows);
        }
    }
}

/// The game's output, newest line pinned to the bottom.
pub fn log(ui: &mut Ui, lines: &VecDeque<String>) {
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    ScrollArea::both()
        .auto_shrink(false)
        .stick_to_bottom(true)
        .show_rows(ui, row_height, lines.len(), |ui, range| {
            for line in lines.range(range) {
                ui.add(Label::new(RichText::new(line).monospace().color(TEXT)).extend());
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_first_then_files_by_name() {
        let dir = std::env::temp_dir().join(format!("besfa_assets_{}", std::process::id()));
        fs::create_dir_all(dir.join("models")).unwrap();
        fs::create_dir_all(dir.join(".hidden")).unwrap();
        for file in ["b.txt", "A.scn.ron", "models/ship.glb"] {
            fs::write(dir.join(file), "").unwrap();
        }
        let mut rows = Vec::new();
        list(&dir, "", 0, &mut rows);
        let paths: Vec<_> = rows
            .iter()
            .map(|row| (row.path.as_str(), row.depth))
            .collect();
        assert_eq!(
            paths,
            [
                ("models", 0),
                ("models/ship.glb", 1),
                ("A.scn.ron", 0),
                ("b.txt", 0)
            ]
        );
        assert!(can_place("models/ship.glb") && can_place("A.scn.ron") && !can_place("b.txt"));
        fs::remove_dir_all(dir).unwrap();
    }
}
