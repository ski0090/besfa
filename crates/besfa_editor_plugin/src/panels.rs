//! The editor's panels the game draws itself, into the editor's viewport:
//! the Hierarchy on the left and the Inspector on the right.
//!
//! The editor names the panels it leaves to the game in `BESFA_PANELS`. The
//! game reads its own world for them, so nothing is sent to the editor to be
//! drawn. A camera of their own, above the scene view and the game's
//! cameras, draws them with egui; those cameras keep to the rest of the
//! viewport. A game with neither window nor viewport (`BESFA_HEADLESS`)
//! draws them into the scene view's picture, so tests can click them.
//!
//! The game has no window, so the editor's `pointer` commands are the
//! panels' mouse: a press that starts on a panel stays the panel's until
//! the button goes up, and one that starts on the scene stays the scene's.
//! While a field has the keyboard the game says so (`panel_focus`) and the
//! editor passes on keys, typed and composed text and the clipboard as
//! `panel_input`.
//!
//! A value changed in the Inspector is set at once, and the editor is told
//! what it was and is (`edited`) for its undo history. Every other click
//! that changes the scene goes back to the editor as a [`Report::Panel`],
//! and the editor sends the command its own panel sent: undo, the unsaved
//! mark and saving stay where they were.

use bevy::{
    camera::{CameraUpdateSystems, Hdr, RenderTarget, Viewport, visibility::RenderLayers},
    ecs::{entity::EntityHashMap, schedule::ScheduleLabel},
    prelude::*,
};
use bevy_egui::{
    EguiContext, EguiGlobalSettings, EguiInput, EguiInputSet, EguiPlugin, EguiPreUpdateSet,
    EguiSchedule,
    egui::{self, Id, RichText, ScrollArea, collapsing_header::CollapsingState},
    input::EguiInputEvent,
};
use serde_json::Value;

use besfa_protocol::{
    Button, ComponentData, ComponentType, EntityInfo, LANGUAGE_ENV, PANELS_ENV, PanelAction,
    PanelInput, PointerEvent, Report, SCALE_ENV, SpawnKind, SystemInfo, placement, tree_rows,
};
use besfa_ui::{
    MUTED, TEXT,
    i18n::t,
    inspector::{Action, Inspected, Inspector},
    panel_header, tree_row, unfolded,
};

use crate::{
    EditorOnly, edit,
    inspect::{self, Selected, report_selection, send_report},
    part_changes,
    preview::ModelPreview,
    scene_view::{EDITOR_CAMERA_ORDER, HeadlessView, SceneView},
    viewport::{VIEWPORT_HANDLE, ViewportTexture},
};

/// What the Hierarchy's add menu creates, with the names it shows.
const SPAWN_KINDS: [(SpawnKind, &str); 8] = [
    (SpawnKind::Empty, "Empty"),
    (SpawnKind::Cube, "Cube"),
    (SpawnKind::Sphere, "Sphere"),
    (SpawnKind::Plane, "Plane"),
    (SpawnKind::Light, "Light"),
    (SpawnKind::Camera, "Camera"),
    (SpawnKind::Terrain, "Terrain"),
    (SpawnKind::Sky, "Sky"),
];

/// In [`crate::default_plugins`] when the editor shows the game in its
/// viewport and left it a panel.
pub(crate) struct PanelsPlugin;

/// The panels the editor named in `BESFA_PANELS`.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
struct Asked {
    hierarchy: bool,
    inspector: bool,
}

impl Asked {
    fn of(panels: &str) -> Self {
        let named = |panel| panels.split(',').any(|name| name.trim() == panel);
        Self {
            hierarchy: named("hierarchy"),
            inspector: named("inspector"),
        }
    }
}

