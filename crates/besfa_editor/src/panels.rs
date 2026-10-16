//! The editor's side and bottom panels, the hierarchy, the asset folder and
//! the log, and what panels and pages share.

use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

use besfa_protocol::SpawnKind;
use eframe::egui::{
    self, Color32, Id, Label, PointerButton, RichText, ScrollArea, Ui, WidgetInfo, WidgetType,
    collapsing_header::{CollapsingState, paint_default_icon},
};

use crate::{
    process::{Prebuild, PrebuildPhase},
    scene::Scene,
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

/// `rows`, given in tree order, each indented by its `depth` after a toggle
/// that folds away the rows under it, when it has any. Folds are named by
/// `fold`, which also says whether one starts open; `show` draws the rest of
/// a row.
fn tree<T>(
    ui: &mut Ui,
    rows: &[T],
    depth: impl Fn(&T) -> usize,
    fold: impl Fn(&T) -> (Id, bool),
    mut show: impl FnMut(&mut Ui, &T),
) {
    for at in unfolded(ui.ctx(), rows, &depth, &fold) {
        tree_row(ui, rows, at, &depth, &fold, &mut show);
    }
}

/// The indices of the `rows` [`tree`] shows: those no folded row above hides.
fn unfolded<T>(
    ctx: &egui::Context,
    rows: &[T],
    depth: impl Fn(&T) -> usize,
    fold: impl Fn(&T) -> (Id, bool),
) -> Vec<usize> {
    let mut shown = Vec::new();
    // The depth of the folded row whose rows are being skipped.
    let mut folded = None;
    for (at, row) in rows.iter().enumerate() {
        let row_depth = depth(row);
        if folded.is_some_and(|folded| row_depth > folded) {
            continue;
        }
        shown.push(at);
        let closed = || {
            let (id, open) = fold(row);
            !CollapsingState::load_with_default_open(ctx, id, open).is_open()
        };
        let parent = rows.get(at + 1).is_some_and(|next| depth(next) > row_depth);
        folded = (parent && closed()).then_some(row_depth);
    }
    shown
}

/// Row `at` of [`tree`].
fn tree_row<T>(
    ui: &mut Ui,
    rows: &[T],
    at: usize,
    depth: impl Fn(&T) -> usize,
    fold: impl Fn(&T) -> (Id, bool),
    show: impl FnOnce(&mut Ui, &T),
) {
    let row = &rows[at];
    let row_depth = depth(row);
    ui.horizontal(|ui| {
        ui.add_space(row_depth as f32 * 16.0);
        if rows.get(at + 1).is_some_and(|next| depth(next) > row_depth) {
            let (id, open) = fold(row);
            let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, open);
            let response = state.show_toggle_button(ui, paint_default_icon);
            let label = if state.is_open() { "Fold" } else { "Unfold" };
            response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
            state.store(ui.ctx());
        } else {
            ui.allocate_space(egui::vec2(ui.spacing().indent, ui.spacing().icon_width));
        }
        show(ui, row);
    });
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

/// What the add menu spawns, with its label.
pub const SPAWN_KINDS: [(SpawnKind, &str); 7] = [
    (SpawnKind::Empty, "Empty"),
    (SpawnKind::Cube, "Cube"),
    (SpawnKind::Sphere, "Sphere"),
    (SpawnKind::Plane, "Plane"),
    (SpawnKind::Light, "Light"),
    (SpawnKind::Camera, "Camera"),
    (SpawnKind::Terrain, "Terrain"),
];

pub enum HierarchyAction {
    Select(Option<u64>),
    Spawn(SpawnKind),
    Duplicate,
    Delete,
    SavePrefab,
}

/// The game's entities as a tree; clicking one selects it, clicking the
/// selected one clears the selection. The header adds, duplicates and
/// deletes entities, and saves one as a prefab when `can_save_prefab`.
/// `terrain`: the game's plugin makes terrain.
pub fn hierarchy(
    ui: &mut Ui,
    scene: &Scene,
    can_save_prefab: bool,
    terrain: bool,
) -> Option<HierarchyAction> {
    let mut action = None;
    let selection = scene.selected_entity().is_some();
    panel_header(ui, "Hierarchy", |ui| {
        let buttons = [
            (
                can_save_prefab,
                "📦",
                "Save as prefab",
                HierarchyAction::SavePrefab,
            ),
            (true, "🗑", "Delete (Delete)", HierarchyAction::Delete),
            (true, "🗐", "Duplicate (Ctrl+D)", HierarchyAction::Duplicate),
        ];
        for (shown, icon, tip, act) in buttons {
            if shown
                && ui
                    .add_enabled(selection, egui::Button::new(icon).small())
                    .on_hover_text(tip)
                    .clicked()
            {
                action = Some(act);
            }
        }
        ui.menu_button("➕", |ui| {
            for (kind, label) in SPAWN_KINDS {
                if kind == SpawnKind::Terrain && !terrain {
                    continue;
                }
                if ui.button(label).clicked() {
                    action = Some(HierarchyAction::Spawn(kind));
                }
            }
        })
        .response
        .on_hover_text("Add entity");
    });
    let rows = scene.rows();
    let entity = |&(at, _): &(usize, usize)| &scene.entities[at];
    let depth = |&(_, depth): &(usize, usize)| depth;
    // Folds start closed, as Unity's do: placed models' parts, thousands in a
    // big scene, and a group's members are seldom what is picked here.
    let fold = |row: &(usize, usize)| (Id::new(("hierarchy fold", entity(row).id)), false);
    // A new selection, picked in the scene view too, opens the folds above
    // it once it is listed.
    let at = rows
        .iter()
        .position(|row| Some(entity(row).id) == scene.selected);
    let listed = at.map(|at| entity(&rows[at]).id);
    let shown = Id::new("hierarchy selection");
    if ui.data(|data| data.get_temp(shown)) != Some(listed) {
        ui.data_mut(|data| data.insert_temp(shown, listed));
        if let Some(at) = at {
            let mut below = depth(&rows[at]);
            for row in rows[..at].iter().rev() {
                if depth(row) < below {
                    below = depth(row);
                    let (id, open) = fold(row);
                    let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, open);
                    state.set_open(true);
                    state.store(ui.ctx());
                }
            }
        }
    }
    // Rows are buttons, all of one height, so only those in view are laid out.
    let spacing = ui.spacing();
    let row_height = spacing
        .interact_size
        .y
        .max(ui.text_style_height(&egui::TextStyle::Body) + 2.0 * spacing.button_padding.y);
    let visible = unfolded(ui.ctx(), rows, depth, fold);
    ScrollArea::vertical().auto_shrink(false).show_rows(
        ui,
        row_height,
        visible.len(),
        |ui, range| {
            for &at in &visible[range] {
                tree_row(ui, rows, at, depth, fold, |ui, row| {
                    let entity = entity(row);
                    let selected = Some(entity.id) == scene.selected;
                    // Entities the game spawns while running are not saved.
                    let mut text = RichText::new(entity.label());
                    text = if entity.scene { text } else { text.italics() };
                    text = text.color(if entity.name.is_none() || !entity.scene {
                        MUTED
                    } else {
                        TEXT
                    });
                    if ui.selectable_label(selected, text).clicked() {
                        action = Some(HierarchyAction::Select((!selected).then_some(entity.id)));
                    }
                });
            }
        },
    );
    action
}

