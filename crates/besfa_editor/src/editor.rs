//! The editor for one project. The game runs in one of two states: paused
//! in edit mode, where the viewport shows the scene as Startup built it, or
//! playing. Stop ends play by relaunching the game in edit mode, which
//! resets the scene and picks up code changes. In both states the game
//! reports its entities, which the hierarchy and inspector panels show and
//! change through commands on its stdin, one JSON object per line.

use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

use besfa_protocol::{Command, Report, Request, Tool, VERSION};
use eframe::{
    egui::{
        self, Button, Color32, Key, KeyboardShortcut, Modifiers, RichText, Ui, ViewportCommand,
    },
    egui_wgpu::RenderState,
};
use serde_json::Value;

use crate::{
    inspector::{Action, Inspector},
    panels::{
        self, ACCENT, AssetAction, AssetBrowser, GOOD, HierarchyAction, MUTED, WARNING,
        prebuild_status,
    },
    preview::ModelPreview,
    process::{CliProcess, Output, Prebuild},
    project::{self, Project},
    scene::{Edit, EditHistory, Scene},
    viewport::{ViewportInput, ViewportTexture, adapter_name},
};

/// The log keeps the latest lines.
const MAX_LOG_LINES: usize = 2000;

/// The viewport texture's size until the panel has been laid out.
const DEFAULT_VIEWPORT: [u32; 2] = [1280, 720];

/// How long the viewport panel keeps its size before the texture follows.
const RESIZE_DELAY: Duration = Duration::from_millis(250);

/// How long the editor waits for the game to report a save.
const SAVE_TIMEOUT: Duration = Duration::from_secs(5);

/// The scene view's handle tools with their icons, tooltips and keys.
const TOOLS: [(Tool, &str, &str, Key); 3] = [
    (Tool::Translate, "✚", "Move (W)", Key::W),
    (Tool::Rotate, "⟳", "Rotate (E)", Key::E),
    (Tool::Scale, "⬈", "Scale (R)", Key::R),
];

/// What follows a save once the game reports it, or gives up waiting.
#[derive(Clone, Copy)]
enum AfterSave {
    Rebuild,
    Leave,
    Close,
}

enum Dialog {
    /// Asks for a prefab's file name, without folders or extension.
    PrefabName(String),
    /// Asks about unsaved changes before what follows.
    Unsaved(AfterSave),
}

pub struct Editor {
    project: Project,
    render: Option<RenderState>,
    logs: VecDeque<String>,
    scene: Scene,
    /// Scene changes of this edit session, for undo and redo.
    history: EditHistory,
    game: Option<CliProcess>,
    /// `cargo update` of the plugin, while it runs.
    update: Option<CliProcess>,
    viewport: Option<ViewportTexture>,
    /// Textures replaced by a resize. The game renders into one until it
    /// takes the `viewport` command, so they go once it answers the latest
    /// one, or when the next game launches: plugins from before protocol
    /// versions do not answer.
    retired: Vec<ViewportTexture>,
    /// The latest `viewport` request, which the retired textures wait on.
    viewport_request: Option<u64>,
    /// The last request number given to this game process.
    seq: u64,
    /// The protocol version the game's plugin speaks: 0 for one from before
    /// versions, None until the game says.
    protocol: Option<u32>,
    /// The viewport panel's size in physical pixels, which the texture
    /// follows once it settles.
    wanted_viewport: Option<[u32; 2]>,
    resize_at: Option<Instant>,
    input: ViewportInput,
    tool: Tool,
    /// Waiting for a game process: stopping one to launch the next, or
    /// updating the plugin first.
    starting: bool,
    /// The game was told to play; otherwise it sits paused in edit mode.
    playing: bool,
    /// Launch an edit session once the game exits.
    relaunch: bool,
    /// The prebuild holds the shared build directory; the edit session
    /// starts once it is done.
    waiting_for_prebuild: bool,
    /// `besfa_editor_plugin` revisions: the project's and the prebuild's.
    plugin_revision: Option<String>,
    latest_plugin_revision: Option<String>,
    /// A save in flight, until when it is waited for, and what follows it.
    pending_save: Option<(Instant, AfterSave)>,
    /// Save once this frame's fields have committed.
    save_after_ui: bool,
    source: SourceWatch,
    inspector: Inspector,
    assets: AssetBrowser,
    assets_tab: bool,
    /// The model preview's tab, while it is open.
    preview: Option<ModelPreview>,
    /// The viewport shows the model preview instead of the scene.
    previewing: bool,
    dialog: Option<Dialog>,
    /// The window may close: nothing is unsaved, or the user chose.
    closing: bool,
    leave: bool,
}

impl Editor {
    pub fn new(
        project: Project,
        render: Option<RenderState>,
        prebuild: &Prebuild,
        ctx: &egui::Context,
    ) -> Self {
        let mut editor = Self {
            source: SourceWatch::new(project.file("src")),
            assets: AssetBrowser::new(project.file("assets")),
            project,
            render,
            logs: VecDeque::new(),
            scene: Scene::default(),
            history: EditHistory::default(),
            game: None,
            update: None,
            viewport: None,
            retired: Vec::new(),
            viewport_request: None,
            seq: 0,
            protocol: None,
            wanted_viewport: None,
            resize_at: None,
            input: ViewportInput::default(),
            tool: Tool::Translate,
            starting: false,
            playing: false,
            relaunch: false,
            waiting_for_prebuild: prebuild.running(),
            plugin_revision: None,
            latest_plugin_revision: None,
            pending_save: None,
            save_after_ui: false,
            inspector: Inspector::default(),
            assets_tab: false,
            preview: None,
            previewing: false,
            dialog: None,
            closing: false,
            leave: false,
        };
        editor.read_plugin_revisions();
        if !editor.waiting_for_prebuild {
            editor.launch(false, ctx);
        }
        editor
    }