impl PanelsPlugin {
    /// Whether the editor named a panel this plugin draws.
    pub(crate) fn asked() -> bool {
        std::env::var(PANELS_ENV).is_ok_and(|panels| Asked::of(&panels) != Asked::default())
    }
}

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        // ponytail: a game that adds `EguiPlugin` itself cannot run under
        // the editor yet. Bevy takes a plugin only once and this one adds it
        // first, as part of `default_plugins()`, and egui cannot be added
        // before Bevy's own plugins either. Take the game's when a game
        // with egui of its own shows up.
        if !app.is_plugin_added::<EguiPlugin>() {
            app.add_plugins(EguiPlugin::default());
            app.world_mut()
                .resource_mut::<EguiGlobalSettings>()
                .auto_create_primary_context = false;
        }
        besfa_ui::i18n::set_korean(
            std::env::var(LANGUAGE_ENV).is_ok_and(|language| language == "ko"),
        );
        let scale = std::env::var(SCALE_ENV)
            .ok()
            .and_then(|scale| scale.parse().ok());
        app.insert_resource(Panels {
            asked: Asked::of(&std::env::var(PANELS_ENV).unwrap_or_default()),
            scale: valid_scale(scale).unwrap_or(1.0),
            ..default()
        })
        .init_resource::<Listed>()
        .init_resource::<Inspecting>()
        // Startup made what the camera draws into, and Bevy looks for it
        // as soon as startup is over.
        .add_systems(PostStartup, aim.before(CameraUpdateSystems))
        .add_systems(
            PreUpdate,
            keep_focused
                .after(EguiInputSet::WriteEguiEvents)
                .before(EguiPreUpdateSet::BeginPass),
        )
        .add_systems(PanelsPass, draw)
        .add_systems(
            PostUpdate,
            (aim, keep_cameras_clear).before(CameraUpdateSystems),
        );
    }

    /// The camera is made before any schedule runs, so that Stop, which
    /// despawns what the game and its startup made, keeps it: with it stay
    /// the panels' egui, its theme and font, and how wide the panels are.
    /// What is folded does not stay: Stop numbers the entities anew.
    fn finish(&self, app: &mut App) {
        app.world_mut().spawn((
            Name::new("Panels camera"),
            EditorOnly,
            PanelsCamera,
            Camera2d,
            Camera {
                // After the scene view and the model preview.
                order: EDITOR_CAMERA_ORDER + 10,
                // Over what the cameras below drew.
                clear_color: ClearColorConfig::None,
                // Until there is something to draw into ([`aim`]).
                is_active: false,
                ..default()
            },
            RenderTarget::None { size: UVec2::ONE },
            // Sees nothing of the scene: Bevy 0.20 draws no meshes for the
            // scene view while another active camera sees them.
            RenderLayers::none(),
            // bevy_egui draws into Rgba8UnormSrgb or, with Hdr, Rgba16Float.
            // A plain camera's picture has the viewport's format, BGRA, and
            // the game would quit on the mismatch.
            Hdr,
            Msaa::Off,
            EguiSchedule::new(PanelsPass),
        ));
    }
}

/// The schedule the panels' egui context runs, apart from a game's own egui.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct PanelsPass;

/// The camera the panels are drawn through, over the scene view and the game.
#[derive(Component)]
pub(crate) struct PanelsCamera;

/// The entities the Hierarchy lists: what the game last reported to the
/// editor, in tree order.
#[derive(Resource, Default)]
pub(crate) struct Listed {
    entities: Vec<EntityInfo>,
    rows: Vec<(usize, usize)>,
}

impl Listed {
    pub(crate) fn set(&mut self, entities: &[EntityInfo]) {
        self.entities = entities.to_vec();
        self.rows = tree_rows(&self.entities);
    }
}

/// What the Inspector shows, as the game reports it to the editor: the
/// selected entity's components, and the game's systems and the components
/// it can add, which do not change once the game runs.
#[derive(Resource, Default)]
pub(crate) struct Inspecting {
    /// An entity's components as of the last frame's report, and whose. A
    /// frame after the selection changes they are still the last one's.
    pub(crate) entity: Option<Entity>,
    pub(crate) components: Vec<ComponentData>,
    systems: Vec<SystemInfo>,
    addable: Vec<ComponentType>,
    crate_name: Option<String>,
    /// The systems and components are read, once.
    listed: bool,
    panel: Inspector,
}

/// Where the panels are and whose the pointer and the keyboard are.
#[derive(Resource, Default)]
pub(crate) struct Panels {
    asked: Asked,
    /// The pixels a point of the editor's interface takes.
    scale: f32,
    /// The viewport pixels the Hierarchy covers from the left edge and the
    /// Inspector from the right, and the viewport's size in pixels, as drawn
    /// last frame.
    left: f32,
    right: f32,
    size: Vec2,
    /// How far past its edge, in pixels, a panel's edge is still grabbed to
    /// make it wider or narrower.
    grab: f32,
    /// Where the pointer was last, in viewport pixels.
    pointer: Option<Vec2>,
    /// While a button is down: whether the press started on a panel.
    held: Option<bool>,
    /// The cameras this module moved off the panels, with the part of the
    /// viewport it gave each: where it starts and its size.
    kept_clear: EntityHashMap<(UVec2, UVec2)>,
    /// What the editor was last told about the keyboard.
    focus: (bool, Option<[f32; 4]>),
    /// What the editor was last told the panels cover, left and right.
    told: (f32, f32),
    /// A field just lost the keyboard and has yet to set what was typed.
    settle: bool,
}

impl Panels {
    /// Whether a panel's strip, with the edge it is resized by, lies over
    /// the viewport at `x` pixels.
    fn over(&self, x: f32) -> bool {
        (self.left > 0.0 && x < self.left + self.grab)
            || (self.right > 0.0 && x >= self.size.x - self.right - self.grab)
    }

    /// The middle of the scene view the panels leave, in viewport pixels.
    fn scene_middle(&self) -> Option<[f32; 2]> {
        let (from, to) = (self.left, self.size.x - self.right);
        (to > from).then(|| [(from + to) / 2.0, self.size.y / 2.0])
    }
}