/// Files the scene can show: glTF models, and scene and prefab files.
pub fn can_place(path: &str) -> bool {
    can_preview(path) || path.ends_with(".scn.ron")
}

/// glTF models, which the model preview shows.
pub fn can_preview(path: &str) -> bool {
    path.ends_with(".glb") || path.ends_with(".gltf")
}

/// What the Assets tab asks for, with an asset path.
pub enum AssetAction {
    /// Put it in the scene.
    Add(String),
    /// Open it in the model preview.
    Preview(String),
}

/// One row of the asset tree: a path relative to the asset folder, with
/// forward slashes as asset paths have them.
struct AssetRow {
    path: String,
    name: String,
    depth: usize,
    folder: bool,
    modified: Option<SystemTime>,
}

/// Where loading a changed model file again stands.
pub enum Reimport {
    Running,
    Done(Instant),
    Failed(String),
}

/// How long a finished reimport says so.
const REIMPORTED_SHOWN: Duration = Duration::from_secs(5);

impl Reimport {
    /// A spinner, a check for a while, or a warning that says why.
    pub fn ui(&self, ui: &mut Ui) {
        match self {
            Reimport::Running => {
                ui.add(egui::Spinner::new().size(10.0))
                    .on_hover_text("Reimporting");
            }
            Reimport::Done(at) if at.elapsed() < REIMPORTED_SHOWN => {
                ui.label(RichText::new("✔").small().color(GOOD))
                    .on_hover_text("Reimported");
            }
            Reimport::Done(_) => {}
            Reimport::Failed(error) => {
                ui.label(RichText::new("⚠").small().color(ERROR))
                    .on_hover_text(format!("Could not reimport: {error}"));
            }
        }
    }
}