    /// The window title: the project, marked while the scene has unsaved
    /// changes.
    pub fn title(&self) -> String {
        let dirty = if self.scene.dirty { " *" } else { "" };
        format!("{}{dirty} - Besfa Editor", self.project.name())
    }

    /// Shows the editor; true once it should go back to the hub.
    pub fn ui(&mut self, ui: &mut Ui, prebuild: &Prebuild) -> bool {
        let ctx = ui.ctx().clone();
        self.poll_game(&ctx);
        self.poll_update(&ctx);
        if self.waiting_for_prebuild && !prebuild.running() {
            self.waiting_for_prebuild = false;
            // The prebuild moved its lock to the latest plugin.
            self.read_plugin_revisions();
            self.launch(false, &ctx);
        }
        if let Some((deadline, then)) = self.pending_save
            && Instant::now() >= deadline
        {
            self.pending_save = None;
            self.after_save(then, &ctx);
        }
        if self.source.settled(&ctx) {
            self.rebuild(prebuild, &ctx);
        }
        if self.resize_at.is_some_and(|at| Instant::now() >= at) {
            self.resize_viewport();
        }
        if self.dialog.is_none() {
            self.shortcuts(&ctx);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.closing && self.scene.dirty {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            // A second click on close while the question is up asks nothing more.
            if self.dialog.is_none() {
                self.dialog = Some(Dialog::Unsaved(AfterSave::Close));
            }
        }

        self.menu_bar(ui);
        self.toolbar(ui, prebuild, &ctx);
        egui::Panel::bottom("bottom")
            .default_size(200.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.assets_tab, false, "Log");
                    ui.selectable_value(&mut self.assets_tab, true, "Assets");
                });
                ui.separator();
                if !self.assets_tab {
                    panels::log(ui, &self.logs);
                } else {
                    match self.assets.ui(ui, !self.playing) {
                        Some(AssetAction::Add(path)) => self.instantiate(path),
                        Some(AssetAction::Preview(path)) => self.open_preview(path),
                        None => {}
                    }
                }
            });
        egui::Panel::left("hierarchy")
            .default_size(220.0)
            .show(ui, |ui| {
                if let Some(action) = panels::hierarchy(ui, &self.scene, !self.playing) {
                    self.on_hierarchy(action);
                }
            });
        egui::Panel::right("inspector")
            .default_size(320.0)
            .show(ui, |ui| {
                match self.preview.as_mut().filter(|_| self.previewing) {
                    Some(preview) => {
                        for command in preview.ui(ui, &self.scene) {
                            self.send(command);
                        }
                    }
                    None => {
                        let scene = self.scene.inspected();
                        if let Some(action) = self.inspector.ui(ui, &scene, !self.playing) {
                            self.on_inspector(action);
                        }
                    }
                }
            });
        egui::CentralPanel::default().show(ui, |ui| self.viewport_ui(ui, &ctx));
        self.dialogs(&ctx);

        if std::mem::take(&mut self.save_after_ui) {
            self.save();
        }
        // The game renders on its own; show its newest frame every frame.
        if let (Some(_), Some(viewport), Some(render)) = (&self.game, &self.viewport, &self.render)
        {
            viewport.copy_frame(render);
            ctx.request_repaint();
        }
        std::mem::take(&mut self.leave)
    }

    /// Reports from the game go to the scene, everything else to the log.
    fn poll_game(&mut self, ctx: &egui::Context) {
        let Some(game) = &self.game else {
            return;
        };
        for output in game.poll() {
            match output {
                Output::Line(line) => match self.scene.handle(&line) {
                    Some(report) => self.on_report(report, ctx),
                    None => self.log(line),
                },
                Output::Exit(code) => {
                    self.log(format!("Process exited with code {code}."));
                    self.game = None;
                    self.playing = false;
                    if std::mem::take(&mut self.relaunch) {
                        self.launch(false, ctx);
                    }
                }
            }
        }
    }

    fn poll_update(&mut self, ctx: &egui::Context) {
        let Some(update) = &self.update else {
            return;
        };
        for output in update.poll() {
            match output {
                Output::Line(line) => self.log(line),
                Output::Exit(code) => {
                    if code != 0 {
                        self.log(format!("cargo update exited with code {code}."));
                    }
                    self.update = None;
                    self.after_update(ctx);
                }
            }
        }
    }

    fn log(&mut self, line: impl Into<String>) {
        self.logs.push_back(line.into());
        if self.logs.len() > MAX_LOG_LINES {
            self.logs.pop_front();
        }
    }

    fn read_plugin_revisions(&mut self) {
        self.plugin_revision = project::plugin_revision(&self.project.file("Cargo.lock"));
        self.latest_plugin_revision = project::prebuild_plugin_revision();
    }

    /// Starts the game paused in edit mode, or already playing with `play`.
    fn launch(&mut self, play: bool, ctx: &egui::Context) {
        self.starting = false;
        self.scene.reset();
        self.history.clear();
        // The previous game has exited, so nothing renders into them any more.
        self.retired.clear();
        // Requests are numbered per game process.
        self.viewport_request = None;
        self.seq = 0;
        self.protocol = None;
        self.ensure_viewport();
        let shared = self.viewport.as_ref().zip(self.render.as_ref());
        let shared = shared.map(|(viewport, render)| (viewport.name.clone(), adapter_name(render)));
        // Read by besfa_editor_plugin: start paused, play on `play`.
        let mut env = vec![("BESFA_EDIT_MODE", "1")];
        if let Some((name, adapter)) = &shared {
            // The plugin takes the size from the texture itself. It opens the
            // texture on D3D12, on the editor's GPU.
            env.extend([
                ("BESFA_VIEWPORT", name.as_str()),
                ("WGPU_BACKEND", "dx12"),
                ("WGPU_ADAPTER_NAME", adapter),
            ]);
        }
        // Dynamic linking makes rebuilds after a code change much faster.
        // Only the editor turns it on, so a plain `cargo build` stays
        // standalone.
        let args = ["run", "--features", "bevy/dynamic_linking"];
        match CliProcess::start(Path::new(&self.project.path), "cargo", &args, &env, ctx) {
            Ok(game) => self.game = Some(game),
            Err(error) => return self.log(format!("Could not start cargo: {error}")),
        }
        // Sent while cargo still builds, these wait in the pipe for the game.
        if play {
            self.send(Command::Play);
        }
        // The scene is rebuilt from the same file, so the selection carries over.
        if let Some(id) = self.scene.selected {
            self.send(Command::Select { id: Some(id) });
        }
        if self.tool != Tool::Translate {
            self.send(Command::Tool { tool: self.tool });
        }
        // The game plays without the preview; a new edit session loads it again.
        if play {
            self.previewing = false;
        } else if self.previewing
            && let Some(preview) = &mut self.preview
        {
            for command in preview.commands() {
                self.send(command);
            }
        }
        self.playing = play;
    }

    /// Unsaved changes are saved first: Stop restarts from the file.
    fn play(&mut self, ctx: &egui::Context) {
        self.logs.clear();
        self.input.focus_next = true;
        if self.game.is_none() {
            // The edit session is gone, e.g. it failed to build or crashed.
            return self.launch(true, ctx);
        }
        if self.scene.dirty {
            self.save();
        }
        // Playing drops the game's preview.
        self.previewing = false;
        self.send(Command::Play);
        self.playing = true;
    }

    /// Stops the game and starts a new edit session once it has exited: the
    /// running game locks its executable against the next build.
    fn restart(&mut self, ctx: &egui::Context) {
        match &self.game {
            Some(game) => {
                self.starting = true;
                self.relaunch = true;
                game.stop();
            }
            None => self.launch(false, ctx),
        }
    }

    /// Sends `command` and returns the number the game answers it with.
    /// Input goes many times a second, so it is not numbered.
    fn send(&mut self, command: Command) -> Option<u64> {
        let game = self.game.as_mut()?;
        let seq = (!command.is_input()).then(|| {
            self.seq += 1;
            self.seq
        });
        self.scene.sent(&command);
        if let Some(preview) = &mut self.preview {
            preview.sent(&command);
        }
        game.send(&serde_json::to_string(&Request { seq, command }).unwrap());
        seq
    }

    /// Sends a command that changes the scene, recording `undo`, the
    /// commands that take it back. A change nothing can take back, like
    /// removing an unreflected component, leaves no entry. Changes made while
    /// playing end with the play session, so they do not count as unsaved.
    fn edit(&mut self, command: Command, undo: Vec<Command>) {
        let seq = self.send(command.clone());
        if !undo.is_empty() {
            self.record(Edit {
                undo,
                redo: vec![command],
                seq,
            });
        }
        if !self.playing {
            self.scene.mark_dirty();
        }
    }

    /// Adds `edit` to the history. An undone change it forgets may have left
    /// an entity hidden for redo; nothing can restore that entity now, so the
    /// game despawns it.
    fn record(&mut self, edit: Edit) {
        for forgotten in self.history.add(edit) {
            for command in forgotten.undo {
                if let Command::Delete { id } = command {
                    self.send(Command::Despawn { id });
                }
            }
        }
    }

    /// Undo and redo replay commands; the game's ids stay valid for the edit
    /// session, which is where the history lives.
    fn replay(&mut self, undo: bool) {
        if self.playing {
            return;
        }
        let commands = if undo {
            self.history.undo()
        } else {
            self.history.redo()
        };
        for command in commands.into_iter().flatten() {
            self.send(command);
            self.scene.mark_dirty();
        }
    }

    /// Records what the game did on the editor's behalf, notices saves and
    /// answers to requests, and checks the plugin's protocol.
    fn on_report(&mut self, report: Report, ctx: &egui::Context) {
        match report {
            Report::Hello { version } => {
                self.protocol = Some(version);
                if let Some(warning) = protocol_warning(version) {
                    self.log(warning);
                }
            }
            // Reports come before any hello only from an early plugin.
            Report::Entities { .. } if self.protocol.is_none() => {
                self.protocol = Some(0);
                self.log(protocol_warning(0).unwrap_or_default());
            }
            Report::Spawned { id } => self.record(Edit {
                undo: vec![Command::Delete { id }],
                redo: vec![Command::Restore { id }],
                seq: None,
            }),
            Report::Edited {
                id,
                component,
                before,
                after,
            } => {
                let set = |value| Command::Set {
                    id,
                    component: component.clone(),
                    value,
                };
                self.record(Edit {
                    undo: vec![set(before)],
                    redo: vec![set(after)],
                    seq: None,
                });
            }
            Report::Preview { path, model } => {
                if let Some(error) = &model.error {
                    self.log(format!("Could not preview {path}: {error}"));
                }
                if let Some(preview) = self.preview.as_mut().filter(|preview| preview.path == path)
                {
                    for command in preview.loaded(model) {
                        self.send(command);
                    }
                }
            }
            // The scene took it if it is the selection's.
            Report::Entity {
                id,
                changes,
                components,
            } => {
                if let Some(preview) = &mut self.preview {
                    preview.entity_report(id, changes, components, self.scene.changes());
                }
            }
            Report::Screenshot { path, error } => {
                match &error {
                    None => self.log(format!("Saved {path}")),
                    Some(error) => self.log(format!("Could not save {path}: {error}")),
                }
                if let Some(preview) = &mut self.preview {
                    preview.screenshot(path, error);
                }
            }
            Report::Saved { .. } => {
                if let Some((_, then)) = self.pending_save.take() {
                    self.after_save(then, ctx);
                }
            }
            // The game logs why a request failed; a change it could not make
            // leaves nothing to undo.
            Report::Done {
                seq,
                error: Some(_),
            } => self.history.discard(seq),
            // The game renders into the newest texture now.
            Report::Done { seq, error: None } if Some(seq) == self.viewport_request => {
                self.retired.clear();
            }
            _ => {}
        }
    }

    /// The game reports whether saving worked, which clears the dirty mark.
    fn save(&mut self) {
        if self.game.is_some() && !self.playing {
            self.send(Command::Save);
        }
    }

    /// Saves and does `then` once the game says it did, or after a while.
    fn save_then(&mut self, then: AfterSave, ctx: &egui::Context) {
        self.save();
        self.pending_save = Some((Instant::now() + SAVE_TIMEOUT, then));
        ctx.request_repaint_after(SAVE_TIMEOUT);
    }

    fn after_save(&mut self, then: AfterSave, ctx: &egui::Context) {
        match then {
            AfterSave::Rebuild => self.restart(ctx),
            // A failed save keeps the changes unsaved; the log says why.
            AfterSave::Leave => self.leave = !self.scene.dirty,
            AfterSave::Close if !self.scene.dirty => self.close(ctx),
            AfterSave::Close => {}
        }
    }

    fn close(&mut self, ctx: &egui::Context) {
        self.closing = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    /// Goes back to the hub, asking about unsaved changes first.
    fn request_leave(&mut self) {
        if self.scene.dirty {
            self.dialog = Some(Dialog::Unsaved(AfterSave::Leave));
        } else {
            self.leave = true;
        }
    }

    /// Restarts the edit session when the game's code changed, which
    /// rebuilds the game, saving unsaved changes first. A play session is
    /// left alone: Stop rebuilds anyway.
    fn rebuild(&mut self, prebuild: &Prebuild, ctx: &egui::Context) {
        if self.playing || self.starting || prebuild.running() {
            return;
        }
        self.log("Source changed; rebuilding.");
        if self.game.is_some() && self.scene.dirty {
            self.starting = true;
            self.save_then(AfterSave::Rebuild, ctx);
        } else {
            self.restart(ctx);
        }
    }

    /// Moves the project's plugin to the latest commit and restarts the edit
    /// session, which rebuilds the game against it.
    fn update_plugin(&mut self, ctx: &egui::Context) {
        self.starting = true;
        let args = ["update", "-p", "besfa_editor_plugin"];
        match CliProcess::start(Path::new(&self.project.path), "cargo", &args, &[], ctx) {
            Ok(update) => self.update = Some(update),
            Err(error) => {
                self.log(format!("Could not start cargo: {error}"));
                self.after_update(ctx);
            }
        }
    }

    fn after_update(&mut self, ctx: &egui::Context) {
        self.read_plugin_revisions();
        self.restart(ctx);
    }

    fn set_command(id: u64, component: &str, value: Value) -> Command {
        Command::Set {
            id,
            component: component.into(),
            value,
        }
    }

    /// The selected entity's current value of `component`, as reported.
    fn current_value(&self, component: &str) -> Option<Value> {
        let reported = self
            .scene
            .components
            .iter()
            .find(|reported| reported.path == component);
        reported.and_then(|reported| reported.value.clone())
    }

    fn on_inspector(&mut self, action: Action) {
        // Components show only for a selected entity.
        let Some(id) = self.scene.selected else {
            return;
        };
        match action {
            Action::Set(component, value) => {
                let before = self.current_value(&component).unwrap_or_default();
                let undo = Self::set_command(id, &component, before);
                self.edit(Self::set_command(id, &component, value), vec![undo]);
            }
            Action::Insert(component) => self.edit(
                Command::Insert {
                    id,
                    component: component.clone(),
                },
                vec![Command::Remove { id, component }],
            ),
            // Setting a missing component adds it back with its value.
            Action::Remove(component) => {
                let undo = self
                    .current_value(&component)
                    .map(|value| Self::set_command(id, &component, value));
                self.edit(
                    Command::Remove { id, component },
                    undo.into_iter().collect(),
                );
            }
            // Writes a file, not the scene: nothing to undo or save.
            Action::ApplyPrefab(id) => {
                self.send(Command::ApplyPrefab { id });
            }
        }
    }

    fn on_hierarchy(&mut self, action: HierarchyAction) {
        match action {
            // The game reports the selected entity's components back.
            HierarchyAction::Select(id) => {
                self.scene.select(id);
                self.send(Command::Select { id });
            }
            // The game reports what it spawns, which records the undo.
            HierarchyAction::Spawn(kind) => self.edit(Command::Spawn { kind }, vec![]),
            HierarchyAction::Duplicate => self.duplicate(),
            HierarchyAction::Delete => self.delete(),
            HierarchyAction::SavePrefab => {
                if let Some(entity) = self.scene.selected_entity() {
                    self.dialog = Some(Dialog::PrefabName(
                        entity.name.clone().unwrap_or("Prefab".into()),
                    ));
                }
            }
        }
    }

    fn duplicate(&mut self) {
        if let Some(entity) = self.scene.selected_entity() {
            self.edit(Command::Duplicate { id: entity.id }, vec![]);
        }
    }

    /// The game hides the entity, so undoing brings the same one back.
    fn delete(&mut self) {
        if let Some(entity) = self.scene.selected_entity() {
            let id = entity.id;
            self.edit(Command::Delete { id }, vec![Command::Restore { id }]);
        }
    }

    /// Puts the asset at `path` in the scene; the game reports it as spawned.
    fn instantiate(&mut self, path: String) {
        self.edit(Command::Instantiate { path }, vec![]);
    }

    /// Opens the model at `path` in the preview tab, replacing the model
    /// there, once the game's plugin knows how.
    fn open_preview(&mut self, path: String) {
        if self.protocol.is_none_or(|version| version < 3) {
            return self.log(
                "The model preview needs the game running with besfa_editor_plugin speaking \
                 protocol 3 or later. Update the plugin.",
            );
        }
        self.preview = Some(ModelPreview::new(path, PathBuf::from(&self.project.path)));
        self.show_preview(true);
    }

    /// Shows the preview tab, or the scene; the game drops the model while
    /// the scene shows and loads it again on the way back.
    fn show_preview(&mut self, show: bool) {
        self.previewing = show && self.preview.is_some();
        if self.previewing {
            let commands = self.preview.as_mut().map(ModelPreview::commands);
            for command in commands.into_iter().flatten() {
                self.send(command);
            }
        } else {
            self.send(Command::Preview { path: None });
        }
    }

    fn close_preview(&mut self) {
        if self.previewing {
            self.show_preview(false);
        }
        self.preview = None;
    }

    /// The Scene tab and the model preview's tab, while one is open.
    fn tabs(&mut self, ui: &mut Ui) {
        let Some(preview) = &self.preview else {
            return;
        };
        let name = format!("⛶ {}", preview.name());
        let path = preview.path.clone();
        let mut show = None;
        let mut close = false;
        ui.horizontal(|ui| {
            if ui.selectable_label(!self.previewing, "Scene").clicked() && self.previewing {
                show = Some(false);
            }
            let tab = Button::selectable(self.previewing, name);
            if ui
                .add_enabled(!self.playing, tab)
                .on_hover_text(path)
                .clicked()
                && !self.previewing
            {
                show = Some(true);
            }
            close = ui
                .small_button("🗙")
                .on_hover_text("Close the preview")
                .clicked();
        });
        if close {
            self.close_preview();
        } else if let Some(show) = show {
            self.show_preview(show);
        }
    }

    fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.send(Command::Tool { tool });
    }

    /// Scene shortcuts. Delete, Ctrl+D and undo leave text fields alone,
    /// and a focused field is committed before Ctrl+S saves. While the game
    /// plays, keys are the game's: the viewport hands them over.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.playing || self.input.looking(self.playing) {
            return;
        }
        let pressed = |modifiers, key| {
            ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
        };
        if pressed(Modifiers::COMMAND, Key::S) {
            // The focused field commits as it loses focus this frame.
            ctx.memory_mut(|memory| memory.stop_text_input());
            self.save_after_ui = true;
            return;
        }
        // Unity's key for its Asset Store.
        if pressed(Modifiers::COMMAND, Key::Num9) {
            return project::open_url(project::BEVY_ASSETS);
        }
        if ctx.text_edit_focused() {
            return;
        }
        // The scene is out of sight: only framing the model is left.
        if self.previewing {
            if pressed(Modifiers::NONE, Key::F) {
                self.send(Command::Focus);
            }
            return;
        }
        if pressed(Modifiers::COMMAND, Key::D) {
            self.duplicate();
        } else if pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
            || pressed(Modifiers::COMMAND, Key::Y)
        {
            self.replay(false);
        } else if pressed(Modifiers::COMMAND, Key::Z) {
            self.replay(true);
        } else if pressed(Modifiers::NONE, Key::Delete) {
            self.delete();
        } else if pressed(Modifiers::NONE, Key::F) {
            self.send(Command::Focus);
        } else if let Some((tool, ..)) = TOOLS
            .into_iter()
            .find(|(.., key)| pressed(Modifiers::NONE, *key))
        {
            self.set_tool(tool);
        }
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    // Only the edit session's scene is worth keeping.
                    let can_save = self.game.is_some() && !self.playing;
                    if ui
                        .add_enabled(can_save, Button::new("Save scene").shortcut_text("Ctrl+S"))
                        .clicked()
                    {
                        self.save_after_ui = true;
                    }
                    if ui.button("Back to Project Hub").clicked() {
                        self.request_leave();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    let undo = Button::new("Undo").shortcut_text("Ctrl+Z");
                    if ui
                        .add_enabled(self.history.can_undo() && !self.playing, undo)
                        .clicked()
                    {
                        self.replay(true);
                    }
                    let redo = Button::new("Redo").shortcut_text("Ctrl+Y");
                    if ui
                        .add_enabled(self.history.can_redo() && !self.playing, redo)
                        .clicked()
                    {
                        self.replay(false);
                    }
                });
                ui.menu_button("Window", |ui| {
                    let assets = Button::new("Bevy Assets").shortcut_text("Ctrl+9");
                    if ui.add(assets).on_hover_text(project::BEVY_ASSETS).clicked() {
                        project::open_url(project::BEVY_ASSETS);
                    }
                });
            });
        });
    }

    fn toolbar(&mut self, ui: &mut Ui, prebuild: &Prebuild, ctx: &egui::Context) {
        let busy = self.starting || prebuild.running();
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                let (label, play) = if self.playing {
                    ("⏹ Stop", false)
                } else {
                    ("▶ Play", true)
                };
                if ui.add_enabled(!busy, Button::new(label)).clicked() {
                    if play {
                        self.play(ctx)
                    } else {
                        self.restart(ctx)
                    }
                }
                ui.add_space(12.0);
                for (tool, icon, tooltip, _) in TOOLS {
                    let button = Button::selectable(self.tool == tool, icon);
                    if ui
                        .add_enabled(!self.playing && !self.previewing, button)
                        .on_hover_text(tooltip)
                        .clicked()
                    {
                        self.set_tool(tool);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    prebuild_status(ui, prebuild);
                    ui.add_space(16.0);
                    self.plugin_status(ui, busy, ctx);
                });
            });
        });
    }

    /// Whether the project's `besfa_editor_plugin` matches the one the Bevy
    /// prebuild has and speaks this editor's protocol, as an icon that
    /// updates the plugin when pressed. Hidden while neither is known: the
    /// lock does not pin the plugin to a git revision and the game has not
    /// said which protocol it speaks.
    fn plugin_status(&mut self, ui: &mut Ui, busy: bool, ctx: &egui::Context) {
        let warning = self.protocol.and_then(protocol_warning);
        if self.plugin_revision.is_none() && warning.is_none() {
            return;
        }
        // Different, not necessarily older: a push made while the editor
        // runs leaves the project ahead of the prebuild.
        let differs = self.plugin_revision.as_ref().is_some_and(|project| {
            self.latest_plugin_revision
                .as_ref()
                .is_some_and(|latest| latest != project)
        });
        let (icon, color, state) = if differs || warning.is_some() {
            ("⚠", WARNING, "differs from")
        } else {
            ("✔", GOOD, "matches")
        };
        let tooltip = match warning {
            Some(warning) => format!("{warning} Click to update it to the latest commit."),
            None => format!(
                "The plugin {state} the Bevy prebuild. Click to update it to the latest commit."
            ),
        };
        let button = Button::new(RichText::new(icon).color(color)).frame(false);
        if ui
            .add_enabled(!busy, button)
            .on_hover_text(tooltip)
            .clicked()
        {
            self.update_plugin(ctx);
        }
    }

    fn viewport_ui(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        self.tabs(ui);
        let panel = ui.available_rect_before_wrap();
        self.fit_viewport(panel.size() * ui.pixels_per_point(), ctx);
        let shown = self.viewport.as_ref().filter(|_| self.game.is_some());
        let Some((texture, size)) =
            shown.map(|v| (v.texture_id, egui::vec2(v.width as f32, v.height as f32)))
        else {
            ui.centered_and_justified(|ui| ui.label(RichText::new("Viewport").color(MUTED)));
            return;
        };
        let scale = (panel.width() / size.x).min(panel.height() / size.y);
        let rect = egui::Rect::from_center_size(panel.center(), size * scale);
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().image(texture, rect, uv, Color32::WHITE);
        let mut commands = Vec::new();
        let response = self.input.ui(ui, rect, size, self.playing, &mut |command| {
            commands.push(command)
        });
        for command in commands {
            self.send(command);
        }
        if !self.playing {
            let hint = if self.previewing {
                "Drag orbit  ·  Middle-drag pan  ·  Scroll zoom  ·  F fit"
            } else {
                "Click select  ·  Right-drag orbit  ·  Right-hold + WASD/QE fly  ·  Middle-drag pan  ·  Scroll zoom  ·  F focus"
            };
            ui.painter().text(
                rect.left_bottom() + egui::vec2(8.0, -8.0),
                egui::Align2::LEFT_BOTTOM,
                hint,
                egui::FontId::proportional(11.0),
                Color32::from_white_alpha(0x99),
            );
        }
        // Assets dragged from the Assets tab land in the scene, when it shows.
        if self.previewing {
            return;
        }
        if response.dnd_hover_payload::<String>().is_some() {
            ui.painter()
                .rect_stroke(rect, 0.0, (2.0, ACCENT), egui::StrokeKind::Inside);
        }
        if let Some(path) = response.dnd_release_payload::<String>() {
            self.instantiate((*path).clone());
        }
    }

    /// Creates the viewport texture once per editor. Without it the game
    /// opens its own window, so failures are logged rather than fatal.
    fn ensure_viewport(&mut self) {
        if self.viewport.is_some() {
            return;
        }
        let [width, height] = self.wanted_viewport.unwrap_or(DEFAULT_VIEWPORT);
        let created = match &self.render {
            Some(render) => ViewportTexture::create(render, width, height),
            None => Err("the editor is not rendering with wgpu".into()),
        };
        match created {
            Ok(viewport) => self.viewport = Some(viewport),
            Err(error) => self.log(format!(
                "Viewport unavailable, the game opens its own window: {error}"
            )),
        }
    }

    /// Follows the viewport panel's size, `physical` pixels, once it settles.
    fn fit_viewport(&mut self, physical: egui::Vec2, ctx: &egui::Context) {
        let size = [physical.x, physical.y].map(|side| side.round().clamp(16.0, 4096.0) as u32);
        if Some(size) != self.wanted_viewport {
            self.wanted_viewport = Some(size);
            self.resize_at = Some(Instant::now() + RESIZE_DELAY);
            ctx.request_repaint_after(RESIZE_DELAY);
        }
    }

    /// Makes a texture of the wanted size, shows it, and tells the game to
    /// render into it. The old texture is kept until the game moves.
    fn resize_viewport(&mut self) {
        self.resize_at = None;
        let (Some(old), Some([width, height]), Some(render)) =
            (&self.viewport, self.wanted_viewport, &self.render)
        else {
            return;
        };
        if [old.width, old.height] == [width, height] {
            return;
        }
        match ViewportTexture::create(render, width, height) {
            Ok(viewport) => {
                let name = viewport.name.clone();
                self.retired.extend(self.viewport.replace(viewport));
                self.viewport_request = self.send(Command::Viewport { name });
            }
            Err(error) => self.log(format!("Could not resize the viewport: {error}")),
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        match &mut self.dialog {
            Some(Dialog::PrefabName(name)) => {
                let mut submit = false;
                let mut cancel = false;
                let valid = {
                    let name = name.trim();
                    !name.is_empty()
                        && name
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || "_- ".contains(c))
                };
                let modal = egui::Modal::new(egui::Id::new("prefab_name")).show(ctx, |ui| {
                    ui.set_width(360.0);
                    ui.heading("Save as prefab");
                    ui.label("Prefab name");
                    let field =
                        ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                    if ui.memory(|memory| memory.focused().is_none()) {
                        field.request_focus();
                    }
                    submit = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    if valid {
                        ui.label(
                            RichText::new("Saved to assets/prefabs")
                                .small()
                                .color(MUTED),
                        );
                    } else {
                        ui.label(
                            RichText::new("Letters, digits, spaces, - and _ only")
                                .small()
                                .color(panels::ERROR),
                        );
                    }
                    ui.horizontal(|ui| {
                        cancel = ui.button("Cancel").clicked();
                        submit |= ui.button("Save").clicked();
                    });
                });
                if submit && valid {
                    let path = format!("prefabs/{}.scn.ron", name.trim());
                    self.dialog = None;
                    if let Some(id) = self.scene.selected {
                        self.send(Command::SavePrefab { id, path });
                    }
                } else if cancel || modal.should_close() {
                    self.dialog = None;
                }
            }
            Some(Dialog::Unsaved(then)) => {
                let then = *then;
                // The changes live in the edit session; without it they are gone.
                let can_save = self.game.is_some() && !self.playing;
                let mut choice = None;
                let modal = egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
                    ui.set_width(380.0);
                    ui.heading("Save changes to the scene?");
                    ui.label("The scene has changes that are not saved to scenes/main.scn.ron.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            choice = Some("cancel");
                        }
                        if ui.button("Discard").clicked() {
                            choice = Some("discard");
                        }
                        if can_save && ui.button("Save").clicked() {
                            choice = Some("save");
                        }
                    });
                });
                if modal.should_close() {
                    choice = choice.or(Some("cancel"));
                }
                if let Some(choice) = choice {
                    self.dialog = None;
                    match (choice, then) {
                        ("save", _) => self.save_then(then, ctx),
                        ("discard", AfterSave::Close) => self.close(ctx),
                        ("discard", _) => self.leave = true,
                        _ => {}
                    }
                }
            }
            None => {}
        }
    }
}