/// Whether a panel lies over the viewport at `pointer`, in viewport pixels.
pub(crate) fn covers(world: &mut World, pointer: Vec2) -> bool {
    let Some(panels) = world.get_resource::<Panels>() else {
        return false;
    };
    let (strip, scale) = (panels.over(pointer.x), panels.scale);
    strip || popup_at(world, pointer / scale)
}

/// Whether a press that started on a panel is still held.
pub(crate) fn holds(world: &World) -> bool {
    world
        .get_resource::<Panels>()
        .is_some_and(|panels| panels.held == Some(true))
}

/// Whether a menu, a popup or a dialog of the panels, which may reach past
/// their strips, is at `point` in egui's points.
fn popup_at(world: &mut World, point: Vec2) -> bool {
    let mut contexts = world.query_filtered::<&mut EguiContext, With<PanelsCamera>>();
    contexts.single_mut(world).is_ok_and(|mut context| {
        context
            .get_mut()
            .layer_id_at(egui::pos2(point.x, point.y))
            // The background is the panels' own layer, as wide as the viewport.
            .is_some_and(|layer| layer.order != egui::Order::Background)
    })
}

fn valid_scale(scale: Option<f32>) -> Option<f32> {
    scale.filter(|scale| scale.is_finite() && (0.25..=8.0).contains(scale))
}

/// The editor's interface takes `scale` pixels a point from now on.
pub(crate) fn rescale(world: &mut World, scale: Option<f32>) {
    if let (Some(mut panels), Some(scale)) =
        (world.get_resource_mut::<Panels>(), valid_scale(scale))
    {
        panels.scale = scale;
    }
}

/// Points the panels' camera at where the scene view is drawn: the picture
/// a game without a viewport draws into, which every Stop makes anew, or
/// the editor's viewport once it is open. With neither, as when the
/// viewport could not be opened, the camera stays off.
fn aim(
    headless: Option<Res<HeadlessView>>,
    viewport: Option<Res<ViewportTexture>>,
    camera: Single<(&mut Camera, &mut RenderTarget), With<PanelsCamera>>,
) {
    let (mut camera, mut target) = camera.into_inner();
    let drawn = headless.is_some() || viewport.is_some();
    if camera.is_active != drawn {
        camera.is_active = drawn;
    }
    if drawn && !shares_target(&target, headless.as_deref()) {
        *target = match headless.as_deref() {
            Some(view) => RenderTarget::Image(view.0.clone().into()),
            None => RenderTarget::TextureView(VIEWPORT_HANDLE),
        };
    }
}

fn shares_target(target: &RenderTarget, headless: Option<&HeadlessView>) -> bool {
    match (target, headless) {
        (RenderTarget::Image(image), Some(view)) => image.handle == view.0,
        (RenderTarget::TextureView(handle), None) => *handle == VIEWPORT_HANDLE,
        _ => false,
    }
}

/// egui asks whether its window has the keyboard before a field says it has
/// focus, and bevy_egui answers for a window. The panels have none: theirs
/// is the editor's viewport, which says when it loses the keyboard
/// ([`unfocus`]).
fn keep_focused(mut input: Single<&mut EguiInput, With<PanelsCamera>>) {
    input.focused = true;
}

/// Hands `events` to the panels' egui for its next pass.
fn feed(world: &mut World, events: impl IntoIterator<Item = egui::Event>) {
    let mut cameras = world.query_filtered::<Entity, With<PanelsCamera>>();
    if let Ok(context) = cameras.single(world) {
        for event in events {
            world.write_message(EguiInputEvent { context, event });
        }
    }
}

/// One `pointer` command from the editor, in viewport pixels. True when the
/// panels took it, so the scene view and the game are not told.
pub(crate) fn pointer(world: &mut World, event: &PointerEvent) -> bool {
    let Some(panels) = world.get_resource::<Panels>() else {
        return false;
    };
    let scale = panels.scale;
    let at = match *event {
        PointerEvent::Move { x, y }
        | PointerEvent::Down { x, y, .. }
        | PointerEvent::Up { x, y, .. } => Some(Vec2::new(x, y)),
        PointerEvent::Scroll { .. } => panels.pointer,
    };
    // A panel's strip, or a menu or dialog of the panels reaching past it.
    let over = at.is_some_and(|at| covers(world, at));
    let mut panels = world.resource_mut::<Panels>();
    panels.pointer = at;
    let point = |x: f32, y: f32| egui::pos2(x / scale, y / scale);
    let mut events = Vec::new();
    let taken = match *event {
        PointerEvent::Move { x, y } => {
            // Always, so a row stops looking hovered once the pointer leaves.
            events.push(egui::Event::PointerMoved(point(x, y)));
            panels.held.unwrap_or(over)
        }
        // Only the panels' own presses: egui would act on the scene's too,
        // and drag a panel's edge along with a drag that starts near it.
        PointerEvent::Down { x, y, button } => {
            let on_panel = *panels.held.get_or_insert(over);
            if on_panel {
                events.push(egui::Event::PointerMoved(point(x, y)));
                events.push(pressed(point(x, y), button, true));
            }
            on_panel
        }
        PointerEvent::Up { x, y, button } => {
            let on_panel = panels.held.take().unwrap_or(over);
            if on_panel {
                events.push(pressed(point(x, y), button, false));
            }
            on_panel
        }
        PointerEvent::Scroll { delta } => {
            if over {
                events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    // The editor's scroll is in its points and positive
                    // downwards; egui moves what is shown down on a
                    // positive one.
                    delta: egui::vec2(0.0, -delta),
                    phase: egui::TouchPhase::Move,
                    modifiers: default(),
                });
            }
            over
        }
    };
    // A press on the scene takes the keyboard from a field and closes a
    // menu the panels left open.
    if matches!(event, PointerEvent::Down { .. }) && !taken {
        unfocus(world);
        let mut contexts = world.query_filtered::<&mut EguiContext, With<PanelsCamera>>();
        if let Ok(mut context) = contexts.single_mut(world) {
            egui::Popup::close_all(context.get_mut());
        }
    }
    feed(world, events);
    taken
}

