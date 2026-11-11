//! The editor for one project. The game runs in one of two states: paused
//! in edit mode, where the viewport shows the scene as Startup built it, or
//! playing. Stop ends play by resetting the scene to its file in the same
//! process, or by relaunching the game in edit mode when its code changed. In both states the game
//! reports its entities, which the hierarchy and inspector panels show and
//! change through commands on its stdin, one JSON object per line. A net
//! Play runs the game as a dedicated server and as the clients joined to it
//! instead; see docs/ARCHITECTURE.md.

use std::{
    collections::VecDeque,
    fs,
    net::{Ipv4Addr, UdpSocket},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

use besfa_protocol::{
    Command, HIERARCHY_PROTOCOL, INSPECTOR_PROTOCOL, LANGUAGE_ENV, PANELS_ENV, PanelAction,
    Placement, Report, Request, SCALE_ENV, ScatterBrush, SpawnKind, TerrainBrush, Tool, VERSION,
};
use eframe::{
    egui::{
        self, Button, Color32, Key, KeyboardShortcut, Modifiers, RichText, Ui, ViewportCommand,
    },
    egui_wgpu::RenderState,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    agent::{self, AgentPanel, AgentSettings, Agents, Route},
    animation,
    i18n::{self, Language, t},
    inspector::{Action, TRANSFORM},
    organization::{ORGANIZATIONS_DIR, OrgPanel},
    panels::{
        self, ACCENT, AssetAction, AssetBrowser, GOOD, MUTED, Reimport, WARNING, prebuild_status,
    },
    preview::ModelPreview,
    process::{CliProcess, Output, Prebuild},
    project::{self, Project},
    scatter::ScatterTools,
    scene::{Edit, EditHistory, Scene},
    terrain::{TERRAIN, TerrainTools},
    viewport::{ViewportInput, ViewportTexture, adapter_name, path_with_dxc},
};

/// The protocol whose plugins make and sculpt terrain from the editor.
const TERRAIN_PROTOCOL: u32 = 6;

/// The protocols whose plugins drop things to the ground and scatter them
/// with the scene view's brush.
const DROP_PROTOCOL: u32 = 7;
const SCATTER_PROTOCOL: u32 = 8;

/// The protocol whose plugins place things where the viewport points, show
/// a dragged asset's ghost and snap handles.
const PLACE_PROTOCOL: u32 = 11;

/// The protocol whose plugins end play in the same process, which keeps
/// the shaders they compiled.
const RESET_PROTOCOL: u32 = 13;

/// Where a project keeps the scene view's snapping, in the project.
const SNAP_FILE: &str = "besfa/snap.besfa";
/// The scene the editor edits, which the agent may change too.
const SCENE_FILE: &str = "assets/scenes/main.scn.ron";

/// The log keeps the latest lines.
const MAX_LOG_LINES: usize = 2000;

/// The most clients a net Play starts: as many players as a duskhold world
/// takes.
const MAX_CLIENTS: usize = 4;

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
    /// A Play: a net Play's server and clients read the scene file, and
    /// Stop restarts from it.
    Play,
    /// A plugin update, which restarts the game from the scene file.
    Update,
    Leave,
    Close,
}

/// What the right panel shows while the scene is in the viewport, when
/// both have something to show. The game draws the Inspector.
#[derive(Clone, Copy, PartialEq)]
enum RightTab {
    Terrain,
    Scatter,
}