/// Why a game whose plugin speaks protocol `version` needs a look, if it
/// does.
fn protocol_warning(version: u32) -> Option<String> {
    use std::cmp::Ordering;
    match version.cmp(&VERSION) {
        Ordering::Equal => None,
        Ordering::Less if version == 0 => Some(
            "The game's besfa_editor_plugin predates protocol versions, so failed edits \
             stay in the undo history. Update the plugin."
                .into(),
        ),
        Ordering::Less => Some(format!(
            "The game's besfa_editor_plugin speaks protocol {version}, older than this \
             editor's {VERSION}. Update the plugin."
        )),
        Ordering::Greater => Some(format!(
            "The game's besfa_editor_plugin speaks protocol {version}, newer than this \
             editor's {VERSION}. Update the editor."
        )),
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        for process in self.game.iter().chain(&self.update) {
            process.stop();
        }
    }
}

/// How often the game's source folder is checked for changes.
const SOURCE_POLL: Duration = Duration::from_millis(500);

/// Notices when a Rust file under the game's `src` changes, once the changes
/// settle.
// ponytail: polls modification times; watch the folder (notify) if that
// ever costs.
struct SourceWatch {
    directory: PathBuf,
    files: Vec<(PathBuf, SystemTime)>,
    polled: Instant,
    changed: Option<Instant>,
}

