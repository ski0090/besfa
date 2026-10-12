//! The project hub: create a project, open one, or pick a recent one.

use std::{
    path::Path,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use eframe::egui::{self, Button, Frame, Key, Label, RichText, ScrollArea, TextEdit, Ui};

use crate::{
    panels::{ACCENT, ERROR, MUTED, TEXT, prebuild_status},
    process::Prebuild,
    project::{self, Project, RecentProjects},
};

/// How long a message stays at the bottom of the hub.
const MESSAGE_TIME: Duration = Duration::from_secs(4);

const CARD: egui::Color32 = egui::Color32::from_rgb(0x20, 0x24, 0x2B);

pub struct Hub {
    recent: Vec<Project>,
    message: Option<(String, Instant)>,
    create: Option<CreateDialog>,
    /// A project the user asked to delete, until they confirm.
    confirm_delete: Option<Project>,
    /// A project being moved to the Recycle Bin, and whether that worked.
    deleting: Option<(Project, Receiver<bool>)>,
}

#[derive(Default)]
struct CreateDialog {
    directory: String,
    error: Option<String>,
    /// `besfa new` while it runs: the project's path or what went wrong.
    creating: Option<Receiver<Result<String, String>>>,
}

/// Runs `job` off the UI thread and wakes the UI with its result.
fn background<T: Send + 'static>(
    ctx: &egui::Context,
    job: impl FnOnce() -> T + Send + 'static,
) -> Receiver<T> {
    let (sender, receiver) = mpsc::channel();
    let ctx = ctx.clone();
    thread::spawn(move || {
        let _ = sender.send(job());
        ctx.request_repaint();
    });
    receiver
}

impl Hub {
    pub fn new(recent: &RecentProjects) -> Self {
        Self {
            recent: recent.load(),
            message: None,
            create: None,
            confirm_delete: None,
            deleting: None,
        }
    }

    fn show_message(&mut self, message: String) {
        self.message = Some((message, Instant::now()));
    }