/// Where the game writes models' thumbnails, in the project: a build
/// folder version control leaves out.
const THUMBNAILS: &str = "target/besfa/thumbnails";

/// The thumbnail of the model at asset `path`, relative to the project.
pub fn thumbnail_file(path: &str) -> String {
    format!("{THUMBNAILS}/{path}.png")
}

/// Thumbnails' sides in the tree and in a model's tooltip, in points.
const THUMBNAIL_ROW: f32 = 18.0;
const THUMBNAIL_TIP: f32 = 128.0;

/// The project's asset folder as a tree that follows changes on disk.
/// Models, scenes and prefabs can be added to the scene with a button or
/// dragged out as their asset path; double-clicking a model previews it.
/// Models show their thumbnails, and models that change on disk are
/// noticed, to load them again and picture them anew.
#[derive(Default)]
pub struct AssetBrowser {
    directory: PathBuf,
    /// None while there is no asset folder.
    rows: Option<Vec<AssetRow>>,
    listed: Option<Instant>,
    /// Each model's modification time, and whether it changed at the last
    /// listing, so it may still be being written.
    models: HashMap<String, (SystemTime, bool)>,
    pub reimports: HashMap<String, Reimport>,
    /// Each model thumbnail's modification time, at the last listing.
    thumbnails: HashMap<String, SystemTime>,
    /// Thumbnails read, by the modification time of the file read; None
    /// for a file that could not be read.
    textures: HashMap<String, (SystemTime, Option<egui::TextureHandle>)>,
    /// The model's modification time each thumbnail was asked for at.
    asked: HashMap<String, SystemTime>,
}

/// How often the asset folder is listed again.
// ponytail: polling; watch the folder (notify) if asset folders grow large.
const ASSET_POLL: Duration = Duration::from_secs(1);