fn pressed(pos: egui::Pos2, button: Button, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: match button {
            Button::Left => egui::PointerButton::Primary,
            Button::Middle => egui::PointerButton::Middle,
            Button::Right => egui::PointerButton::Secondary,
        },
        pressed,
        modifiers: default(),
    }
}

/// The editor's keyboard for the field that has it. An error for a key
/// egui has no name for.
pub(crate) fn input(world: &mut World, input: PanelInput) -> Result<(), String> {
    let event = match input {
        PanelInput::Key {
            key,
            pressed,
            repeat,
            modifiers,
        } => egui::Event::Key {
            key: egui::Key::from_name(&key).ok_or_else(|| format!("{key} is not a key"))?,
            physical_key: None,
            pressed,
            repeat,
            modifiers: egui::Modifiers {
                alt: modifiers.alt,
                ctrl: modifiers.ctrl,
                shift: modifiers.shift,
                mac_cmd: false,
                // What egui's shortcuts ask for: Ctrl, off macOS.
                command: modifiers.ctrl,
            },
        },
        PanelInput::Text { text } => egui::Event::Text(text),
        PanelInput::Preedit { text } => egui::Event::Ime(egui::ImeEvent::Preedit {
            text,
            active_range_chars: None,
        }),
        PanelInput::Commit { text } => egui::Event::Ime(egui::ImeEvent::Commit(text)),
        PanelInput::Copy => egui::Event::Copy,
        PanelInput::Cut => egui::Event::Cut,
        PanelInput::Paste { text } => egui::Event::Paste(text),
    };
    feed(world, [event]);
    Ok(())
}

/// Takes the keyboard from whatever field of the panels has it: the
/// editor's viewport lost it, or the scene was pressed. The field sets what
/// was typed in it when it is next drawn, so the editor's commands that
/// follow wait for that ([`settling`]).
pub(crate) fn unfocus(world: &mut World) {
    let mut contexts = world.query_filtered::<&mut EguiContext, With<PanelsCamera>>();
    let Ok(mut context) = contexts.single_mut(world) else {
        return;
    };
    let had = context.get_mut().memory_mut(|memory| {
        let focused = memory.focused();
        if let Some(focused) = focused {
            memory.surrender_focus(focused);
        }
        focused.is_some()
    });
    if had && let Some(mut panels) = world.get_resource_mut::<Panels>() {
        panels.settle = true;
    }
}

/// Whether a field lost the keyboard since this was last asked, and has
/// yet to be drawn without it.
pub(crate) fn settling(world: &mut World) -> bool {
    world
        .get_resource_mut::<Panels>()
        .is_some_and(|mut panels| std::mem::take(&mut panels.settle))
}

/// Stop started the game over in this process: no field has the keyboard,
/// and the editor, which forgets what it was told at the game's `hello`,
/// is told again where the panels are and who has the keyboard.
pub(crate) fn reset(world: &mut World) {
    unfocus(world);
    if let Some(mut panels) = world.get_resource_mut::<Panels>() {
        panels.settle = false;
        panels.focus = (false, None);
        panels.told = (0.0, 0.0);
    }
}