/// What the bottom panel shows.
#[derive(Clone, Copy, PartialEq)]
enum BottomTab {
    Log,
    Assets,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AgentView {
    Chat,
    Organization,
    Settings,
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
    /// The terrain brush the game paints with, as last sent.
    brush: Option<TerrainBrush>,
    /// The scatter brush, as last sent.
    scatter_brush: Option<ScatterBrush>,
    scatter: ScatterTools,
    right_tab: RightTab,
    /// The viewport panel's size in physical pixels, which the texture
    /// follows once it settles.
    wanted_viewport: Option<[u32; 2]>,
    /// The pixels a point takes in the editor's window, for the panels the
    /// game draws.
    scale: f32,
    resize_at: Option<Instant>,
    input: ViewportInput,
    tool: Tool,
    snapping: Snapping,
    /// The snap last sent: the grid, and whether handles snap, from the
    /// toolbar or with Ctrl held.
    sent_snap: Option<(f32, bool)>,
    /// The asset dragged over the viewport and where, whose ghost the game
    /// shows, as last sent.
    ghost: Option<(String, [f32; 2])>,
    /// Waiting for a game process: stopping one to launch the next, or
    /// updating the plugin first.
    starting: bool,
    /// The game was told to play; otherwise it sits paused in edit mode.
    playing: bool,
    /// Clients a Play starts joined to a dedicated server, the game in the
    /// viewport first; none plays the game alone.
    net_clients: usize,
    /// A net Play's dedicated server and clients in windows, with what
    /// their log lines start with.
    net: Vec<(String, CliProcess)>,
    /// Launch a game, playing or not, once the game and the net Play's
    /// processes have exited.
    relaunch: Option<bool>,
    /// The prebuild holds the shared build directory; the edit session
    /// starts once it is done.
    waiting_for_prebuild: bool,
    /// `besfa_editor_plugin` revisions: the project's and the prebuild's.
    plugin_revision: Option<String>,
    latest_plugin_revision: Option<String>,
    /// A save in flight, until when it is waited for, and what follows it.
    pending_save: Option<(Instant, AfterSave)>,
    /// The game's code changed since the game was built, so Stop rebuilds.
    code_changed: bool,
    /// A rebuild put off until the scene is saved.
    rebuild_after_save: bool,
    /// Save once this frame's fields have committed.
    save_after_ui: bool,
    source: SourceWatch,
    /// The brush for a selected terrain.
    terrain: TerrainTools,
    /// The game's plugin takes the terrain brush, in edit mode.
    can_sculpt: bool,
    /// Whether a field of a panel the game draws has the keyboard, and where
    /// its text cursor is in the viewport's pixels.
    panel_focus: (bool, Option<[f32; 4]>),
    /// The viewport's pixels the panels the game draws cover from its left
    /// and its right edge; the scene view is between them.
    panel_layout: (f32, f32),
    assets: AssetBrowser,
    bottom_tab: BottomTab,
    /// The model preview's tab, while it is open.
    preview: Option<ModelPreview>,
    /// The viewport shows the model preview instead of the scene.
    previewing: bool,
    dialog: Option<Dialog>,
    /// Which agent and model prompts go to, how each agent can be reached,
    /// and whether the settings window is open.
    agent: AgentSettings,
    agents: Agents,
    agent_open: bool,
    /// Whether the Agent chat has its own window.
    agent_chat_open: bool,
    /// Whether the animation state machine has its own window.
    animation_open: bool,
    agent_panel: AgentPanel,
    /// Which tab the Agent window shows, and its Organization tab.
    agent_view: AgentView,
    organization: OrgPanel,
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
        let snapping = Snapping::read(&project.file(SNAP_FILE));
        let organization = OrgPanel::new(project.file(ORGANIZATIONS_DIR));
        // A run the editor closed on goes on from its chat.
        let mut agent_panel = AgentPanel::read(&AgentPanel::chats_file(&project.file("")));
        if let Some(paused) = organization.paused() {
            agent_panel.restore(paused.chat(), paused.files(), paused.organization());
        }
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
            brush: None,
            scatter_brush: None,
            scatter: ScatterTools::default(),
            right_tab: RightTab::Terrain,
            wanted_viewport: None,
            scale: 1.0,
            resize_at: None,
            input: ViewportInput::default(),
            tool: Tool::Translate,
            snapping,
            sent_snap: None,
            ghost: None,
            starting: false,
            playing: false,
            net_clients: 0,
            net: Vec::new(),
            relaunch: None,
            waiting_for_prebuild: prebuild.running(),
            plugin_revision: None,
            latest_plugin_revision: None,
            pending_save: None,
            rebuild_after_save: false,
            code_changed: false,
            save_after_ui: false,
            terrain: TerrainTools::default(),
            can_sculpt: false,
            panel_focus: (false, None),
            panel_layout: (0.0, 0.0),
            bottom_tab: BottomTab::Log,
            preview: None,
            previewing: false,
            dialog: None,
            agent: AgentSettings::read(&AgentSettings::file()),
            agents: Agents::load(),
            agent_open: false,
            agent_chat_open: false,
            animation_open: false,
            agent_panel,
            agent_view: AgentView::Chat,
            organization,
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
        self.poll_processes(&ctx);
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
        let settled = self.source.settled(&ctx);
        self.code_changed |= settled;
        if settled || (self.rebuild_after_save && !self.scene.dirty) {
            self.rebuild(prebuild, &ctx);
        }
        for path in self.assets.poll(&ctx) {
            self.reimport(path);
        }
        self.ask_thumbnails();
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
                    ui.selectable_value(&mut self.bottom_tab, BottomTab::Log, t("Log"));
                    ui.selectable_value(&mut self.bottom_tab, BottomTab::Assets, t("Assets"));
                });
                ui.separator();
                match self.bottom_tab {
                    BottomTab::Log => panels::log(ui, &self.logs),
                    BottomTab::Assets => match self.assets.ui(ui, !self.playing) {
                        Some(AssetAction::Add(path)) => {
                            self.instantiate(path, self.viewport_middle())
                        }
                        Some(AssetAction::Preview(path)) => self.open_preview(path),
                        None => {}
                    },
                }
            });
        // The game draws the Hierarchy in the viewport. A plugin from before
        // it did leaves the game without one.
        if self.protocol.is_some() && !self.draws_hierarchy() {
            egui::Panel::left("hierarchy")
                .default_size(220.0)
                .show(ui, |ui| {
                    panels::panel_header(ui, "Hierarchy", |_| {});
                    ui.label(
                        RichText::new(t(
                            "Update besfa_editor_plugin in the game to show the Hierarchy.",
                        ))
                        .color(MUTED),
                    );
                });
        }
        self.right_panel(ui);
        egui::CentralPanel::default().show(ui, |ui| self.viewport_ui(ui, &ctx));
        self.dialogs(&ctx);
        self.agent_window(&ctx);
        self.agent_chat(&ctx);
        self.animation_window(&ctx);
        self.send_brushes(&ctx);
        self.send_snap(&ctx);

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

    /// Takes in what the game, the net Play and the plugin update said, and
    /// launches the next game once the last one is gone.
    fn poll_processes(&mut self, ctx: &egui::Context) {
        self.poll_game(ctx);
        self.poll_net();
        self.relaunch_when_exited(ctx);
        self.poll_update(ctx);
        self.agents.poll();
        self.poll_agent(ctx);
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
                    // A field of its panels had the keyboard: nobody takes
                    // the keys now, and the editor's shortcuts are back.
                    self.panel_focus = (false, None);
                    self.panel_layout = (0.0, 0.0);
                    // A net Play ends with the game in the viewport.
                    self.stop_net();
                }
            }
        }
    }

    /// The net Play's lines go to the log, each marked with its process.
    fn poll_net(&mut self) {
        let mut lines = Vec::new();
        self.net.retain(|(label, process)| {
            let mut running = true;
            for output in process.poll() {
                lines.push(match output {
                    Output::Line(line) => format!("[{label}] {line}"),
                    Output::Exit(code) => {
                        running = false;
                        format!("[{label}] Process exited with code {code}.")
                    }
                });
            }
            running
        });
        for line in lines {
            self.log(line);
        }
    }

    /// Launches the game `relaunch` asks for once the last game and its net
    /// Play have exited.
    fn relaunch_when_exited(&mut self, ctx: &egui::Context) {
        if self.game.is_none()
            && self.net.is_empty()
            && let Some(play) = self.relaunch.take()
        {
            self.launch(play, ctx);
        }
    }

    fn stop_net(&self) {
        for (_, process) in &self.net {
            process.stop();
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
        // The game builds from the code as it is now.
        self.rebuild_after_save = false;
        self.code_changed = false;
        self.scene.reset();
        self.history = crate::scene::EditHistory::default();
        // The previous game has exited, so nothing renders into them any more.
        self.retired.clear();
        // Requests are numbered per game process.
        self.viewport_request = None;
        self.seq = 0;
        self.protocol = None;
        self.brush = None;
        self.scatter_brush = None;
        self.sent_snap = None;
        self.ghost = None;
        self.ensure_viewport();
        // The old game cannot answer them; the new one reads the files fresh.
        self.assets
            .reimports
            .retain(|_, reimport| !matches!(reimport, Reimport::Running));
        self.assets.forget_asked();
        let shared = self.viewport.as_ref().zip(self.render.as_ref());
        let shared = shared.map(|(viewport, render)| (viewport.name.clone(), adapter_name(render)));
        let path = shared.as_ref().and_then(|_| path_with_dxc());
        // A net Play's server listens on a port free now, apart from any
        // server started outside the editor.
        let mut port = None;
        if play && self.net_clients > 0 {
            match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).and_then(|s| s.local_addr()) {
                Ok(address) => port = Some(address.port().to_string()),
                Err(error) => self.log(format!(
                    "Could not find a free port for the server: {error}"
                )),
            }
        }
        // Read by besfa_editor_plugin: start paused, play on `play`.
        let mut env = vec![("BESFA_EDIT_MODE", "1")];
        let scale = ctx.pixels_per_point().to_string();
        if let Some((name, adapter)) = &shared {
            // The plugin takes the size from the texture itself. It opens the
            // texture on D3D12, on the editor's GPU.
            env.extend([
                ("BESFA_VIEWPORT", name.as_str()),
                ("WGPU_BACKEND", "dx12"),
                ("WGPU_ADAPTER_NAME", adapter),
                // The panels left to the game, which draws them in the
                // viewport at the editor's size and in its language.
                (PANELS_ENV, "hierarchy,inspector"),
                (SCALE_ENV, scale.as_str()),
            ]);
            if Language::current() == Language::Korean {
                env.push((LANGUAGE_ENV, "ko"));
            }
            match &path {
                // DXC, or FXC should it fail to load.
                Some(path) => env.extend([("WGPU_DX12_COMPILER", "auto"), ("PATH", path.as_str())]),
                None => self.log(
                    "No Windows SDK DXC found: the game compiles its shaders with FXC, \
                     which can draw much slower.",
                ),
            }
        }
        // Dynamic linking makes rebuilds after a code change much faster, and
        // the editor feature brings in the plugin; a plain `cargo build`
        // stays standalone and without the editor.
        let manifest = fs::read_to_string(self.project.file("Cargo.toml")).unwrap_or_default();
        let features = besfa_protocol::run_features(&manifest);
        let args = ["run", "--features", features.as_str()];
        // The same build plays every part: the first `cargo run` builds it
        // while the others wait for the build directory.
        if let Some(port) = &port {
            let server = [("BESFA_NET", "serve"), ("BESFA_NET_PORT", port)];
            self.start_net("server".into(), &args, &server, ctx);
            env.extend([
                ("BESFA_NET", "join"),
                ("BESFA_NET_PORT", port),
                ("BESFA_NET_CLIENT", "1"),
            ]);
        }
        match CliProcess::start(Path::new(&self.project.path), "cargo", &args, &env, ctx) {
            Ok(game) => self.game = Some(game),
            Err(error) => return self.log(format!("Could not start cargo: {error}")),
        }
        // The other clients play in windows of their own.
        if let Some(port) = &port {
            for client in 2..=self.net_clients {
                let number = client.to_string();
                let join = [
                    ("BESFA_NET", "join"),
                    ("BESFA_NET_PORT", port),
                    ("BESFA_NET_CLIENT", &number),
                ];
                self.start_net(format!("client {client}"), &args, &join, ctx);
            }
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

    /// Starts one process of a net Play, which `env` tells its part.
    fn start_net(
        &mut self,
        label: String,
        args: &[&str],
        env: &[(&str, &str)],
        ctx: &egui::Context,
    ) {
        match CliProcess::start(Path::new(&self.project.path), "cargo", args, env, ctx) {
            Ok(process) => self.net.push((label, process)),
            Err(error) => self.log(format!("[{label}] Could not start cargo: {error}")),
        }
    }

    /// Unsaved changes are saved first, and playing waits for that: Stop
    /// restarts from the file.
    fn play(&mut self, ctx: &egui::Context) {
        self.logs.clear();
        self.input.focus_next = true;
        if self.game.is_some() && self.scene.dirty {
            self.starting = true;
            return self.save_then(AfterSave::Play, ctx);
        }
        if self.net_clients > 0 {
            // A game picks being a server or a client as it starts, so the
            // edit session makes way for them, and they read the scene file.
            return self.restart(true, ctx);
        }
        if self.game.is_none() {
            // The edit session is gone, e.g. it failed to build or crashed.
            return self.launch(true, ctx);
        }
        // Playing drops the game's preview.
        self.previewing = false;
        self.send(Command::Play);
        self.playing = true;
        self.starting = false;
    }

    /// Ends play. The game goes back to edit mode in its own process, which
    /// skips the build and keeps its compiled shaders, unless `restart`
    /// asks for a new process or one is needed: the code changed, a net
    /// Play runs other processes, or the game's plugin cannot.
    fn stop(&mut self, restart: bool, ctx: &egui::Context) {
        let in_place = !restart
            && !self.code_changed
            && self.net_clients == 0
            && self.net.is_empty()
            && self.update.is_none()
            && self.protocol >= Some(RESET_PROTOCOL);
        if !in_place || self.send(Command::Reset).is_none() {
            return self.restart(false, ctx);
        }
        self.scene.reset();
        // The game numbers the entities anew.
        self.scene.selected = None;
        self.history = crate::scene::EditHistory::default();
        // The game's scene view starts over, without the brushes and snap.
        self.brush = None;
        self.scatter_brush = None;
        self.sent_snap = None;
        self.ghost = None;
        self.assets.forget_asked();
        if self.tool != Tool::Translate {
            self.send(Command::Tool { tool: self.tool });
        }
        self.playing = false;
    }

    /// Stops the game and any net Play, and launches the next game, playing
    /// with `play`, once they have exited: a running game locks its
    /// executable against the next build.
    fn restart(&mut self, play: bool, ctx: &egui::Context) {
        self.starting = true;
        self.relaunch = Some(play);
        if let Some(game) = &self.game {
            game.stop();
        }
        self.stop_net();
        self.relaunch_when_exited(ctx);
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
            self.scene.dirty = true;
        }
    }

    /// Adds `edit` to the history. An undone change it forgets may have left
    /// an entity hidden for redo; nothing can restore that entity now, so the
    /// game despawns it.
    fn record(&mut self, edit: Edit) {
        for forgotten in self.history.add(edit) {
            for command in forgotten.undo {
                match command {
                    Command::Delete { id } => {
                        self.send(Command::Despawn { id });
                    }
                    // What the game keeps to undo the stroke can go.
                    Command::TerrainRestore { stroke, .. } => {
                        self.send(Command::TerrainForget { stroke });
                    }
                    _ => {}
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
            self.scene.dirty = true;
        }
    }

    /// Records what the game did on the editor's behalf, notices saves and
    /// answers to requests, and checks the plugin's protocol.
    fn on_report(&mut self, report: Report, ctx: &egui::Context) {
        match report {
            Report::Hello { version } => {
                self.protocol = Some(version);
                // The language may have changed since the game was started.
                if self.draws_hierarchy() {
                    let korean = Language::current() == Language::Korean;
                    self.send(Command::Language { korean });
                }
                self.panel_focus = (false, None);
                self.panel_layout = (0.0, 0.0);
                if let Some(warning) = protocol_warning(version) {
                    self.log(warning);
                }
            }
            // Reports come before any hello only from an early plugin.
            Report::Entities { .. } if self.protocol.is_none() => {
                self.protocol = Some(0);
                self.log(protocol_warning(0).unwrap_or_default());
            }
            Report::Panel(action) => self.on_panel(action),
            Report::PanelFocus { keyboard, ime } => self.panel_focus = (keyboard, ime),
            Report::Copied { text } => ctx.copy_text(text),
            Report::PanelLayout { left, right } => self.panel_layout = (left, right),
            // What one drop moved, undone in one step.
            Report::Moved { moves } if !moves.is_empty() => {
                let set = |id, value| Self::set_command(id, TRANSFORM, value);
                self.record(Edit {
                    undo: moves.iter().map(|m| set(m.id, m.before.clone())).collect(),
                    redo: moves.into_iter().map(|m| set(m.id, m.after)).collect(),
                    seq: None,
                });
                self.scene.dirty = true;
            }
            // One scatter stroke: what it placed and what it erased.
            Report::Scattered { added, removed } => {
                let delete = |&id| Command::Delete { id };
                let restore = |&id| Command::Restore { id };
                self.record(Edit {
                    undo: added
                        .iter()
                        .map(delete)
                        .chain(removed.iter().map(restore))
                        .collect(),
                    redo: added
                        .iter()
                        .map(restore)
                        .chain(removed.iter().map(delete))
                        .collect(),
                    seq: None,
                });
                self.scene.dirty = true;
            }
            // A stroke the scene view's brush painted.
            Report::TerrainPainted { stroke, .. } => {
                let restore = |before| Command::TerrainRestore { stroke, before };
                self.record(Edit {
                    undo: vec![restore(true)],
                    redo: vec![restore(false)],
                    seq: None,
                });
                self.scene.dirty = true;
            }
            Report::Spawned { id } => self.record(Edit {
                undo: vec![Command::Delete { id }],
                redo: vec![Command::Restore { id }],
                seq: None,
            }),
            // One still on its way when Stop started the scene over names an
            // entity that is gone: nothing to undo, nothing unsaved.
            Report::Edited { id, .. } if !self.scene.entities.iter().any(|e| e.id == id) => {}
            Report::Edited {
                id,
                component,
                before,
                after,
            } => {
                let set = |value| Self::set_command(id, &component, value);
                self.record(Edit {
                    undo: vec![set(before)],
                    redo: vec![set(after)],
                    seq: None,
                });
                // What changes while the game plays is not the scene's.
                if !self.playing {
                    self.scene.dirty = true;
                }
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
            Report::Reimported { path, error } => {
                let reimport = match error {
                    None => {
                        self.log(format!("Reimported {path}"));
                        Reimport::Done(Instant::now())
                    }
                    Some(error) => {
                        self.log(format!("Could not reimport {path}: {error}"));
                        Reimport::Failed(error)
                    }
                };
                self.assets.reimports.insert(path, reimport);
            }
            Report::Thumbnail { path, error } => match error {
                Some(error) => self.log(format!("Could not picture {path}: {error}")),
                None => self.assets.refresh(),
            },
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
            // A failed or late save left changes out of the file a restart
            // would load, so the game keeps them instead.
            AfterSave::Rebuild | AfterSave::Play | AfterSave::Update if self.scene.dirty => {
                self.starting = false;
                self.rebuild_after_save |= matches!(then, AfterSave::Rebuild);
                self.log(match then {
                    AfterSave::Rebuild => {
                        "The scene's changes are not saved; the game rebuilds once they are."
                    }
                    AfterSave::Play => "Not playing: the scene's changes are not saved.",
                    _ => "Not updating the plugin: the scene's changes are not saved.",
                });
            }
            AfterSave::Rebuild => self.restart(false, ctx),
            AfterSave::Play => self.play(ctx),
            AfterSave::Update => self.update_plugin(ctx),
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
        self.rebuild_after_save = false;
        self.log("Source changed; rebuilding.");
        if self.game.is_some() && self.scene.dirty {
            self.starting = true;
            self.save_then(AfterSave::Rebuild, ctx);
        } else {
            self.restart(false, ctx);
        }
    }

    /// Moves the project's plugin to the latest commit and restarts the edit
    /// session, which rebuilds the game against it, once unsaved changes
    /// are saved.
    fn update_plugin(&mut self, ctx: &egui::Context) {
        self.starting = true;
        if self.game.is_some() && !self.playing && self.scene.dirty {
            return self.save_then(AfterSave::Update, ctx);
        }
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
        self.restart(false, ctx);
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

    /// What is left of the right panel now that the game draws the Inspector
    /// in the viewport: the previewed model's panel, a selected terrain's
    /// tools and the scatter brush's settings, each only while it is of use.
    fn right_panel(&mut self, ui: &mut Ui) {
        let previewing = self.previewing && self.preview.is_some();
        self.can_sculpt = self.protocol >= Some(TERRAIN_PROTOCOL) && !self.playing;
        let terrain = self
            .scene
            .components
            .iter()
            .find(|component| component.path == TERRAIN && self.scene.selected.is_some())
            .cloned();
        // A plugin from before the game drew the Inspector has none.
        let outdated = self.protocol.is_some() && !self.draws_inspector();
        if !(previewing || terrain.is_some() || self.scatter.on || outdated) {
            return;
        }
        egui::Panel::right("inspector")
            .default_size(320.0)
            .show(ui, |ui| {
                if let Some(preview) = self.preview.as_mut().filter(|_| previewing) {
                    let reimport = self.assets.reimports.get(&preview.path);
                    let commands = preview.ui(ui, &self.scene, reimport);
                    preview.save();
                    for command in commands {
                        self.send(command);
                    }
                    return;
                }
                if outdated {
                    panels::panel_header(ui, "Inspector", |_| {});
                    ui.label(
                        RichText::new(t(
                            "Update besfa_editor_plugin in the game to show the Inspector.",
                        ))
                        .color(MUTED),
                    );
                }
                if terrain.is_some() && self.scatter.on {
                    ui.horizontal(|ui| {
                        for (tab, label) in [
                            (RightTab::Terrain, t("Terrain")),
                            (RightTab::Scatter, t("Scatter")),
                        ] {
                            ui.selectable_value(&mut self.right_tab, tab, label);
                        }
                    });
                }
                let scatter =
                    self.scatter.on && (terrain.is_none() || self.right_tab == RightTab::Scatter);
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| match &terrain {
                        _ if scatter => self.scatter.ui(ui),
                        Some(terrain) => {
                            panels::panel_header(ui, "Terrain", |_| {});
                            if let Some(value) = self.terrain.ui(ui, terrain, self.can_sculpt) {
                                self.on_inspector(Action::Set(TERRAIN.into(), value));
                            }
                        }
                        None => {}
                    });
            });
    }

    /// Whether the game's plugin draws the Hierarchy itself.
    fn draws_hierarchy(&self) -> bool {
        self.protocol
            .is_some_and(|protocol| protocol >= HIERARCHY_PROTOCOL)
    }

    /// Whether the game's plugin draws the Inspector itself.
    fn draws_inspector(&self) -> bool {
        self.protocol
            .is_some_and(|protocol| protocol >= INSPECTOR_PROTOCOL)
    }

    /// A click in a panel the game draws.
    fn on_panel(&mut self, action: PanelAction) {
        match action {
            // The game reports what it spawns, which records the undo.
            // Lights and cameras keep their own height and aim. The rest
            // goes where the game says the scene view's middle is.
            PanelAction::Spawn { kind, pointer } => {
                let pointer = matches!(
                    kind,
                    SpawnKind::Empty | SpawnKind::Cube | SpawnKind::Sphere | SpawnKind::Plane
                )
                .then(|| pointer.or_else(|| self.viewport_middle()))
                .flatten();
                self.edit(
                    Command::Spawn {
                        kind,
                        place: Placement {
                            pointer,
                            ..Placement::default()
                        },
                    },
                    vec![],
                )
            }
            PanelAction::Duplicate => self.duplicate(),
            PanelAction::Delete => self.delete(),
            // Edit mode's, as the button is.
            PanelAction::SavePrefab => {
                if let Some(entity) = self.scene.selected_entity().filter(|_| !self.playing) {
                    self.dialog = Some(Dialog::PrefabName(
                        entity.name.clone().unwrap_or("Prefab".into()),
                    ));
                }
            }
            // The Inspector's, which the editor's own asked the same way.
            PanelAction::Insert { component } => self.on_inspector(Action::Insert(component)),
            PanelAction::Remove { component } => self.on_inspector(Action::Remove(component)),
            PanelAction::ApplyPrefab { id } if !self.playing => {
                self.on_inspector(Action::ApplyPrefab(id))
            }
            PanelAction::ApplyPrefab { .. } => {}
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

    /// Puts the asset at `path` in the scene, on what the viewport shows at
    /// `pointer`, in its pixels, or else at the origin; the game reports it
    /// as spawned.
    fn instantiate(&mut self, path: String, pointer: Option<[f32; 2]>) {
        self.edit(
            Command::Instantiate {
                path,
                place: Placement {
                    pointer,
                    ..Placement::default()
                },
            },
            vec![],
        );
    }

    /// The middle of the scene view, in the viewport's pixels, where new
    /// things go: between the panels the game draws over its sides.
    fn viewport_middle(&self) -> Option<[f32; 2]> {
        let viewport = self.viewport.as_ref()?;
        let (left, right) = self.panel_layout;
        let (from, to) = (left, viewport.width as f32 - right);
        (to > from).then(|| [(from + to) / 2.0, viewport.height as f32 / 2.0])
    }

    /// Has the game show `ghost`, the asset dragged over the viewport at its
    /// pointer, where it would land, or nothing, when that changed.
    fn show_ghost(&mut self, ghost: Option<(String, [f32; 2])>) {
        if ghost == self.ghost || self.protocol < Some(PLACE_PROTOCOL) || self.playing {
            return;
        }
        let (path, pointer) = match ghost.clone() {
            Some((path, pointer)) => (Some(path), pointer),
            None => (None, [0.0; 2]),
        };
        self.send(Command::Ghost { path, pointer });
        self.ghost = ghost;
    }

    /// Asks the game in edit mode to picture the models without an up to
    /// date thumbnail.
    fn ask_thumbnails(&mut self) {
        if self.game.is_none() || self.playing || self.protocol.is_none_or(|version| version < 3) {
            return;
        }
        for path in self.assets.wanted_thumbnails() {
            let file = panels::thumbnail_file(&path);
            self.assets.asked(&path);
            self.send(Command::Thumbnail { path, file });
        }
    }

    /// Loads the model at `path` again in the game after its file changed;
    /// a game that is not running reads it fresh when it starts.
    fn reimport(&mut self, path: String) {
        if self.game.is_none() || self.protocol.is_none_or(|version| version < 3) {
            return;
        }
        self.send(Command::Reimport { path: path.clone() });
        self.assets.reimports.insert(path, Reimport::Running);
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
            if ui.selectable_label(!self.previewing, t("Scene")).clicked() && self.previewing {
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
                .on_hover_text(t("Close the preview"))
                .clicked();
        });
        if close {
            if self.previewing {
                self.show_preview(false);
            }
            self.preview = None;
        } else if let Some(show) = show {
            self.show_preview(show);
        }
    }

    /// Tells the game which terrain brush to paint the selection with,
    /// when that changed: a brush is picked, the selection is a terrain,
    /// and the scene shows in edit mode. Shift turns Raise into Lower.
    fn send_brushes(&mut self, ctx: &egui::Context) {
        // Scatter is turned on with the terrain tool put away, so both at
        // once means a terrain tool was picked since.
        if self.scatter.on && self.terrain.tool.is_some() {
            self.scatter.on = false;
        }
        let terrain = self.scene.components.iter().any(|c| c.path == TERRAIN);
        let shift = ctx.input(|i| i.modifiers.shift);
        let brush = self
            .terrain
            .brush(shift)
            .filter(|_| terrain && self.can_sculpt && !self.previewing);
        if brush != self.brush && self.game.is_some() {
            self.brush = brush;
            self.send(Command::TerrainBrush { brush });
        }
        let scatter = self.scatter.brush(shift).filter(|_| self.can_scatter());
        if scatter != self.scatter_brush && self.game.is_some() {
            self.scatter_brush = scatter.clone();
            self.send(Command::ScatterBrush { brush: scatter });
        }
    }

    /// Tells the game the grid and whether handles snap, from the toolbar or
    /// with Ctrl held, when that changed.
    fn send_snap(&mut self, ctx: &egui::Context) {
        let held = ctx.input(|i| i.modifiers.command);
        let snap = (self.snapping.grid, self.snapping.on || held);
        if self.sent_snap != Some(snap)
            && self.game.is_some()
            && self.protocol >= Some(PLACE_PROTOCOL)
        {
            self.sent_snap = Some(snap);
            self.send(Command::Snap {
                grid: snap.0,
                on: snap.1,
            });
        }
    }

    /// The game's plugin scatters with the scene view's brush, and the
    /// scene shows in edit mode.
    fn can_scatter(&self) -> bool {
        self.protocol >= Some(SCATTER_PROTOCOL) && !self.playing && !self.previewing
    }

    /// Puts the selection down on the surface below it; the game reports
    /// what moved, which records the undo.
    fn drop_to_surface(&mut self) {
        if let Some(id) = self.scene.selected
            && self.protocol >= Some(DROP_PROTOCOL)
        {
            self.send(Command::DropToSurface { ids: vec![id] });
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
        // Undo would change the scene out of sight of the model preview, and
        // is the field's own while a field of the game's panels is typed in.
        if let Some(key) = EditKey::pressed(ctx, !self.previewing && !self.panel_focus.0) {
            return self.on_edit_key(key, ctx);
        }
        // Unity's key for its Asset Store.
        if pressed(Modifiers::COMMAND, Key::Num9) {
            return project::open_url(project::BEVY_ASSETS);
        }
        if ctx.text_edit_focused() || self.panel_focus.0 {
            return;
        }
        // The scene is out of sight: only framing the model is left.
        if self.previewing {
            if pressed(Modifiers::NONE, Key::F) {
                self.send(Command::Focus);
            }
            return;
        }
        if pressed(Modifiers::NONE, Key::Escape) {
            self.terrain.tool = None;
            self.scatter.on = false;
        } else if pressed(Modifiers::NONE, Key::End) {
            self.drop_to_surface();
        } else if pressed(Modifiers::COMMAND, Key::D) {
            self.duplicate();
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

    /// Does what `key` asks, pressed in the window `ctx` is drawing.
    fn on_edit_key(&mut self, key: EditKey, ctx: &egui::Context) {
        match key {
            EditKey::Save => {
                // The focused field commits as it loses focus this frame.
                ctx.memory_mut(|memory| memory.stop_text_input());
                self.save_after_ui = true;
            }
            EditKey::Undo => self.replay(true),
            EditKey::Redo => self.replay(false),
        }
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(t("File"), |ui| {
                    // Only the edit session's scene is worth keeping.
                    let can_save = self.game.is_some() && !self.playing;
                    if ui
                        .add_enabled(
                            can_save,
                            Button::new(t("Save scene")).shortcut_text("Ctrl+S"),
                        )
                        .clicked()
                    {
                        self.save_after_ui = true;
                    }
                    if ui.button(t("Back to Project Hub")).clicked() {
                        self.request_leave();
                    }
                });
                ui.menu_button(t("Edit"), |ui| {
                    let undo = Button::new(t("Undo")).shortcut_text("Ctrl+Z");
                    if ui
                        .add_enabled(self.history.can_undo() && !self.playing, undo)
                        .clicked()
                    {
                        self.replay(true);
                    }
                    let redo = Button::new(t("Redo")).shortcut_text("Ctrl+Y");
                    if ui
                        .add_enabled(self.history.can_redo() && !self.playing, redo)
                        .clicked()
                    {
                        self.replay(false);
                    }
                    let drop = Button::new(t("Drop to surface")).shortcut_text("End");
                    let can_drop = self.scene.selected.is_some()
                        && !self.playing
                        && self.protocol >= Some(DROP_PROTOCOL);
                    if ui.add_enabled(can_drop, drop).clicked() {
                        self.drop_to_surface();
                    }
                });
                ui.menu_button(t("Window"), |ui| {
                    // Unchecking is the window's own close button.
                    let animation =
                        ui.checkbox(&mut self.animation_open, t("Animation State Machine"));
                    let agent = ui.checkbox(&mut self.agent_chat_open, t("Agent Window"));
                    if animation.changed() || agent.changed() {
                        ui.close();
                    }
                });
                let agent_settings = |ui: &mut Ui| {
                    if ui.button(t("Agent Settings...")).clicked() {
                        self.agent_open = true;
                        self.agents.check(ui.ctx());
                    }
                };
                let language = Language::current();
                if let Some(error) = i18n::settings_menu(ui, agent_settings) {
                    self.log(format!("Could not save the settings: {error}"));
                }
                // The panels the game draws follow, on a plugin that draws any.
                if Language::current() != language && self.draws_hierarchy() {
                    let korean = Language::current() == Language::Korean;
                    self.send(Command::Language { korean });
                }
                ui.menu_button(t("Help"), |ui| {
                    let assets = Button::new(t("Bevy Assets")).shortcut_text("Ctrl+9");
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
                let label = if self.playing {
                    t("⏹ Stop")
                } else {
                    t("▶ Play")
                };
                let mut button = ui.add_enabled(!busy, Button::new(label));
                if self.playing {
                    button = button.on_hover_text(t("Ctrl+click restarts the game"));
                }
                if button.clicked() {
                    if self.playing {
                        let restart = ui.input(|input| input.modifiers.command);
                        self.stop(restart, ctx)
                    } else {
                        self.play(ctx)
                    }
                }
                self.play_as(ui, !busy && !self.playing);
                ui.add_space(12.0);
                for (tool, icon, tooltip, _) in TOOLS {
                    let button = Button::selectable(self.tool == tool, icon);
                    if ui
                        .add_enabled(!self.playing && !self.previewing, button)
                        .on_hover_text(t(tooltip))
                        .clicked()
                    {
                        self.set_tool(tool);
                    }
                }
                self.snap_tools(ui);
                let scatter = Button::selectable(self.scatter.on, "🌲");
                if ui
                    .add_enabled(self.can_scatter(), scatter)
                    .on_hover_text(t("Scatter"))
                    .clicked()
                {
                    self.scatter.on = !self.scatter.on;
                    if self.scatter.on {
                        self.right_tab = RightTab::Scatter;
                        self.terrain.tool = None;
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

    /// Whether Play runs the game alone, or as a dedicated server and the
    /// clients joined to it.
    fn play_as(&mut self, ui: &mut Ui, enabled: bool) {
        let name = |clients: usize| match clients {
            0 => t("Standalone").to_owned(),
            1 => t("Server + 1 client").to_owned(),
            n => t("Server + {} clients").replace("{}", &n.to_string()),
        };
        ui.add_enabled_ui(enabled, |ui| {
            egui::ComboBox::from_id_salt("play_as")
                .selected_text(name(self.net_clients))
                .show_ui(ui, |ui| {
                    for clients in 0..=MAX_CLIENTS {
                        ui.selectable_value(&mut self.net_clients, clients, name(clients));
                    }
                })
                .response
                .on_hover_text(
                    "Play a dedicated server with clients joined to it. The first client \
                     plays in the viewport; the others open windows.",
                );
        });
    }

    /// The snap button and the grid's spacing, which the project keeps.
    fn snap_tools(&mut self, ui: &mut Ui) {
        let before = self.snapping;
        let enabled = !self.playing && !self.previewing;
        let snap = Button::selectable(self.snapping.on, "#");
        if ui
            .add_enabled(enabled, snap)
            .on_hover_text(t("Snap (hold Ctrl)"))
            .clicked()
        {
            self.snapping.on = !self.snapping.on;
        }
        ui.add_enabled(enabled, egui::Label::new(t("Grid")));
        let grid = egui::DragValue::new(&mut self.snapping.grid)
            .range(0.01..=100.0)
            .speed(0.05)
            .suffix(" m");
        ui.add_enabled(enabled, grid);
        if self.snapping != before
            && let Err(error) = self.snapping.write(&self.project.file(SNAP_FILE))
        {
            self.log(format!("Could not keep the snapping: {error}"));
        }
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
        let differs = matches!(
            (&self.plugin_revision, &self.latest_plugin_revision),
            (Some(project), Some(latest)) if project != latest
        );
        let (icon, color, state) = if differs || warning.is_some() {
            (
                "⚠",
                WARNING,
                "The plugin differs from the Bevy prebuild. Click to update it to the latest commit.",
            )
        } else {
            (
                "✔",
                GOOD,
                "The plugin matches the Bevy prebuild. Click to update it to the latest commit.",
            )
        };
        let tooltip = match warning {
            Some(warning) => {
                t("{} Click to update it to the latest commit.").replace("{}", &warning)
            }
            None => t(state).to_owned(),
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
            ui.centered_and_justified(|ui| ui.label(RichText::new(t("Viewport")).color(MUTED)));
            return;
        };
        let scale = (panel.width() / size.x).min(panel.height() / size.y);
        let rect = egui::Rect::from_center_size(panel.center(), size * scale);
        egui::Image::new((texture, size)).paint_at(ui, rect);
        let mut commands = Vec::new();
        let panel_focus = self.panel_focus;
        let response = self
            .input
            .ui(ui, rect, size, self.playing, panel_focus, &mut |command| {
                commands.push(command)
            });
        for command in commands {
            self.send(command);
        }
        if !self.playing {
            let hint = if self.previewing {
                t("Drag orbit  ·  Middle-drag pan  ·  Scroll zoom  ·  F fit")
            } else if self.scatter_brush.is_some() {
                t(
                    "Paint to place; hold Shift to erase.  ·  Right-drag orbit  ·  Middle-drag pan  ·  Scroll zoom  ·  Esc stop",
                )
            } else if self.brush.is_some() {
                t(
                    "Paint to sculpt; hold Shift to invert.  ·  Right-drag orbit  ·  Middle-drag pan  ·  Scroll zoom  ·  Esc stop",
                )
            } else {
                t(
                    "Click select  ·  Right-drag orbit  ·  Right-hold + WASD/QE fly  ·  Middle-drag pan  ·  Scroll zoom  ·  F focus",
                )
            };
            // In the middle of the scene view, between the panels the game
            // draws over the viewport's sides, when it fits there.
            let scale = rect.width() / size.x;
            let (left, right) = (self.panel_layout.0 * scale, self.panel_layout.1 * scale);
            let color = Color32::from_white_alpha(0x99);
            let font = egui::FontId::proportional(11.0);
            let hint = ui.painter().layout_no_wrap(hint.to_owned(), font, color);
            let scene = rect.width() - left - right;
            if hint.size().x + 16.0 <= scene {
                let middle = rect.left() + left + scene / 2.0;
                let at = egui::pos2(
                    middle - hint.size().x / 2.0,
                    rect.bottom() - 8.0 - hint.size().y,
                );
                ui.painter().galley(at, hint, color);
            }
        }
        // Assets dragged from the Assets tab show where they would land, and
        // land there, when the scene shows.
        if self.previewing {
            return;
        }
        let pointer = ui
            .input(|i| i.pointer.latest_pos())
            .map(|pos| (pos - rect.min) * size / rect.size())
            .map(|at| [at.x, at.y]);
        let hovered = response.dnd_hover_payload::<String>();
        if hovered.is_some() {
            ui.painter()
                .rect_stroke(rect, 0.0, (2.0, ACCENT), egui::StrokeKind::Inside);
        }
        self.show_ghost(hovered.map(|path| (*path).clone()).zip(pointer));
        if let Some(path) = response.dnd_release_payload::<String>() {
            self.instantiate((*path).clone(), pointer);
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
        self.scale = ctx.pixels_per_point();
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
                let scale = Some(self.scale);
                self.viewport_request = self.send(Command::Viewport { name, scale });
            }
            Err(error) => self.log(format!("Could not resize the viewport: {error}")),
        }
    }

    /// The selected model's animation state machine in a window of its
    /// own; it follows the selection, and edits go through the inspector's
    /// route for undo and saving.
    fn animation_window(&mut self, ctx: &egui::Context) {
        if !self.animation_open {
            return;
        }
        let builder = egui::ViewportBuilder::default()
            .with_title("Besfa Animation")
            .with_inner_size([480.0, 640.0]);
        let mut close = false;
        let id = egui::ViewportId::from_hash_of("animation_window");
        let action = ctx.show_viewport_immediate(id, builder, |ui, _| {
            close = ui.input(|input| input.viewport().close_requested());
            // Keys pressed here reach only this window's pass. Undo works
            // during the model preview too, like Edit > Undo: the window
            // shows what it undoes.
            if !self.playing
                && let Some(key) = EditKey::pressed(ui.ctx(), true)
            {
                self.on_edit_key(key, ui.ctx());
            }
            animation_panel(ui, &self.scene)
        });
        if let Some(action) = action {
            self.on_inspector(action);
        }
        if close {
            self.animation_open = false;
        }
    }

    /// The Agent chat in a window of its own.
    fn agent_chat(&mut self, ctx: &egui::Context) {
        if !self.agent_chat_open {
            return;
        }
        let builder = egui::ViewportBuilder::default()
            .with_title("Besfa Agent")
            .with_inner_size([900.0, 640.0]);
        let mut close = false;
        let id = egui::ViewportId::from_hash_of("agent_chat");
        ctx.show_viewport_immediate(id, builder, |ui, _| {
            egui::Panel::top("agent_view").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.agent_view, AgentView::Chat, t("Chat"));
                    ui.selectable_value(
                        &mut self.agent_view,
                        AgentView::Organization,
                        t("Organization"),
                    )
                    .on_hover_text(t("Agents with roles that work on a task together."));
                    ui.selectable_value(&mut self.agent_view, AgentView::Settings, t("Settings"))
                        .on_hover_text(t(
                            "How each provider is reached, and what the agents may use in this project.",
                        ));
                });
            });
            match self.agent_view {
                AgentView::Chat => self.agent_tab(ui),
                AgentView::Organization => self.organization_tab(ui),
                AgentView::Settings => self.agent_settings_tab(ui),
            }
            close = ui.input(|input| input.viewport().close_requested());
        });
        if close {
            self.agent_chat_open = false;
        }
    }

    fn agent_tab(&mut self, ui: &mut Ui) {
        let ctx = &ui.ctx().clone();
        self.agents.check_once(ctx);
        let before = self.agent.clone();
        let route = self.agent_route();
        // Each organization agent's way is checked when the task starts.
        let blocked = route
            .as_ref()
            .err()
            .copied()
            .filter(|_| self.agent_panel.organization().is_none())
            .or(self.agent_blocked());
        let prompt = self.agent_panel.ui(
            ui,
            &mut self.agent,
            &self.agents,
            blocked,
            &mut self.organization,
        );
        if self.agent_panel.take_continue() {
            let directory = self.project.file("");
            let scene = self.project.file(SCENE_FILE);
            let resumed = self
                .organization
                .resume(&self.agent, &self.agents, &directory, scene);
            if let Err(error) = resumed {
                self.agent_panel.say(&error);
            }
        }
        if self.agent != before
            && let Err(error) = self.agent.write(&AgentSettings::file())
        {
            self.log(format!("Could not save the agent settings: {error}"));
        }
        if let Some((prompt, files)) = &prompt
            && let Some(name) = self.agent_panel.organization().map(str::to_string)
        {
            let task = self.agent_panel.ask(prompt);
            let directory = self.project.file("");
            let scene = self.project.file(SCENE_FILE);
            let started = self.organization.start(
                &name,
                task,
                files,
                self.agent_panel.chat_id(),
                &self.agent,
                &self.agents,
                &directory,
                scene,
            );
            if let Err(error) = started {
                self.agent_panel.say(&error);
            }
            return;
        }
        // The pick may have changed this frame, so the route is asked again.
        if let (Some((prompt, files)), Ok(route)) = (prompt, self.agent_route()) {
            let provider = self.agent.provider;
            let directory = self.project.file("");
            let invocation = agent::invocation(
                provider,
                &route,
                self.agent.model(provider),
                &files,
                &self.agent.project(&directory),
            );
            let scene = self.project.file(SCENE_FILE);
            self.agent_panel
                .start(invocation, &prompt, &directory, scene, ctx);
        }
    }

    fn agent_blocked(&self) -> Option<&'static str> {
        self.scene
            .dirty
            .then_some(t("Save the scene first. The agent may change its file."))
    }

    fn organization_tab(&mut self, ui: &mut Ui) {
        if let Some(error) = self.organization.ui(ui) {
            self.log(format!("Could not save the organization: {error}"));
        }
    }

    fn agent_settings_tab(&mut self, ui: &mut Ui) {
        self.agents.check_once(ui.ctx());
        let directory = self.project.file("");
        let changed = egui::CentralPanel::default()
            .show(ui, |ui| {
                agent::settings_tab(ui, &mut self.agent, &mut self.agents, &directory)
            })
            .inner;
        if changed && let Err(error) = self.agent.write(&AgentSettings::file()) {
            self.log(format!("Could not save the agent settings: {error}"));
        }
    }

    /// Reloads the scene the agent changed, unless that would throw away
    /// edits made while it ran.
    fn poll_agent(&mut self, ctx: &egui::Context) {
        let asking = self.agent_panel.asking();
        let finished = self.agent_panel.poll().or_else(|| {
            self.organization
                .poll(ctx, self.agent_panel.chat_and_asks())
        });
        let busy = self.agent_panel.running() || self.organization.running();
        if self.agent_panel.asking() > asking {
            // The Agent window may be closed or on another tab.
            self.log("An agent waits for your permission in the Agent window's Chat tab.");
        }
        self.answer_asks();
        if !busy {
            self.agent_panel.drop_asks();
        }
        let chats = AgentPanel::chats_file(&self.project.file(""));
        if let Err(error) = self.agent_panel.keep(&chats, busy) {
            self.log(format!("Could not save the agent chats: {error}"));
        }
        let Some(scene_changed) = finished else {
            return;
        };
        self.log("The agent finished.");
        if !scene_changed {
            return;
        }
        if self.scene.dirty {
            self.log(format!(
                "The agent changed {SCENE_FILE} while the scene had unsaved changes. Saving them overwrites the agent's."
            ));
        } else if !self.playing && !self.starting {
            self.log("The agent changed the scene; reloading it.");
            self.restart(false, ctx);
        }
    }

    /// Sends the running agent the user's answers to what it asked, and
    /// saves what the user allowed for good.
    fn answer_asks(&mut self) {
        for (ask, allow, always) in self.agent_panel.take_answers() {
            let line = ask.answer(allow, always);
            if !self.agent_panel.send(&line) {
                self.organization.send(&line);
            }
            if allow && always {
                if let Some(rule) = ask.allowed() {
                    self.organization.allow(&rule);
                }
                self.agent.allow(&self.project.file(""), &ask);
                if let Err(error) = self.agent.write(&AgentSettings::file()) {
                    self.log(format!("Could not save the agent settings: {error}"));
                }
            }
        }
    }

    /// How a prompt to the picked provider would go out, or why it cannot.
    fn agent_route(&self) -> Result<Route, &'static str> {
        let provider = self.agent.provider;
        self.agents.route(provider, self.agent.access(provider))
    }

    fn agent_window(&mut self, ctx: &egui::Context) {
        if agent::window(ctx, &mut self.agent_open, &mut self.agent, &mut self.agents)
            && let Err(error) = self.agent.write(&AgentSettings::file())
        {
            self.log(format!("Could not save the agent settings: {error}"));
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
                    ui.heading(t("Save as prefab"));
                    ui.label(t("Prefab name"));
                    let field =
                        ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                    if ui.memory(|memory| memory.focused().is_none()) {
                        field.request_focus();
                    }
                    submit = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    if valid {
                        ui.label(
                            RichText::new(t("Saved to assets/prefabs"))
                                .small()
                                .color(MUTED),
                        );
                    } else {
                        ui.label(
                            RichText::new(t("Letters, digits, spaces, - and _ only"))
                                .small()
                                .color(panels::ERROR),
                        );
                    }
                    ui.horizontal(|ui| {
                        cancel = ui.button(t("Cancel")).clicked();
                        submit |= ui.button(t("Save")).clicked();
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
                    ui.heading(t("Save changes to the scene?"));
                    ui.label(t(
                        "The scene has changes that are not saved to scenes/main.scn.ron.",
                    ));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(t("Cancel")).clicked() {
                            choice = Some("cancel");
                        }
                        if ui.button(t("Discard")).clicked() {
                            choice = Some("discard");
                        }
                        if can_save && ui.button(t("Save")).clicked() {
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

/// The scene view's snapping, which each project keeps.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
struct Snapping {
    on: bool,
    /// Meters between the ground grid's lines, which moving snaps to.
    grid: f32,
}

impl Default for Snapping {
    fn default() -> Self {
        Self {
            on: false,
            grid: 1.0,
        }
    }
}

impl Snapping {
    /// What `file` keeps, or the defaults when it keeps nothing usable.
    fn read(file: &Path) -> Self {
        fs::read_to_string(file)
            .ok()
            .and_then(|text| ron::from_str::<Self>(&text).ok())
            .filter(|snapping| snapping.grid > 0.0 && snapping.grid.is_finite())
            .unwrap_or_default()
    }

    fn write(&self, file: &Path) -> Result<(), String> {
        if let Some(folder) = file.parent() {
            fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        let text = ron::to_string(self).map_err(|error| error.to_string())?;
        fs::write(file, text).map_err(|error| error.to_string())
    }
}

/// Why a game whose plugin speaks protocol `version` needs a look, if it
/// does.
fn protocol_warning(version: u32) -> Option<String> {
    if version == VERSION {
        return None;
    }
    if version == 0 {
        return Some(
            "The game's besfa_editor_plugin predates protocol versions, so failed edits \
             stay in the undo history. Update the plugin."
                .into(),
        );
    }
    if version < HIERARCHY_PROTOCOL {
        return Some("The plugin is too old to show the Hierarchy and the Inspector.".into());
    }
    if version < INSPECTOR_PROTOCOL {
        return Some("The plugin is too old to show the Inspector.".into());
    }
    let (age, fix) = if version < VERSION {
        ("older", "plugin")
    } else {
        ("newer", "editor")
    };
    Some(format!(
        "The game's besfa_editor_plugin speaks protocol {version}, {age} than this \
         editor's {VERSION}. Update the {fix}."
    ))
}

/// Save, undo and redo, which work from the editor's own windows too.
#[derive(Clone, Copy, Debug, PartialEq)]
enum EditKey {
    Save,
    Undo,
    Redo,
}

impl EditKey {
    /// The one pressed in the window `ctx` is drawing, taken from its keys.
    /// Undo and redo only if `undo`, and not while a text field, which has
    /// its own, is focused.
    fn pressed(ctx: &egui::Context, undo: bool) -> Option<Self> {
        let pressed = |modifiers, key| {
            ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
        };
        if pressed(Modifiers::COMMAND, Key::S) {
            Some(Self::Save)
        } else if !undo || ctx.text_edit_focused() {
            None
        } else if pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
            || pressed(Modifiers::COMMAND, Key::Y)
        {
            Some(Self::Redo)
        } else {
            pressed(Modifiers::COMMAND, Key::Z).then_some(Self::Undo)
        }
    }
}

/// The animation window's contents: the state machine of `scene`'s
/// selection, and the change to make to it, if any.
fn animation_panel(ui: &mut Ui, scene: &Scene) -> Option<Action> {
    egui::CentralPanel::default()
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| animation::ui(ui, &scene.inspected()))
                .inner
        })
        .inner
}

impl Drop for Editor {
    fn drop(&mut self) {
        for process in self.game.iter().chain(&self.update) {
            process.stop();
        }
        self.stop_net();
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
    changed: bool,
}

impl SourceWatch {
    fn new(directory: PathBuf) -> Self {
        Self {
            files: rust_files(&directory),
            directory,
            polled: Instant::now(),
            changed: false,
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
            self.changed = true;
            return false;
        }
        std::mem::take(&mut self.changed)
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
    use crate::agent::{Access, Cli, Provider};

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
        // Before the game drew a panel, there is none to show.
        assert!(
            protocol_warning(HIERARCHY_PROTOCOL - 1)
                .unwrap()
                .contains("too old to show the Hierarchy and the Inspector")
        );
        assert_eq!(
            protocol_warning(INSPECTOR_PROTOCOL - 1).unwrap(),
            "The plugin is too old to show the Inspector."
        );
    }

    #[test]
    fn a_net_play_ends_with_its_game_and_the_next_game_waits_for_all() {
        let dir = std::env::temp_dir().join(format!("besfa_net_play_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ctx = egui::Context::default();
        let waiting = Prebuild::with_phase(crate::process::PrebuildPhase::Running, "");
        let mut editor = Editor::new(Project::new(dir.to_string_lossy()), None, &waiting, &ctx);
        let start = |command: &str| CliProcess::start(&dir, "cmd", &["/c", command], &[], &ctx);
        let long = "ping -n 60 127.0.0.1 > nul";
        let server_exits = |editor: &Editor| {
            let exited = |line: &&String| line.starts_with("[server] Process exited");
            editor.logs.iter().filter(exited).count()
        };
        // Polls as each frame does until `done`; taskkill takes seconds.
        let poll_until = |editor: &mut Editor, done: &dyn Fn(&Editor) -> bool| {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !done(editor) {
                assert!(Instant::now() < deadline, "{:?}", editor.logs);
                editor.poll_processes(&ctx);
                std::thread::sleep(Duration::from_millis(10));
            }
        };

        // The game in the viewport ends on its own: the server goes too, and
        // nothing launches.
        editor.game = Some(start("exit 3").unwrap());
        editor.net = vec![("server".into(), start(long).unwrap())];
        poll_until(&mut editor, &|editor| editor.net.is_empty());
        assert_eq!(server_exits(&editor), 1);
        assert!(editor.game.is_none());

        // After Stop the edit session launches once the server has exited
        // too, which outlives the game here.
        editor.game = Some(start("exit 3").unwrap());
        editor.net = vec![("server".into(), start(long).unwrap())];
        editor.playing = true;
        editor.relaunch = Some(false);
        poll_until(&mut editor, &|editor| editor.relaunch.is_none());
        assert_eq!(server_exits(&editor), 2, "{:?}", editor.logs);
        assert!(editor.game.is_some() && !editor.playing);
        drop(editor);
        let _ = fs::remove_dir_all(dir);
    }

    /// Keys pressed in the animation window reach only its own pass, so it
    /// takes undo itself, during the model preview too.
    #[test]
    fn undoes_with_keys_pressed_in_the_animation_window() {
        let dir = std::env::temp_dir().join(format!("besfa_anim_keys_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ctx = egui::Context::default();
        let waiting = Prebuild::with_phase(crate::process::PrebuildPhase::Running, "");
        let mut editor = Editor::new(Project::new(dir.to_string_lossy()), None, &waiting, &ctx);
        editor.animation_open = true;
        editor.previewing = true;
        editor.edit(Command::Focus, vec![Command::Focus]);
        // As eframe does, the window runs a pass of its own, with Ctrl+Z.
        ctx.set_embed_viewports(false);
        egui::Context::set_immediate_viewport_renderer(|ctx, mut viewport| {
            let id = viewport.ids.this;
            let mut input = egui::RawInput {
                viewport_id: id,
                events: vec![egui::Event::Key {
                    key: Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::COMMAND,
                }],
                ..Default::default()
            };
            input.viewports.insert(id, Default::default());
            let mut output = ctx.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui));
            output.textures_delta.clear();
        });
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            editor.animation_window(ui.ctx())
        });
        output.textures_delta.clear();
        assert!(editor.history.can_redo() && !editor.history.can_undo());
        drop(editor);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_games_inspector_edits_are_recorded_and_its_field_takes_the_keys() {
        let dir = std::env::temp_dir().join(format!("besfa_inspect_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ctx = egui::Context::default();
        let waiting = Prebuild::with_phase(crate::process::PrebuildPhase::Running, "");
        let mut editor = Editor::new(Project::new(dir.to_string_lossy()), None, &waiting, &ctx);
        editor
            .scene
            .handle(r#"@besfa {"type":"entities","entities":[{"id":7,"name":"Ship"}]}"#);
        editor.scene.handle(r#"@besfa {"type":"selected","id":7}"#);
        let edited = || Report::Edited {
            id: 7,
            component: "my_game::Speed".into(),
            before: serde_json::json!(1.0),
            after: serde_json::json!(2.0),
        };

        // The game set the value itself: the editor only remembers how to
        // undo it, and that the scene has changed.
        editor.on_report(edited(), &ctx);
        assert!(editor.history.can_undo() && editor.scene.dirty);

        // What changes while the game plays is not the scene's to save.
        editor.scene.dirty = false;
        editor.playing = true;
        editor.on_report(edited(), &ctx);
        assert!(!editor.scene.dirty);
        editor.playing = false;

        // Adding a component is the editor's to do, as from its own panel.
        let insert = PanelAction::Insert {
            component: "my_game::Spin".into(),
        };
        editor.on_report(Report::Panel(insert), &ctx);
        assert!(editor.scene.dirty);

        // A field of the game's panels has the keyboard, until a new game.
        // Where the panels are is told too, for the hint and new things.
        editor.on_report(
            Report::PanelLayout {
                left: 220.0,
                right: 320.0,
            },
            &ctx,
        );
        assert_eq!(editor.panel_layout, (220.0, 320.0));

        // An edit of an entity the scene no longer has, as after Stop, is
        // nobody's to undo.
        editor.scene.dirty = false;
        let steps = editor.history.can_undo();
        editor.on_report(
            Report::Edited {
                id: 99,
                component: "my_game::Speed".into(),
                before: serde_json::json!(1.0),
                after: serde_json::json!(2.0),
            },
            &ctx,
        );
        assert!(!editor.scene.dirty && editor.history.can_undo() == steps);

        let ime = Some([972.0, 41.0, 3.0, 18.0]);
        editor.on_report(
            Report::PanelFocus {
                keyboard: true,
                ime,
            },
            &ctx,
        );
        assert_eq!(editor.panel_focus, (true, ime));
        editor.on_report(Report::Hello { version: VERSION }, &ctx);
        assert_eq!(editor.panel_focus, (false, None));
        assert_eq!(editor.panel_layout, (0.0, 0.0));
        drop(editor);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_click_in_the_games_hierarchy_does_what_the_editors_did() {
        let dir = std::env::temp_dir().join(format!("besfa_panel_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ctx = egui::Context::default();
        let waiting = Prebuild::with_phase(crate::process::PrebuildPhase::Running, "");
        let mut editor = Editor::new(Project::new(dir.to_string_lossy()), None, &waiting, &ctx);
        editor
            .scene
            .handle(r#"@besfa {"type":"entities","entities":[{"id":7,"name":"Ship"}]}"#);
        let save_prefab = Report::Panel(PanelAction::SavePrefab);

        // Nothing selected: the game's button is off, and a stray report
        // opens nothing.
        editor.on_report(save_prefab.clone(), &ctx);
        assert!(editor.dialog.is_none());

        // The game selects on a click and says so, then asks for the prefab.
        editor.scene.handle(r#"@besfa {"type":"selected","id":7}"#);
        editor.on_report(save_prefab.clone(), &ctx);
        assert!(matches!(&editor.dialog, Some(Dialog::PrefabName(name)) if name == "Ship"));

        // Prefabs are saved in edit mode only.
        editor.dialog = None;
        editor.playing = true;
        editor.on_report(save_prefab, &ctx);
        assert!(editor.dialog.is_none());
        drop(editor);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn saves_unsaved_changes_before_updating_the_plugin() {
        let dir = std::env::temp_dir().join(format!("besfa_update_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ctx = egui::Context::default();
        let waiting = Prebuild::with_phase(crate::process::PrebuildPhase::Running, "");
        let mut editor = Editor::new(Project::new(dir.to_string_lossy()), None, &waiting, &ctx);
        let game = CliProcess::start(
            &dir,
            "cmd",
            &["/c", "ping -n 60 127.0.0.1 > nul"],
            &[],
            &ctx,
        );
        editor.game = Some(game.unwrap());
        editor.scene.dirty = true;

        // The update waits for the save, which restarting would throw away.
        editor.update_plugin(&ctx);
        assert!(editor.update.is_none());
        assert!(matches!(editor.pending_save, Some((_, AfterSave::Update))));

        // A failed save leaves the game, and its changes, alone.
        editor.on_report(
            Report::Saved {
                error: Some("disk full".into()),
            },
            &ctx,
        );
        assert!(editor.update.is_none() && !editor.starting && editor.scene.dirty);

        // Once saved, the update starts.
        editor.update_plugin(&ctx);
        editor.scene.dirty = false;
        editor.on_report(Report::Saved { error: None }, &ctx);
        assert!(editor.update.is_some());

        editor.game.as_ref().unwrap().stop();
        drop(editor);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn plays_and_rebuilds_only_once_the_scene_is_saved() {
        let dir = std::env::temp_dir().join(format!("besfa_play_save_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ctx = egui::Context::default();
        let waiting = Prebuild::with_phase(crate::process::PrebuildPhase::Running, "");
        let ready = Prebuild::with_phase(crate::process::PrebuildPhase::Ready, "");
        let mut editor = Editor::new(Project::new(dir.to_string_lossy()), None, &waiting, &ctx);
        let game = CliProcess::start(
            &dir,
            "cmd",
            &["/c", "ping -n 60 127.0.0.1 > nul"],
            &[],
            &ctx,
        );
        editor.game = Some(game.unwrap());
        let failed = || Report::Saved {
            error: Some("disk full".into()),
        };

        // Play waits for the save; a failed one leaves the edit session be,
        // since Stop would restart from the file.
        editor.scene.dirty = true;
        editor.play(&ctx);
        assert!(!editor.playing && editor.starting);
        editor.on_report(failed(), &ctx);
        assert!(!editor.playing && !editor.starting && editor.scene.dirty);

        // A rebuild the failed save put off waits for the scene to save.
        editor.rebuild(&ready, &ctx);
        editor.on_report(failed(), &ctx);
        assert!(editor.rebuild_after_save && editor.relaunch.is_none());
        editor.scene.dirty = false;
        editor.rebuild(&ready, &ctx);
        assert!(!editor.rebuild_after_save && editor.relaunch == Some(false));

        // Saved, Play plays.
        editor.relaunch = None;
        editor.starting = false;
        editor.scene.dirty = true;
        editor.play(&ctx);
        editor.scene.dirty = false;
        editor.on_report(Report::Saved { error: None }, &ctx);
        assert!(editor.playing && !editor.starting);

        drop(editor);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stop_keeps_the_game_unless_a_new_process_is_needed() {
        let dir = std::env::temp_dir().join(format!("besfa_stop_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ctx = egui::Context::default();
        let waiting = Prebuild::with_phase(crate::process::PrebuildPhase::Running, "");
        let mut editor = Editor::new(Project::new(dir.to_string_lossy()), None, &waiting, &ctx);
        let long = || {
            CliProcess::start(
                &dir,
                "cmd",
                &["/c", "ping -n 60 127.0.0.1 > nul"],
                &[],
                &ctx,
            )
        };
        let playing = |editor: &mut Editor, protocol| {
            editor.game = Some(long().unwrap());
            editor.protocol = Some(protocol);
            editor.playing = true;
            editor.relaunch = None;
            editor.starting = false;
        };

        playing(&mut editor, RESET_PROTOCOL);
        editor.scene.selected = Some(7);
        editor.stop(false, &ctx);
        assert!(!editor.playing && editor.relaunch.is_none() && editor.game.is_some());
        assert_eq!(editor.scene.selected, None);

        let restarts = |editor: &mut Editor, restart: bool| {
            editor.stop(restart, &ctx);
            assert!(editor.relaunch == Some(false) && editor.starting);
            editor.game.take().unwrap().stop();
        };
        restarts(&mut editor, true);
        playing(&mut editor, RESET_PROTOCOL);
        editor.code_changed = true;
        restarts(&mut editor, false);
        editor.code_changed = false;
        playing(&mut editor, RESET_PROTOCOL - 1);
        restarts(&mut editor, false);
        playing(&mut editor, RESET_PROTOCOL);
        editor.net_clients = 1;
        restarts(&mut editor, false);

        drop(editor);
        let _ = fs::remove_dir_all(dir);
    }

    /// The editor and what the manual shows of it, for the screenshots.
    struct Screen {
        editor: Editor,
        prebuild: Prebuild,
        /// Ringed in the picture.
        highlight: Option<egui::Rect>,
        /// Numbered markers the manual's text refers to, in order.
        callouts: Vec<egui::Pos2>,
    }

    /// Where the screenshots' game data lives: what a template game reported
    /// in edit mode with a terrain, a sky and two placed models added, and
    /// the thumbnails it drew of the models.
    const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/screenshot_data");

    /// Entities in `screenshot_data/reports.txt`.
    const CUBE: u64 = 4294966819;
    const TERRAIN: u64 = 4294966805;
    const ZOMBIE: u64 = 4294966789;

    /// The window the screenshots show, in points.
    const WINDOW: egui::Vec2 = egui::vec2(1280.0, 800.0);

    /// The panels the screenshots crop to, in points.
    const RIGHT_PANEL: egui::Rect =
        egui::Rect::from_min_max(egui::pos2(962.0, 44.0), egui::pos2(1280.0, 598.0));
    const BOTTOM_PANEL: egui::Rect =
        egui::Rect::from_min_max(egui::pos2(0.0, 598.0), egui::pos2(700.0, 800.0));

    /// Renders the hub and the editor for the user manual's screenshots,
    /// with the fonts the running editor uses, and writes them to
    /// docs/images. Not part of `cargo test`; run it after changing the UI:
    ///
    /// ```text
    /// cargo test -p besfa_editor manual_screenshots -- --ignored
    /// ```
    #[test]
    #[ignore = "writes docs/images"]
    fn manual_screenshots() {
        hub_screenshots();
        editor_screenshots();
    }

    fn hub_screenshots() {
        use egui_kittest::kittest::Queryable;
        let dir = std::env::temp_dir().join(format!("besfa_manual_hub_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = crate::project::RecentProjects {
            file: dir.join("recent_projects.json"),
        };
        for path in [r"C:\Projects\space_shooter", r"C:\Projects\my_game"] {
            store.add(&Project::new(path));
        }
        let state = (
            crate::hub::Hub::new(&store),
            store,
            Prebuild::with_phase(crate::process::PrebuildPhase::Ready, "Bevy is ready"),
        );
        let mut harness = egui_kittest::Harness::builder()
            .with_size(WINDOW + egui::Vec2::splat(2.0 * MARGIN))
            .with_pixels_per_point(PIXELS_PER_POINT)
            .wgpu()
            .build_ui_state(
                |ui, (hub, store, prebuild): &mut (crate::hub::Hub, _, Prebuild)| {
                    hub.ui(ui, store, prebuild);
                },
                state,
            );
        besfa_ui::style(&harness.ctx);
        harness.run();
        save(&mut harness, "hub", None);

        harness.get_by_label("➕ New project").click();
        harness.run();
        harness
            .get_by_role(egui::accesskit::Role::TextInput)
            .type_text(r"C:\Projects\my_game");
        harness.remove_cursor();
        harness.run();
        save(&mut harness, "hub-create", None);
        drop(harness);
        let _ = fs::remove_dir_all(dir);
    }

    fn editor_screenshots() {
        use egui_kittest::kittest::Queryable;
        const OLD: &str = "28b39927dda69b765ee1e7071794050033b4a307";
        const LATEST: &str = "51b56bc0a1d9cd2bd1f1c5d6a2a8d5c9e0f1a2b3";
        // What the template game prints once it runs, then what it reports.
        let log = [
            r"   Compiling my_game v0.1.0 (C:\Projects\my_game)",
            "    Finished `dev` profile [optimized + debuginfo] target(s) in 5.67s",
            r"     Running `C:\Users\you\AppData\Local\Besfa\target\debug\my_game.exe`",
            "2026-10-07T05:36:08.737706Z  INFO besfa_editor_plugin::viewport: Rendering into the editor viewport 1280x720.",
            "2026-10-07T05:36:08.776701Z  INFO besfa_editor_plugin::scene: Loaded scenes/main.scn.ron",
        ];
        let reports = fs::read_to_string(Path::new(DATA).join("reports.txt")).unwrap();
        let reports: Vec<&str> = reports.lines().collect();
        let game_output: Vec<&str> = log
            .iter()
            .copied()
            .chain(
                // Each entity's components are shown once it is selected.
                reports
                    .iter()
                    .copied()
                    .filter(|line| !line.contains(r#""type":"entity""#)),
            )
            .collect();
        let entity = |id: u64| {
            let id = format!(r#""type":"entity","id":{id},"#);
            *reports.iter().find(|line| line.contains(&id)).unwrap()
        };

        // A project with the models the game pictured, and their thumbnails.
        let dir = std::env::temp_dir().join(format!("besfa_manual_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let models = ["zombie-runner.glb", "oak-tree.glb", "small-stones.glb"];
        for (folder, files) in [
            ("assets/models", &models[..]),
            ("assets/prefabs", &["rock-pile.scn.ron"][..]),
            ("assets/scenes", &["main.scn.ron"][..]),
        ] {
            fs::create_dir_all(dir.join(folder)).unwrap();
            for file in files {
                fs::write(dir.join(folder).join(file), "").unwrap();
            }
        }
        let thumbnails = dir.join("target/besfa/thumbnails/models");
        fs::create_dir_all(&thumbnails).unwrap();
        for model in models {
            let file = format!("{model}.png");
            fs::copy(
                Path::new(DATA).join("thumbnails").join(&file),
                thumbnails.join(file),
            )
            .unwrap();
        }

        // Waiting for a prebuild, the editor starts no game of its own.
        let waiting = Prebuild::with_phase(crate::process::PrebuildPhase::Running, "");
        let project = Project::new(dir.to_string_lossy());
        let mut editor = Editor::new(project, None, &waiting, &egui::Context::default());
        editor.waiting_for_prebuild = false;
        editor.plugin_revision = Some(LATEST.into());
        editor.latest_plugin_revision = Some(LATEST.into());

        let screen = Screen {
            editor,
            prebuild: Prebuild::with_phase(crate::process::PrebuildPhase::Ready, "Bevy is ready"),
            highlight: None,
            callouts: Vec::new(),
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size(WINDOW + egui::Vec2::splat(2.0 * MARGIN))
            .with_pixels_per_point(PIXELS_PER_POINT)
            .wgpu()
            .build_ui_state(
                |ui, screen: &mut Screen| {
                    screen.editor.ui(ui, &screen.prebuild);
                    annotate(ui.ctx(), screen.highlight, &screen.callouts);
                },
                screen,
            );
        besfa_ui::style(&harness.ctx);
        let ctx = harness.ctx.clone();
        let read = |editor: &mut Editor, lines: &[&str]| {
            for line in lines {
                match editor.scene.handle(line) {
                    Some(report) => editor.on_report(report, &ctx),
                    None => editor.log(*line),
                }
            }
        };
        let select = |editor: &mut Editor, id: u64| {
            editor.scene.select(Some(id));
            editor.scene.handle(entity(id));
        };
        read(&mut harness.state_mut().editor, &game_output);
        harness.run_steps(4);

        // The whole window with the cube selected, its parts numbered.
        select(&mut harness.state_mut().editor, CUBE);
        harness.state_mut().callouts = vec![
            egui::pos2(250.0, 11.0),
            egui::pos2(427.0, 33.0),
            egui::pos2(87.0, 57.0),
            egui::pos2(590.0, 293.0),
            egui::pos2(1193.0, 57.0),
            egui::pos2(120.0, 612.0),
        ];
        harness.run_steps(4);
        save(&mut harness, "editor-overview", None);
        harness.state_mut().callouts.clear();

        // Terrain painting, and the sky.
        let editor = &mut harness.state_mut().editor;
        select(editor, TERRAIN);
        editor.terrain.tool = Some(besfa_protocol::TerrainTool::Paint);
        harness.run_steps(4);
        save(&mut harness, "terrain-tools", Some(RIGHT_PANEL));
        let editor = &mut harness.state_mut().editor;
        editor.terrain.tool = None;

        // Scattering two of the models.
        let editor = &mut harness.state_mut().editor;
        editor.right_tab = RightTab::Scatter;
        editor.scatter.on = true;
        editor.scatter.models = ["models/oak-tree.glb", "models/small-stones.glb"]
            .map(|path| besfa_protocol::ScatterModel {
                path: path.into(),
                weight: 1.0,
            })
            .into();
        harness.run_steps(4);
        save(&mut harness, "scatter-tab", Some(RIGHT_PANEL));
        let editor = &mut harness.state_mut().editor;
        editor.scatter.on = false;

        // The placed zombie's state machine, in its own window.
        select(editor, ZOMBIE);
        editor.right_tab = RightTab::Terrain;
        animation_window_screenshot(&harness.state().editor.scene);

        // The Assets tab, with the models' thumbnails.
        harness.state_mut().editor.bottom_tab = BottomTab::Assets;
        harness.run_steps(4);
        save(&mut harness, "assets-tab", Some(BOTTOM_PANEL));

        // The Agent chat has a window of its own, so the Agent tab shot is
        // gone; Claude's CLI is not logged in, ChatGPT's is, and Gemini
        // goes with a key.
        let editor = &mut harness.state_mut().editor;
        editor.agents = Agents::fixed(
            [Cli::LoggedOut, Cli::LoggedIn, Cli::LoggedIn],
            &[(Provider::Gemini, "gemini-key")],
        );
        editor.agent.access.insert(Provider::Gemini, Access::Key);

        // Agent Settings, each provider showing what its choice needs.
        harness.state_mut().editor.agent_open = true;
        harness.run_steps(4);
        let title = harness.get_by_label("Agent Settings").rect();
        save(
            &mut harness,
            "agent-settings",
            Some(egui::Rect::from_min_size(
                title.min - egui::vec2(12.0, 8.0),
                egui::vec2(460.0, 230.0),
            )),
        );
        let editor = &mut harness.state_mut().editor;
        editor.agent_open = false;
        editor.bottom_tab = BottomTab::Log;
        agent_window_screenshot();
        harness.run_steps(4);

        // Who Play starts.
        harness
            .get_all_by_role(egui::accesskit::Role::ComboBox)
            .find(|node| node.rect().min.y < 60.0)
            .unwrap()
            .click();
        harness.remove_cursor();
        harness.run_steps(4);
        save(
            &mut harness,
            "play-as",
            Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 200.0),
            )),
        );
        harness
            .get_all_by_role(egui::accesskit::Role::ComboBox)
            .find(|node| node.rect().min.y < 60.0)
            .unwrap()
            .click();
        harness.remove_cursor();
        harness.run_steps(4);

        // The Edit menu.
        harness.get_by_label("Edit").click();
        harness.remove_cursor();
        harness.run_steps(4);
        save(
            &mut harness,
            "menu-edit",
            Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 160.0),
            )),
        );
        harness.get_by_label("Edit").click();
        harness.remove_cursor();
        harness.run_steps(4);

        // The project's plugin is not the prebuild's: the toolbar warns, and
        // hovering explains what clicking does.
        harness.state_mut().editor.plugin_revision = Some(OLD.into());
        harness.run_steps(4);
        let rect = plugin_icon(&harness, "⚠");
        harness.hover_at(rect.center());
        harness.state_mut().highlight = Some(rect);
        // Past the tooltip's delay.
        harness.run_steps(8);
        save(&mut harness, "plugin-update-available", Some(top(110.0)));

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
        select(editor, CUBE);
        editor.plugin_revision = Some(LATEST.into());
        harness.run_steps(4);
        harness.state_mut().highlight = Some(plugin_icon(&harness, "✔"));
        harness.run_steps(4);
        save(&mut harness, "plugin-updated", None);
        harness.state_mut().highlight = None;

        // Leaving with unsaved changes, while an edit session runs.
        let editor = &mut harness.state_mut().editor;
        editor.game = Some(
            CliProcess::start(
                &dir,
                "cmd",
                &["/c", "ping -n 60 127.0.0.1 > nul"],
                &[],
                &ctx,
            )
            .unwrap(),
        );
        editor.dialog = Some(Dialog::Unsaved(AfterSave::Leave));
        harness.run_steps(4);
        save(
            &mut harness,
            "unsaved-dialog",
            Some(egui::Rect::from_center_size(
                (WINDOW / 2.0).to_pos2(),
                egui::vec2(480.0, 200.0),
            )),
        );
        // The zombie's model preview, last: the panel stays as wide as it.
        let editor = &mut harness.state_mut().editor;
        editor.dialog = None;
        editor.open_preview("models/zombie-runner.glb".into());
        let preview = reports
            .iter()
            .find(|line| line.contains(r#""type":"preview""#));
        read(editor, &[preview.unwrap()]);
        harness.run_steps(4);
        let panel = egui::Rect::from_min_max(egui::pos2(876.0, 44.0), egui::pos2(1280.0, 598.0));
        save(&mut harness, "model-preview", Some(panel));

        drop(harness);
        let _ = fs::remove_dir_all(dir);
    }

    /// Rings `highlight` and numbers `callouts` over the UI.
    fn annotate(ctx: &egui::Context, highlight: Option<egui::Rect>, callouts: &[egui::Pos2]) {
        let red = Color32::from_rgb(0xFF, 0x5F, 0x57);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Debug,
            egui::Id::new("annotate"),
        ));
        if let Some(rect) = highlight {
            painter.rect_stroke(rect.expand(3.0), 6.0, (2.0, red), egui::StrokeKind::Outside);
        }
        for (number, &at) in (1..).zip(callouts) {
            // Callouts are where they show in the picture, inside the margin.
            let at = at + egui::Vec2::splat(MARGIN);
            painter.circle_filled(at, 11.0, red);
            painter.text(
                at,
                egui::Align2::CENTER_CENTER,
                number.to_string(),
                egui::FontId::proportional(14.0),
                Color32::WHITE,
            );
        }
    }

    /// The window's top `height` points.
    fn top(height: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WINDOW.x, height))
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

    /// The Agent chat's window, which the editor opens apart from its own.
    fn agent_window_screenshot() {
        let size = egui::vec2(820.0, 460.0);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(size + egui::Vec2::splat(2.0 * MARGIN))
            .with_pixels_per_point(PIXELS_PER_POINT)
            .build_ui_state(
                |ui,
                 (panel, settings, agents, org): &mut (
                    crate::agent::AgentPanel,
                    AgentSettings,
                    Agents,
                    OrgPanel,
                )| {
                    panel.ui(ui, settings, agents, None, org);
                },
                (
                    crate::agent::AgentPanel::demo(),
                    AgentSettings::default(),
                    Agents::fixed([Cli::LoggedIn; 3], &[]),
                    OrgPanel::demo(),
                ),
            );
        harness.run_steps(4);
        save(
            &mut harness,
            "agent-window",
            Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        );

        let size = egui::vec2(1180.0, 520.0);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(size + egui::Vec2::splat(2.0 * MARGIN))
            .with_pixels_per_point(PIXELS_PER_POINT)
            .build_ui_state(
                |ui, panel: &mut OrgPanel| {
                    panel.ui(ui);
                },
                OrgPanel::demo(),
            );
        harness.run_steps(4);
        save(
            &mut harness,
            "agent-organization",
            Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        );
    }

    /// The animation window, which the editor opens apart from its own,
    /// showing `scene`'s selection.
    fn animation_window_screenshot(scene: &Scene) {
        let size = egui::vec2(480.0, 640.0);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(size + egui::Vec2::splat(2.0 * MARGIN))
            .with_pixels_per_point(PIXELS_PER_POINT)
            .build_ui(|ui| {
                animation_panel(ui, scene);
            });
        besfa_ui::style(&harness.ctx);
        harness.run_steps(4);
        save(
            &mut harness,
            "animation-window",
            Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        );
    }

    /// Writes the window, or the part of it in `crop` (points), to
    /// docs/images.
    fn save<State>(
        harness: &mut egui_kittest::Harness<State>,
        name: &str,
        crop: Option<egui::Rect>,
    ) {
        let image = harness.render().expect("render the window");
        let crop = crop.unwrap_or(egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW));
        let [x, y, width, height] = [
            crop.min.x + MARGIN,
            crop.min.y + MARGIN,
            crop.width(),
            crop.height(),
        ]
        .map(|points| (points * PIXELS_PER_POINT) as u32);
        let image = image::imageops::crop_imm(&image, x, y, width, height).to_image();
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../docs/images/{name}.png"));
        image.save(path).expect("write the screenshot");
    }
}