impl AssetBrowser {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            ..Default::default()
        }
    }

    fn project(&self) -> &Path {
        self.directory.parent().unwrap_or(&self.directory)
    }

    /// Models without a thumbnail, or with one older than the model, not
    /// asked for since the model last changed.
    pub fn wanted_thumbnails(&self) -> Vec<String> {
        self.models
            .iter()
            .filter(|(path, (modified, _))| {
                self.thumbnails
                    .get(*path)
                    .is_none_or(|made| made < modified)
                    && self.asked.get(*path) != Some(modified)
            })
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// Notes that the game was asked for the thumbnail of `path`.
    pub fn asked(&mut self, path: &str) {
        if let Some((modified, _)) = self.models.get(path) {
            self.asked.insert(path.to_owned(), *modified);
        }
    }

    /// A new game process: what the last one was asked for and did not
    /// write is asked for again.
    pub fn forget_asked(&mut self) {
        self.asked.clear();
    }

    /// Lists the folder at the next poll, for a file just written.
    pub fn refresh(&mut self) {
        self.listed = None;
    }

    /// Lists the folder again now and then, whether or not it shows, and
    /// returns the models whose files changed and have stayed the same since
    /// the listing before, so they are written.
    // ponytail: a .gltf's own file only; watch its .bin and textures too if
    // models come split.
    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<String> {
        ctx.request_repaint_after(ASSET_POLL);
        if self
            .listed
            .is_some_and(|listed| listed.elapsed() < ASSET_POLL)
        {
            return Vec::new();
        }
        self.list();
        let mut settled = Vec::new();
        let models = self
            .rows
            .iter()
            .flatten()
            .filter(|row| can_preview(&row.path));
        for row in models {
            let Some(modified) = row.modified else {
                continue;
            };
            match self.models.get_mut(&row.path) {
                None => {
                    self.models.insert(row.path.clone(), (modified, false));
                }
                Some((known, changing)) if *known != modified => {
                    *known = modified;
                    *changing = true;
                }
                Some((_, changing)) if *changing => {
                    *changing = false;
                    settled.push(row.path.clone());
                }
                Some(_) => {}
            }
        }
        settled
    }

    /// The folder's files, and the models' thumbnails.
    fn list(&mut self) {
        self.rows = self.directory.is_dir().then(|| {
            let mut rows = Vec::new();
            list(&self.directory, "", 0, &mut rows);
            rows
        });
        self.listed = Some(Instant::now());
        let project = self.project().to_path_buf();
        self.thumbnails = self
            .rows
            .iter()
            .flatten()
            .filter(|row| can_preview(&row.path))
            .filter_map(|row| {
                let file = project.join(thumbnail_file(&row.path));
                let made = fs::metadata(file).and_then(|file| file.modified()).ok()?;
                Some((row.path.clone(), made))
            })
            .collect();
    }

    /// Returns what to do with an asset; nothing is added or previewed
    /// unless `can_add`.
    pub fn ui(&mut self, ui: &mut Ui, can_add: bool) -> Option<AssetAction> {
        if self.listed.is_none() {
            self.list();
        }
        let project = self.project().to_path_buf();
        let Self {
            rows,
            thumbnails,
            textures,
            reimports,
            ..
        } = self;
        let Some(rows) = rows else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No assets folder").color(MUTED))
            });
            return None;
        };
        let mut action = None;
        let fold = |row: &AssetRow| (Id::new(("asset fold", &row.path)), true);
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            tree(
                ui,
                rows,
                |row| row.depth,
                fold,
                |ui, row| {
                    let placeable = can_add && !row.folder && can_place(&row.path);
                    let icon = if row.folder {
                        "🗀"
                    } else if placeable {
                        "⛶"
                    } else {
                        "🗋"
                    };
                    let thumbnail = thumbnails.get(&row.path).and_then(|&made| {
                        let file = project.join(thumbnail_file(&row.path));
                        texture(ui.ctx(), textures, &row.path, &file, made)
                    });
                    let label = RichText::new(&row.name).color(TEXT);
                    // The thumbnail, or the icon, in a slot of the same size.
                    let slot = |ui: &mut Ui| {
                        let side = egui::vec2(THUMBNAIL_ROW, THUMBNAIL_ROW);
                        match &thumbnail {
                            Some(texture) => ui.image((texture.id(), side)),
                            None => ui.add_sized(side, Label::new(RichText::new(icon).color(TEXT))),
                        };
                    };
                    if placeable {
                        let id = Id::new(("asset", &row.path));
                        let name = ui.dnd_drag_source(id, row.path.clone(), |ui| {
                            ui.horizontal(|ui| {
                                slot(ui);
                                ui.label(label);
                            });
                        });
                        // The drag source on top takes the row's clicks from
                        // egui, so a double-click over it is read directly.
                        // Not `hovered`: egui clears it while the button is down.
                        if can_preview(&row.path) {
                            let name = name.response.on_hover_ui(|ui| {
                                if let Some(texture) = &thumbnail {
                                    ui.image((texture.id(), egui::Vec2::splat(THUMBNAIL_TIP)));
                                }
                                ui.label("Double-click to preview");
                            });
                            if name.contains_pointer()
                                && ui.input(|i| {
                                    i.pointer.button_double_clicked(PointerButton::Primary)
                                })
                            {
                                action = Some(AssetAction::Preview(row.path.clone()));
                            }
                        }
                        if ui
                            .small_button("➕")
                            .on_hover_text("Add to scene")
                            .clicked()
                        {
                            action = Some(AssetAction::Add(row.path.clone()));
                        }
                    } else {
                        slot(ui);
                        ui.label(label);
                    }
                    if let Some(reimport) = reimports.get(&row.path) {
                        reimport.ui(ui);
                    }
                },
            );
        });
        action
    }
}