/// Draws the panels and remembers where, for [`pointer`] and the cameras.
fn draw(world: &mut World) {
    let mut contexts = world.query_filtered::<&mut EguiContext, With<PanelsCamera>>();
    let Ok(mut context) = contexts.single_mut(world) else {
        return;
    };
    // The same egui, shared: the world is free for what the panels change.
    let ctx = context.get_mut().clone();
    let asked = world.resource::<Panels>().asked;
    let scale = world.resource::<Panels>().scale;
    if !ctx.data(|data| data.get_temp::<bool>(Id::new("panels styled")).is_some()) {
        ctx.data_mut(|data| data.insert_temp(Id::new("panels styled"), true));
        besfa_ui::style(&ctx);
    }
    if ctx.pixels_per_point() != scale {
        ctx.set_pixels_per_point(scale);
    }
    // The model preview shows one model, not the scene the panels are about;
    // its panel is the editor's.
    let previewing = world
        .get_resource::<ModelPreview>()
        .is_some_and(|preview| preview.showing);
    let mut root = egui::Ui::new(
        ctx.clone(),
        "panels".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    // Saving and applying prefabs is for edit mode, as the scene is.
    let editing = world
        .get_resource::<SceneView>()
        .is_some_and(|view| !view.stopped);
    let selected = world.resource::<Selected>().0;

    let mut left = 0.0;
    if asked.hierarchy && !previewing {
        let panel = egui::Panel::left("hierarchy")
            .default_size(220.0)
            .show(&mut root, |ui| {
                hierarchy(ui, world.resource::<Listed>(), selected, editing)
            });
        left = panel.response.rect.right() * scale;
        match panel.inner {
            // The game reports the selected entity's components to the editor.
            Some(Click::Select(entity)) => {
                world.resource_mut::<Selected>().0 = entity;
                report_selection(entity);
            }
            // What the add menu makes is placed in the middle of the scene
            // view, which only the game knows the place of.
            Some(Click::Ask(PanelAction::Spawn { kind, .. })) => {
                let pointer = world.resource::<Panels>().scene_middle();
                send_report(&Report::Panel(PanelAction::Spawn { kind, pointer }));
            }
            Some(Click::Ask(action)) => send_report(&Report::Panel(action)),
            None => {}
        }
    }

    let mut right = 0.0;
    if asked.inspector && !previewing {
        list_once(world);
        let panel = world.resource_scope(|world, mut inspecting: Mut<Inspecting>| {
            let inspecting = &mut *inspecting;
            let listed = world.resource::<Listed>();
            let entity = selected.and_then(|entity| {
                listed
                    .entities
                    .iter()
                    .find(|listed| listed.id == entity.to_bits())
            });
            // Not the last selection's, which they are for a frame after
            // the selection changes: a field would set its value from them.
            let components: &[ComponentData] = if inspecting.entity == selected {
                &inspecting.components
            } else {
                &[]
            };
            let inspected = Inspected {
                selected: selected.map(Entity::to_bits),
                entity,
                placement: entity.and_then(|entity| placement(&listed.entities, entity)),
                components,
                systems: &inspecting.systems,
                addable: &inspecting.addable,
                crate_name: inspecting.crate_name.as_deref(),
            };
            egui::Panel::right("inspector")
                .default_size(320.0)
                .show(&mut root, |ui| inspecting.panel.ui(ui, &inspected, editing))
        });
        right = ctx.viewport_rect().right() * scale - panel.response.rect.left() * scale;
        if let (Some(action), Some(entity)) = (panel.inner, selected)
            && world.resource::<Inspecting>().entity == selected
        {
            inspect_action(world, entity, action);
        }
    }

    let focus = (ctx.egui_wants_keyboard_input(), ime_rect(&ctx, scale));
    // Text copied or cut in a field, which only the editor can put on the
    // clipboard: the game is built without one.
    let copied: Vec<String> = ctx.output(|output| {
        output
            .commands
            .iter()
            .filter_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    });
    for text in copied {
        send_report(&Report::Copied { text });
    }
    let mut panels = world.resource_mut::<Panels>();
    panels.left = left;
    panels.right = right;
    let size = ctx.viewport_rect().size() * scale;
    panels.size = Vec2::new(size.x, size.y);
    panels.grab = ctx.global_style().interaction.resize_grab_radius_side * scale;
    if panels.focus != focus {
        panels.focus = focus;
        send_report(&Report::PanelFocus {
            keyboard: focus.0,
            ime: focus.1,
        });
    }
    if panels.told != (left, right) {
        panels.told = (left, right);
        send_report(&Report::PanelLayout { left, right });
    }
}

/// Where the focused field's text cursor is, in viewport pixels.
fn ime_rect(ctx: &egui::Context, scale: f32) -> Option<[f32; 4]> {
    let rect = ctx.output(|output| output.ime.as_ref().map(|ime| ime.cursor_rect))?;
    Some([rect.left(), rect.top(), rect.width(), rect.height()].map(|side| (side * scale).round()))
}

/// Reads the game's systems and the components it can add, which the
/// Inspector lists, the first time it is drawn.
fn list_once(world: &mut World) {
    if world.resource::<Inspecting>().listed {
        return;
    }
    let systems = inspect::systems(world);
    let addable = inspect::addable_components(world);
    let mut inspecting = world.resource_mut::<Inspecting>();
    inspecting.listed = true;
    if let Report::Systems {
        game_crate,
        systems,
    } = systems
    {
        inspecting.systems = systems;
        inspecting.crate_name = game_crate;
    }
    if let Report::Components { components } = addable {
        inspecting.addable = components;
    }
}

/// What the Inspector asked for `entity`. A new value is set here and now,
/// and the editor told for its undo history, as with a handle dragged in
/// the scene view. The rest is the editor's to do.
fn inspect_action(world: &mut World, entity: Entity, action: Action) {
    let report = match action {
        Action::Set(component, after) => {
            let before = world
                .resource::<Inspecting>()
                .components
                .iter()
                .find(|listed| listed.path == component)
                .and_then(|listed| listed.value.clone())
                .unwrap_or(Value::Null);
            if let Err(error) = edit::set(world, entity, &component, &after) {
                error!("Could not apply the Inspector's change: {error}");
                return;
            }
            part_changes::edited(world, entity, &component);
            Report::Edited {
                id: entity.to_bits(),
                component,
                before,
                after,
            }
        }
        Action::Insert(component) => Report::Panel(PanelAction::Insert { component }),
        Action::Remove(component) => Report::Panel(PanelAction::Remove { component }),
        Action::ApplyPrefab(id) => Report::Panel(PanelAction::ApplyPrefab { id }),
    };
    send_report(&report);
}

#[derive(Debug, PartialEq)]
enum Click {
    Select(Option<Entity>),
    /// For the editor to do, so it is recorded for undo.
    Ask(PanelAction),
}

/// The scene's entities as a tree; clicking one selects it, clicking the
/// selected one clears the selection. The header adds, duplicates and
/// deletes entities, and saves one as a prefab while `editing`.
fn hierarchy(
    ui: &mut egui::Ui,
    listed: &Listed,
    selected: Option<Entity>,
    editing: bool,
) -> Option<Click> {
    let mut click = None;
    let selected = selected.map(Entity::to_bits);
    panel_header(ui, "Hierarchy", |ui| {
        let buttons = [
            (editing, "📦", "Save as prefab", PanelAction::SavePrefab),
            (true, "🗑", "Delete (Delete)", PanelAction::Delete),
            (true, "🗐", "Duplicate (Ctrl+D)", PanelAction::Duplicate),
        ];
        for (shown, icon, tip, action) in buttons {
            if shown
                && ui
                    .add_enabled(selected.is_some(), egui::Button::new(icon).small())
                    .on_hover_text(t(tip))
                    .clicked()
            {
                click = Some(Click::Ask(action));
            }
        }
        ui.menu_button("➕", |ui| {
            for (kind, label) in SPAWN_KINDS {
                if ui.button(t(label)).clicked() {
                    let pointer = None;
                    click = Some(Click::Ask(PanelAction::Spawn { kind, pointer }));
                }
            }
        })
        .response
        .on_hover_text(t("Add entity"));
    });
    let rows = &listed.rows;
    let entity = |&(at, _): &(usize, usize)| &listed.entities[at];
    let depth = |&(_, depth): &(usize, usize)| depth;
    // Folds start closed, as Unity's do: placed models' parts, thousands in a
    // big scene, and a group's members are seldom what is picked here.
    let fold = |row: &(usize, usize)| (Id::new(("hierarchy fold", entity(row).id)), false);
    // A new selection, picked in the scene view too, opens the folds above
    // it once it is listed.
    let at = rows.iter().position(|row| Some(entity(row).id) == selected);
    let listed_selection = at.map(|at| entity(&rows[at]).id);
    let shown = Id::new("hierarchy selection");
    if ui.data(|data| data.get_temp(shown)) != Some(listed_selection) {
        ui.data_mut(|data| data.insert_temp(shown, listed_selection));
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
                    let is_selected = Some(entity.id) == selected;
                    // Entities the game spawns while running are not saved.
                    let mut text = RichText::new(entity.label());
                    text = if entity.scene { text } else { text.italics() };
                    text = text.color(if entity.name.is_none() || !entity.scene {
                        MUTED
                    } else {
                        TEXT
                    });
                    if ui.selectable_label(is_selected, text).clicked() {
                        let pick = (!is_selected).then(|| Entity::from_bits(entity.id));
                        click = Some(Click::Select(pick));
                    }
                });
            }
        },
    );
    click
}