impl SourceWatch {
    fn new(directory: PathBuf) -> Self {
        Self {
            files: rust_files(&directory),
            directory,
            polled: Instant::now(),
            changed: None,
        }
    }

    /// True once, after a change has stood still for a poll.
    fn settled(&mut self, ctx: &egui::Context) -> bool {
        ctx.request_repaint_after(SOURCE_POLL);
        if self.polled.elapsed() < SOURCE_POLL {
            return false;
        }
        self.polled = Instant::now();
        let files = rust_files(&self.directory);
        if files != self.files {
            self.files = files;
            self.changed = Some(Instant::now());
            return false;
        }
        self.changed.take().is_some()
    }
}

/// The `.rs` files under `directory` with their modification times.
fn rust_files(directory: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut files = Vec::new();
    let mut folders = vec![directory.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let modified = entry.metadata().and_then(|metadata| metadata.modified());
                files.push((path, modified.unwrap_or(SystemTime::UNIX_EPOCH)));
            }
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_a_source_change_once_it_settles() {
        let dir = std::env::temp_dir().join(format!("besfa_source_{}", std::process::id()));
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(dir.join("main.rs"), "").unwrap();
        let ctx = egui::Context::default();
        let mut watch = SourceWatch::new(dir.clone());
        let mut poll = || {
            watch.polled -= SOURCE_POLL;
            watch.settled(&ctx)
        };
        assert!(!poll(), "nothing changed");
        fs::write(dir.join("nested/lib.rs"), "").unwrap();
        fs::write(dir.join("notes.txt"), "").unwrap();
        assert!(!poll(), "still changing");
        assert!(poll(), "settled");
        assert!(!poll(), "once");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn warns_about_a_plugin_that_speaks_another_protocol() {
        assert_eq!(protocol_warning(VERSION), None);
        assert!(
            protocol_warning(0)
                .unwrap()
                .contains("predates protocol versions")
        );
        assert!(
            protocol_warning(VERSION + 1)
                .unwrap()
                .contains("Update the editor")
        );
    }

    /// The editor and what the README shows of it, for the screenshots.
    struct Screen {
        editor: Editor,
        prebuild: Prebuild,
        /// Ringed in the picture.
        highlight: Option<egui::Rect>,
    }

    /// Renders the editor for the README's screenshots, with the fonts the
    /// running editor uses, and writes them to docs/images. Not part of
    /// `cargo test`; run it after changing the UI:
    ///
    /// ```text
    /// cargo test -p besfa_editor readme_screenshots -- --ignored
    /// ```
    #[test]
    #[ignore = "writes docs/images"]
    fn readme_screenshots() {
        const OLD: &str = "28b39927dda69b765ee1e7071794050033b4a307";
        const LATEST: &str = "51b56bc0a1d9cd2bd1f1c5d6a2a8d5c9e0f1a2b3";
        // What the template game prints and reports once it runs.
        let game_output = [
            r"   Compiling my_game v0.1.0 (C:\Projects\my_game)",
            "    Finished `dev` profile [optimized + debuginfo] target(s) in 5.67s",
            r"     Running `C:\Users\you\AppData\Local\Besfa\target\debug\my_game.exe`",
            "2026-10-07T05:36:08.737706Z  INFO besfa_editor_plugin::viewport: Rendering into the editor viewport 1280x720.",
            "2026-10-07T05:36:08.776701Z  INFO besfa_editor_plugin::scene: Loaded scenes/main.scn.ron",
            r#"@besfa {"type":"systems","crate":"my_game","systems":[{"schedule":"Update","name":"my_game::spin"}]}"#,
            r#"@besfa {"type":"entities","entities":[{"id":4294967295,"name":"Cube"},{"id":4294967294,"name":"Light"},{"id":4294967293,"name":"Camera"}]}"#,
        ];
        let read = |editor: &mut Editor, lines: &[&str]| {
            for line in lines {
                if editor.scene.handle(line).is_none() {
                    editor.log(*line);
                }
            }
        };

        // Waiting for a prebuild, the editor starts no game of its own.
        let waiting = Prebuild::with_phase(crate::process::PrebuildPhase::Running, "");
        let project = Project::new(r"C:\Projects\my_game");
        let mut editor = Editor::new(project, None, &waiting, &egui::Context::default());
        editor.waiting_for_prebuild = false;
        editor.plugin_revision = Some(OLD.into());
        editor.latest_plugin_revision = Some(LATEST.into());
        read(&mut editor, &game_output);

        let screen = Screen {
            editor,
            prebuild: Prebuild::with_phase(crate::process::PrebuildPhase::Ready, "Bevy is ready"),
            highlight: None,
        };
        let mut harness = egui_kittest::Harness::builder()
            // The window, and the margin the harness puts around it.
            .with_size([1280.0 + 2.0 * MARGIN, 800.0 + 2.0 * MARGIN])
            .with_pixels_per_point(PIXELS_PER_POINT)
            .wgpu()
            .build_ui_state(
                |ui, screen: &mut Screen| {
                    screen.editor.ui(ui, &screen.prebuild);
                    if let Some(rect) = screen.highlight {
                        let layer =
                            egui::LayerId::new(egui::Order::Debug, egui::Id::new("highlight"));
                        ui.ctx().layer_painter(layer).rect_stroke(
                            rect.expand(3.0),
                            6.0,
                            (2.0, Color32::from_rgb(0xFF, 0x5F, 0x57)),
                            egui::StrokeKind::Outside,
                        );
                    }
                },
                screen,
            );
        crate::style(&harness.ctx);
        harness.run();

        // The project's plugin is not the prebuild's: the toolbar warns, and
        // hovering explains what clicking does.
        let rect = plugin_icon(&harness, "⚠");
        harness.hover_at(rect.center());
        harness.state_mut().highlight = Some(rect);
        // Past the tooltip's delay.
        harness.run_steps(8);
        save(&mut harness, "plugin-update-available", Some(110.0));

        // Clicking updates it, restarts the game and logs what cargo did.
        harness.remove_cursor();
        let editor = &mut harness.state_mut().editor;
        read(
            editor,
            &[
                "    Updating git repository `https://github.com/ski0090/besfa`",
                &format!(
                    "    Updating besfa_editor_plugin v0.1.0 (https://github.com/ski0090/besfa#{}) -> #{}",
                    &OLD[..8],
                    &LATEST[..8]
                ),
                "Process exited with code 1.",
            ],
        );
        editor.scene.reset();
        read(editor, &game_output);
        editor.plugin_revision = Some(LATEST.into());
        harness.run();
        harness.state_mut().highlight = Some(plugin_icon(&harness, "✔"));
        harness.run();
        save(&mut harness, "plugin-updated", None);
    }

    /// Keeps text sharp.
    const PIXELS_PER_POINT: f32 = 1.5;

    /// The harness's outer margin around the UI, in points.
    const MARGIN: f32 = 8.0;

    /// Where the toolbar's plugin icon showing `icon` is, in points.
    fn plugin_icon(harness: &egui_kittest::Harness<Screen>, icon: &str) -> egui::Rect {
        use egui_kittest::kittest::Queryable;
        harness
            .get_by_role_and_label(egui::accesskit::Role::Button, icon)
            .rect()
    }

    /// Writes the window, or its top `height` points, to docs/images.
    fn save(harness: &mut egui_kittest::Harness<Screen>, name: &str, height: Option<f32>) {
        let image = harness.render().expect("render the editor");
        let margin = (MARGIN * PIXELS_PER_POINT) as u32;
        let width = image.width() - 2 * margin;
        let height = height.map_or(image.height() - 2 * margin, |points| {
            (points * PIXELS_PER_POINT) as u32
        });
        let image = image::imageops::crop_imm(&image, margin, margin, width, height).to_image();
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../docs/images/{name}.png"));
        image.save(path).expect("write the screenshot");
    }
}