/// The thumbnail of the model at `path` from `file`, written at `made`; read
/// again once the file changes.
fn texture(
    ctx: &egui::Context,
    textures: &mut HashMap<String, (SystemTime, Option<egui::TextureHandle>)>,
    path: &str,
    file: &Path,
    made: SystemTime,
) -> Option<egui::TextureHandle> {
    if textures.get(path).is_none_or(|(read, _)| *read != made) {
        let texture = image::open(file).ok().map(|image| {
            let image = image.to_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            let pixels = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            ctx.load_texture(path, pixels, egui::TextureOptions::LINEAR)
        });
        textures.insert(path.to_owned(), (made, texture));
    }
    textures.get(path)?.1.clone()
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
        let modified = fs::metadata(&path).and_then(|file| file.modified()).ok();
        rows.push(AssetRow {
            path: format!("{prefix}{name}"),
            name: name.clone(),
            depth,
            folder,
            modified,
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

    #[test]
    fn asks_for_missing_and_old_thumbnails_once() {
        let dir = std::env::temp_dir().join(format!("besfa_thumbnails_{}", std::process::id()));
        let assets = dir.join("assets");
        fs::create_dir_all(assets.join("models")).unwrap();
        fs::write(assets.join("models/ship.glb"), "").unwrap();
        fs::write(assets.join("models/boat.glb"), "").unwrap();
        let ctx = egui::Context::default();
        let mut browser = AssetBrowser::new(assets.clone());
        browser.poll(&ctx);
        let mut wanted = browser.wanted_thumbnails();
        wanted.sort();
        assert_eq!(wanted, ["models/boat.glb", "models/ship.glb"]);

        browser.asked("models/ship.glb");
        let boat = dir.join(thumbnail_file("models/boat.glb"));
        fs::create_dir_all(boat.parent().unwrap()).unwrap();
        fs::write(&boat, "").unwrap();
        browser.refresh();
        browser.poll(&ctx);
        assert!(browser.wanted_thumbnails().is_empty(), "asked, and made");

        // The model changed after its thumbnail was made.
        let later = SystemTime::now() + Duration::from_secs(10);
        fs::File::options()
            .write(true)
            .open(assets.join("models/boat.glb"))
            .unwrap()
            .set_modified(later)
            .unwrap();
        browser.refresh();
        browser.poll(&ctx);
        assert_eq!(browser.wanted_thumbnails(), ["models/boat.glb"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn notices_a_model_change_once_it_is_written() {
        let dir = std::env::temp_dir().join(format!("besfa_models_{}", std::process::id()));
        fs::create_dir_all(dir.join("models")).unwrap();
        let ship = dir.join("models/ship.glb");
        fs::write(&ship, "one").unwrap();
        fs::write(dir.join("notes.txt"), "").unwrap();
        let ctx = egui::Context::default();
        let mut assets = AssetBrowser::new(dir.clone());
        let mut poll = || {
            assets.listed = None;
            assets.poll(&ctx)
        };
        assert!(poll().is_empty(), "found, not changed");
        let later = SystemTime::now() + Duration::from_secs(10);
        fs::File::options()
            .write(true)
            .open(&ship)
            .unwrap()
            .set_modified(later)
            .unwrap();
        fs::write(dir.join("notes.txt"), "changed").unwrap();
        assert!(poll().is_empty(), "may still be being written");
        assert_eq!(poll(), ["models/ship.glb"]);
        assert!(poll().is_empty(), "once");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn previews_a_model_on_double_click_adds_it_and_drags_it() {
        use egui_kittest::{Harness, kittest::Queryable};

        let dir = std::env::temp_dir().join(format!("besfa_preview_{}", std::process::id()));
        fs::create_dir_all(dir.join("models")).unwrap();
        fs::write(dir.join("models/ship.glb"), "").unwrap();
        // Clicks take a few steps; at 60 a second two still make a double-click.
        let mut harness = Harness::builder().with_step_dt(1.0 / 60.0).build_ui_state(
            |ui, (assets, action): &mut (AssetBrowser, Option<AssetAction>)| {
                if let Some(asked) = assets.ui(ui, true) {
                    *action = Some(asked);
                }
            },
            (AssetBrowser::new(dir.clone()), None),
        );
        harness.run();
        let name = "ship.glb";

        harness.get_by_label(name).click();
        harness.step();
        assert!(harness.state().1.is_none(), "one click does nothing");
        harness.get_by_label(name).click();
        harness.step();
        assert!(
            matches!(&harness.state().1, Some(AssetAction::Preview(path)) if path == "models/ship.glb")
        );

        harness.state_mut().1 = None;
        harness.get_by_label("➕").click();
        harness.run();
        assert!(
            matches!(&harness.state().1, Some(AssetAction::Add(path)) if path == "models/ship.glb")
        );

        let at = harness.get_by_label(name).rect().center();
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
        harness.event(egui::Event::PointerMoved(at + egui::vec2(40.0, 0.0)));
        harness.step();
        harness.step();
        let payload = egui::DragAndDrop::payload::<String>(&harness.ctx);
        assert_eq!(
            payload.as_deref().map(String::as_str),
            Some("models/ship.glb")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn folds_a_folder_away_and_back() {
        use egui_kittest::{Harness, kittest::Queryable};

        let dir = std::env::temp_dir().join(format!("besfa_fold_{}", std::process::id()));
        fs::create_dir_all(dir.join("models/boats")).unwrap();
        for file in ["models/ship.glb", "models/boats/raft.glb", "notes.txt"] {
            fs::write(dir.join(file), "").unwrap();
        }
        let mut harness = Harness::new_ui_state(
            |ui, assets: &mut AssetBrowser| {
                assets.ui(ui, true);
            },
            AssetBrowser::new(dir.clone()),
        );
        harness.run();
        let shows = |harness: &Harness<AssetBrowser>, name| harness.query_by_label(name).is_some();
        assert!(shows(&harness, "raft.glb"));

        // The first fold is models', the second boats'.
        harness.get_all_by_label("Fold").next().unwrap().click();
        harness.run();
        assert!(!shows(&harness, "ship.glb") && !shows(&harness, "boats"));
        assert!(!shows(&harness, "raft.glb"), "folded with its folder");
        assert!(shows(&harness, "notes.txt"), "not in the folder");

        harness.get_by_label("Unfold").click();
        harness.run();
        assert!(shows(&harness, "ship.glb") && shows(&harness, "raft.glb"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn opens_the_folds_above_a_new_selection() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut scene = Scene::default();
        scene.handle(
            r#"@besfa {"type":"entities","entities":[{"id":1,"name":"Ship"},{"id":2,"name":"Hull","parent":1},{"id":3,"name":"Mast","parent":2}]}"#,
        );
        let mut harness = Harness::new_ui_state(
            |ui, scene: &mut Scene| {
                hierarchy(ui, scene, false, false);
            },
            scene,
        );
        harness.run();
        assert!(harness.query_by_label("Hull").is_none(), "starts folded");

        // As a pick in the scene view selects it, two folds down.
        harness.state_mut().select(Some(3));
        harness.run();
        assert!(harness.query_by_label("Mast").is_some());
    }

    #[test]
    fn folds_placed_models_and_lays_out_only_rows_in_view() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut entities = vec![
            r#"{"id":1,"name":"Barrel","asset":"models/barrel.glb"}"#.to_owned(),
            r#"{"id":2,"name":"Stave","parent":1,"scene":false}"#.to_owned(),
        ];
        entities.extend((3..1000).map(|id| format!(r#"{{"id":{id},"name":"Rock {id}"}}"#)));
        let mut scene = Scene::default();
        scene.handle(&format!(
            r#"@besfa {{"type":"entities","entities":[{}]}}"#,
            entities.join(",")
        ));
        let mut harness = Harness::new_ui_state(
            |ui, scene: &mut Scene| {
                hierarchy(ui, scene, false, false);
            },
            scene,
        );
        harness.run();
        assert!(harness.query_by_label("Stave").is_none(), "starts folded");
        assert!(harness.query_by_label("Rock 3").is_some());
        assert!(harness.query_by_label("Rock 999").is_none(), "out of view");

        harness.get_by_label("Unfold").click();
        harness.run();
        assert!(harness.query_by_label("Stave").is_some());
    }
}