    /// Shows the hub; returns the project to open in the editor.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        store: &RecentProjects,
        prebuild: &Prebuild,
    ) -> Option<Project> {
        let mut open = None;
        egui::Panel::bottom("footer").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Besfa Editor").color(MUTED));
                ui.add_space(24.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("0.1.0-dev").small().color(MUTED));
                    ui.add_space(24.0);
                    prebuild_status(ui, prebuild);
                });
            });
            ui.add_space(6.0);
        });
        if let Some((message, shown)) = &self.message {
            let left = MESSAGE_TIME.saturating_sub(shown.elapsed());
            if left.is_zero() {
                self.message = None;
            } else {
                egui::Panel::bottom("message")
                    .show(ui, |ui| ui.label(RichText::new(message).color(TEXT)));
                ui.ctx().request_repaint_after(left);
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let width = ui.available_width().min(920.0);
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - width) / 2.0);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        ui.add_space(32.0);
                        if let Some(action) = self.content(ui, width) {
                            open = self.act(action, store);
                        }
                    });
                });
            });
        });
        let created = self.create_dialog(ui.ctx(), store);
        self.delete_dialog(ui.ctx(), store);
        open.or(created)
    }

    fn content(&self, ui: &mut Ui, width: f32) -> Option<HubAction> {
        let mut action = None;
        ui.label(
            RichText::new("Start creating")
                .size(36.0)
                .color(egui::Color32::WHITE),
        );
        ui.add_space(10.0);
        ui.label(
            RichText::new("Create a new Besfa project or open one you already have.")
                .size(16.0)
                .color(TEXT),
        );
        ui.add_space(32.0);
        let create = (
            "➕",
            "Create project",
            "Start a new binary Rust game project.",
            "New project",
        );
        let open = (
            "🗁",
            "Open project",
            "Open an existing Besfa project folder.",
            "Open folder",
        );
        let narrow = width < 680.0;
        let card_width = if narrow { width } else { (width - 16.0) / 2.0 };
        let mut cards = |ui: &mut Ui| {
            if card(ui, card_width, create, true) {
                action = Some(HubAction::Create);
            }
            if card(ui, card_width, open, false) {
                action = Some(HubAction::OpenFolder);
            }
        };
        if narrow {
            ui.vertical(|ui| cards(ui));
        } else {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                cards(ui);
            });
        }
        ui.add_space(48.0);
        ui.label(
            RichText::new("Recent projects")
                .size(22.0)
                .color(egui::Color32::WHITE),
        );
        ui.add_space(16.0);
        if self.recent.is_empty() {
            Frame::new()
                .stroke((1.0, egui::Color32::from_white_alpha(0x1A)))
                .corner_radius(12.0)
                .inner_margin(46.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("🗀").size(28.0).color(MUTED));
                        ui.label("No recent projects");
                        ui.label(RichText::new("Projects you open will appear here.").color(MUTED));
                    });
                });
            return action;
        }
        Frame::new()
            .fill(CARD)
            .corner_radius(12.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for project in &self.recent {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("🗑").on_hover_text("Delete project").clicked() {
                                action = Some(HubAction::Delete(project.clone()));
                            }
                            if ui.button("🗁").on_hover_text("Show in Explorer").clicked() {
                                action = Some(HubAction::Reveal(project.clone()));
                            }
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                let name = RichText::new(format!("🗀  {}", project.name()))
                                    .size(15.0)
                                    .color(egui::Color32::WHITE);
                                let path = RichText::new(&project.path).color(MUTED);
                                let clicked =
                                    [Label::new(name), Label::new(path).truncate()].map(|label| {
                                        ui.add(label.sense(egui::Sense::click()))
                                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                                            .clicked()
                                    });
                                if clicked.contains(&true) {
                                    action = Some(HubAction::OpenRecent(project.clone()));
                                }
                            });
                        });
                    });
                }
            });
        action
    }

    fn act(&mut self, action: HubAction, store: &RecentProjects) -> Option<Project> {
        match action {
            HubAction::Create => self.create = Some(CreateDialog::default()),
            HubAction::OpenFolder => {
                let directory = project::pick_folder("Open")?;
                let project = Project::new(&directory);
                if project.exists() {
                    return Some(self.open(project, store));
                }
                self.show_message(format!("'{directory}' has no Cargo.toml."));
            }
            HubAction::OpenRecent(project) => {
                if project.exists() {
                    return Some(self.open(project, store));
                }
                self.forget(project, store);
            }
            HubAction::Reveal(project) => {
                if Path::new(&project.path).exists() {
                    project::reveal(&project.path);
                } else {
                    self.show_message(format!("'{}' no longer exists.", project.path));
                }
            }
            // Never delete a folder that is not (or no longer) a Cargo project.
            HubAction::Delete(project) if !project.exists() => self.forget(project, store),
            HubAction::Delete(project) => self.confirm_delete = Some(project),
        }
        None
    }

    fn open(&mut self, project: Project, store: &RecentProjects) -> Project {
        self.recent = store.add(&project);
        project
    }

    fn forget(&mut self, project: Project, store: &RecentProjects) {
        self.recent = store.remove(&project);
        self.show_message(format!(
            "'{}' is no longer a Cargo project and was removed.",
            project.path
        ));
    }

    fn create_dialog(&mut self, ctx: &egui::Context, store: &RecentProjects) -> Option<Project> {
        let dialog = self.create.as_mut()?;
        if let Some(result) = dialog
            .creating
            .as_ref()
            .and_then(|creating| creating.try_recv().ok())
        {
            dialog.creating = None;
            match result {
                Ok(path) => {
                    self.create = None;
                    return Some(self.open(Project::new(path), store));
                }
                Err(error) => dialog.error = Some(error),
            }
        }
        let creating = dialog.creating.is_some();
        let (mut create, mut cancel) = (false, false);
        let modal = egui::Modal::new(egui::Id::new("create_project")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.heading("Create project");
            ui.label("Enter the folder path for the new Cargo project.");
            ui.add_space(12.0);
            ui.label(RichText::new("Project directory").small().color(MUTED));
            ui.horizontal(|ui| {
                if ui.add_enabled(!creating, Button::new("🗁 Browse")).clicked() {
                    browse(dialog);
                }
                let field = TextEdit::singleline(&mut dialog.directory)
                    .hint_text(r"ex) C:\Projects\my_game")
                    .desired_width(f32::INFINITY);
                let field = ui.add_enabled(!creating, field);
                if ui.memory(|memory| memory.focused().is_none()) && !creating {
                    field.request_focus();
                }
                create = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            });
            if let Some(error) = &dialog.error {
                ui.label(RichText::new(error).color(ERROR));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                cancel = ui.add_enabled(!creating, Button::new("Cancel")).clicked();
                if creating {
                    ui.add(egui::Spinner::new());
                    ui.label("Creating…");
                } else {
                    create |= ui
                        .button(RichText::new("➕ Create").color(ACCENT))
                        .clicked();
                }
            });
        });
        if create && !creating {
            let directory = dialog.directory.trim().to_owned();
            if directory.is_empty() {
                dialog.error = Some("Enter a directory for the new project.".into());
            } else {
                dialog.error = None;
                dialog.creating = Some(background(ctx, move || project::create(&directory)));
            }
        } else if !creating && (cancel || modal.should_close()) {
            self.create = None;
        }
        None
    }

    fn delete_dialog(&mut self, ctx: &egui::Context, store: &RecentProjects) {
        if let Some((project, done)) = &self.deleting
            && let Ok(deleted) = done.try_recv()
        {
            let project = project.clone();
            self.deleting = None;
            if deleted {
                self.recent = store.remove(&project);
            } else {
                self.show_message(format!("Could not delete '{}'.", project.path));
            }
        }
        let Some(project) = &self.confirm_delete else {
            return;
        };
        let (mut delete, mut cancel) = (false, false);
        let modal = egui::Modal::new(egui::Id::new("delete_project")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading(format!("Delete '{}'?", project.name()));
            ui.label(format!(
                "'{}' will be moved to the Recycle Bin.",
                project.path
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                cancel = ui.button("Cancel").clicked();
                delete = ui.button(RichText::new("Delete").color(ERROR)).clicked();
            });
        });
        if delete {
            let project = self.confirm_delete.take().unwrap();
            let path = project.path.clone();
            self.deleting = Some((
                project,
                background(ctx, move || project::move_to_recycle_bin(&path)),
            ));
        } else if cancel || modal.should_close() {
            self.confirm_delete = None;
        }
    }
}