/// The part of a target `size` pixels wide and high that the panels, `left`
/// and `right` pixels wide, leave: where it starts and its size. None when
/// they leave nothing or take nothing.
fn rest(size: UVec2, left: u32, right: u32) -> Option<(UVec2, UVec2)> {
    let taken = left + right;
    (taken > 0 && size.x > taken).then(|| (UVec2::new(left, 0), UVec2::new(size.x - taken, size.y)))
}

/// What Bevy makes of the part `given` to a camera when its target is now
/// `size`: the part cut off at the target's edge.
fn cut(given: (UVec2, UVec2), size: UVec2) -> (UVec2, UVec2) {
    let mut viewport = Viewport {
        physical_position: given.0,
        physical_size: given.1,
        ..default()
    };
    viewport.clamp_to_size(size);
    (viewport.physical_position, viewport.physical_size)
}

/// Keeps the scene view and the game's cameras to the part of the viewport
/// the panels leave, so nothing is drawn under them. A camera with a
/// viewport of its own keeps it, also one the game sets later, and a camera
/// that stops drawing into the viewport gets its whole target back: a game
/// camera is switched off again before the scene can be saved, so the file
/// never holds the editor's layout.
// ponytail: a game with split-screen viewports of its own draws under the
// panels; scale its viewports into the rest if a game needs that.
fn keep_cameras_clear(
    mut panels: ResMut<Panels>,
    headless: Option<Res<HeadlessView>>,
    mut cameras: Query<(Entity, &mut Camera, &RenderTarget), Without<PanelsCamera>>,
) {
    let (left, right) = (panels.left.ceil() as u32, panels.right.ceil() as u32);
    for (entity, mut camera, target) in &mut cameras {
        let now = camera
            .viewport
            .as_ref()
            .map(|viewport| (viewport.physical_position, viewport.physical_size));
        // Ours while it is what this system gave it, also after Bevy cut it
        // to a target that shrank; the game's once the game has set another.
        let ours = match (panels.kept_clear.get(&entity), now) {
            (Some(given), Some(now)) => {
                *given == now
                    || camera.physical_target_size().map(|size| cut(*given, size)) == Some(now)
            }
            _ => false,
        };
        if !ours {
            panels.kept_clear.remove(&entity);
        }
        let in_viewport = camera.is_active && shares_target(target, headless.as_deref());
        let rest = camera
            .physical_target_size()
            .filter(|_| in_viewport)
            .and_then(|size| rest(size, left, right));
        match rest {
            Some(rest) if ours || now.is_none() => {
                if now != Some(rest) {
                    camera.viewport = Some(Viewport {
                        physical_position: rest.0,
                        physical_size: rest.1,
                        ..default()
                    });
                }
                panels.kept_clear.insert(entity, rest);
            }
            None if ours => {
                camera.viewport = None;
                panels.kept_clear.remove(&entity);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panels(left: f32, right: f32) -> World {
        let mut world = World::new();
        world.insert_resource(Panels {
            scale: 2.0,
            left,
            right,
            size: Vec2::new(1000.0, 600.0),
            ..default()
        });
        world
    }

    #[test]
    fn a_press_belongs_to_where_it_started() {
        let mut world = panels(200.0, 300.0);
        let down = |x| PointerEvent::Down {
            x,
            y: 10.0,
            button: Button::Left,
        };
        let up = |x| PointerEvent::Up {
            x,
            y: 10.0,
            button: Button::Left,
        };
        let moved = |x| PointerEvent::Move { x, y: 10.0 };

        // Started on a panel: the scene never hears of it, even outside.
        assert!(pointer(&mut world, &down(50.0)));
        assert!(pointer(&mut world, &moved(500.0)));
        assert!(pointer(&mut world, &up(500.0)));
        // Started on the scene: a drag across a panel stays the scene's.
        assert!(!pointer(&mut world, &down(500.0)));
        assert!(!pointer(&mut world, &moved(50.0)));
        assert!(!pointer(&mut world, &up(900.0)));
        // No button down: whatever is under the pointer.
        assert!(pointer(&mut world, &moved(50.0)));
        assert!(pointer(&mut world, &PointerEvent::Scroll { delta: 30.0 }));
        assert!(!pointer(&mut world, &moved(500.0)));
        assert!(!pointer(&mut world, &PointerEvent::Scroll { delta: 30.0 }));
        // The Inspector's strip, on the right.
        assert!(pointer(&mut world, &moved(701.0)));
        assert!(!pointer(&mut world, &moved(699.0)));
    }

    #[test]
    fn without_panels_the_pointer_is_the_scenes() {
        let mut world = World::new();
        assert!(!pointer(&mut world, &PointerEvent::Move { x: 1.0, y: 1.0 }));
        assert!(!covers(&mut world, Vec2::ONE));
        assert!(!holds(&world));
    }

    #[test]
    fn a_press_on_a_panel_is_held_until_it_is_released() {
        let mut world = panels(200.0, 300.0);
        // The edges are grabbed from the scene's side too.
        world.resource_mut::<Panels>().grab = 6.0;
        assert!(covers(&mut world, Vec2::new(204.0, 10.0)));
        assert!(!covers(&mut world, Vec2::new(207.0, 10.0)));
        assert!(covers(&mut world, Vec2::new(695.0, 10.0)));
        assert!(!covers(&mut world, Vec2::new(693.0, 10.0)));
        let (y, button) = (10.0, Button::Right);
        pointer(&mut world, &PointerEvent::Down { x: 50.0, y, button });
        assert!(holds(&world), "the scene view's fly keys wait");
        pointer(
            &mut world,
            &PointerEvent::Up {
                x: 500.0,
                y,
                button,
            },
        );
        assert!(!holds(&world));
        // A press on the scene is not a panel's.
        pointer(
            &mut world,
            &PointerEvent::Down {
                x: 500.0,
                y,
                button,
            },
        );
        assert!(!holds(&world));
    }

    #[test]
    fn leaves_the_cameras_what_the_panels_do_not_cover() {
        let size = UVec2::new(1280, 720);
        assert_eq!(
            rest(size, 220, 320),
            Some((UVec2::new(220, 0), UVec2::new(740, 720)))
        );
        assert_eq!(
            rest(size, 220, 0),
            Some((UVec2::new(220, 0), UVec2::new(1060, 720)))
        );
        assert_eq!(rest(size, 0, 0), None, "no panel, the whole target");
        assert_eq!(rest(size, 640, 640), None, "nothing left to draw into");
        // The editor made the viewport smaller: Bevy cuts the part off at
        // the new edge, which is still the part this module gave.
        let given = rest(size, 220, 320).unwrap();
        assert_eq!(
            cut(given, UVec2::new(900, 500)),
            (UVec2::new(220, 0), UVec2::new(680, 500))
        );
        assert_eq!(cut(given, size), given);
        let panels = Panels {
            left: 220.0,
            right: 320.0,
            size: size.as_vec2(),
            ..default()
        };
        assert_eq!(panels.scene_middle(), Some([590.0, 360.0]));
        assert_eq!(Panels::default().scene_middle(), None);
    }

    #[test]
    fn reads_the_panels_the_editor_named() {
        assert_eq!(
            Asked::of("hierarchy"),
            Asked {
                hierarchy: true,
                inspector: false
            }
        );
        assert_eq!(
            Asked::of("hierarchy, inspector"),
            Asked {
                hierarchy: true,
                inspector: true
            }
        );
        // A panel a later editor names is not this plugin's to draw.
        assert_eq!(Asked::of("scatter"), Asked::default());
    }

    #[test]
    fn names_keys_the_way_egui_does() {
        let mut world = World::new();
        let key = |key: &str| PanelInput::Key {
            key: key.into(),
            pressed: true,
            repeat: false,
            modifiers: default(),
        };
        assert!(input(&mut world, key("Enter")).is_ok());
        assert!(input(&mut world, key("A")).is_ok());
        assert!(input(&mut world, key("NoSuchKey")).is_err());
    }

    fn listed(entities: &str) -> Listed {
        let report = format!(r#"@besfa {{"type":"entities","entities":[{entities}]}}"#);
        let Some(Report::Entities { entities }) = Report::parse(&report) else {
            panic!("not an entities report: {report}");
        };
        let mut listed = Listed::default();
        listed.set(&entities);
        listed
    }

    #[test]
    fn opens_the_folds_above_a_new_selection() {
        use egui_kittest::{Harness, kittest::Queryable};

        let entities = listed(
            r#"{"id":1,"name":"Ship"},{"id":2,"name":"Hull","parent":1},{"id":3,"name":"Mast","parent":2}"#,
        );
        let mut harness = Harness::new_ui_state(
            |ui, selected: &mut Option<Entity>| {
                hierarchy(ui, &entities, *selected, false);
            },
            None,
        );
        harness.run();
        assert!(harness.query_by_label("Hull").is_none(), "starts folded");

        // As a pick in the scene view selects it, two folds down.
        *harness.state_mut() = Some(Entity::from_bits(3));
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
        let entities = listed(&entities.join(","));
        let mut harness = Harness::new_ui(|ui| {
            hierarchy(ui, &entities, None, false);
        });
        harness.run();
        assert!(harness.query_by_label("Stave").is_none(), "starts folded");
        assert!(harness.query_by_label("Rock 3").is_some());
        assert!(harness.query_by_label("Rock 999").is_none(), "out of view");

        harness.get_by_label("Unfold").click();
        harness.run();
        assert!(harness.query_by_label("Stave").is_some());
    }

    #[test]
    fn a_click_selects_and_the_buttons_ask_the_editor() {
        use egui_kittest::{Harness, kittest::Queryable};

        let entities = listed(r#"{"id":1,"name":"Ship"}"#);
        let mut harness = Harness::new_ui_state(
            |ui, state: &mut (Option<Entity>, Option<PanelAction>)| match hierarchy(
                ui, &entities, state.0, true,
            ) {
                Some(Click::Select(entity)) => state.0 = entity,
                Some(Click::Ask(action)) => state.1 = Some(action),
                None => {}
            },
            (None, None),
        );
        harness.run();
        harness.get_by_label("Ship").click();
        harness.run();
        assert_eq!(harness.state().0, Some(Entity::from_bits(1)));

        harness.get_by_label("🗑").click();
        harness.run();
        assert_eq!(harness.state().1, Some(PanelAction::Delete));

        // The selected row again clears the selection.
        harness.get_by_label("Ship").click();
        harness.run();
        assert_eq!(harness.state().0, None);
    }

    #[test]
    fn takes_only_a_sane_scale() {
        assert_eq!(valid_scale(Some(1.5)), Some(1.5));
        assert_eq!(valid_scale(Some(0.0)), None);
        assert_eq!(valid_scale(Some(f32::NAN)), None);
        assert_eq!(valid_scale(None), None);
    }
}