enum HubAction {
    Create,
    OpenFolder,
    OpenRecent(Project),
    Reveal(Project),
    Delete(Project),
}

/// The CLI needs a folder that does not exist yet, so an editable name is
/// appended to the picked location.
fn browse(dialog: &mut CreateDialog) {
    if let Some(parent) = project::pick_folder("Select location") {
        let separator = if parent.ends_with('\\') { "" } else { "\\" };
        dialog.directory = format!("{parent}{separator}new_project");
        dialog.error = None;
    }
}

/// A card with an action button; true when the button is pressed.
fn card(
    ui: &mut Ui,
    width: f32,
    (icon, title, description, action): (&str, &str, &str, &str),
    emphasized: bool,
) -> bool {
    let content = |ui: &mut Ui| {
        ui.set_width(width - 48.0);
        ui.label(RichText::new(icon).size(30.0).color(ACCENT));
        ui.add_space(24.0);
        ui.label(RichText::new(title).size(20.0).color(egui::Color32::WHITE));
        ui.add_space(8.0);
        ui.label(RichText::new(description).color(TEXT));
        ui.add_space(24.0);
        let text = RichText::new(format!("{icon} {action}"));
        let button = if emphasized {
            Button::new(text.color(egui::Color32::from_rgb(0x10, 0x26, 0x4C))).fill(ACCENT)
        } else {
            Button::new(text)
        };
        ui.add(button).clicked()
    };
    let frame = Frame::new()
        .fill(CARD)
        .corner_radius(12.0)
        .inner_margin(24.0);
    // A card stacks its content even in a row of cards.
    frame.show(ui, |ui| ui.vertical(content).inner).inner
}
