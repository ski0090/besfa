//! The editor's panels the game draws itself, into the editor's viewport.
//!
//! The editor names the panels it leaves to the game in `BESFA_PANELS`. The
//! game reads its own world for them, so nothing is sent to the editor to be
//! drawn. A camera of their own, above the scene view and the game's
//! cameras, draws them with egui; those cameras keep to the rest of the
//! viewport. A game with neither window nor viewport (`BESFA_HEADLESS`)
//! draws them into the scene view's picture, so tests can click them.
//! The game has no window, so the editor's `pointer` commands are
//! the panels' mouse: a press that starts on a panel stays the panel's until
//! the button goes up, and one that starts on the scene stays the scene's.
//!
//! A click that changes the scene goes back to the editor as a
//! [`Report::Panel`], and the editor sends the command its own panel sent:
//! undo, the unsaved mark and saving stay where they were.
// ponytail: only the Hierarchy; the Inspector comes with keys and text.

use bevy::{
    camera::{CameraUpdateSystems, Hdr, RenderTarget, Viewport, visibility::RenderLayers},
    ecs::{entity::EntityHashSet, schedule::ScheduleLabel},
    prelude::*,
};
use bevy_egui::{
    EguiContext, EguiGlobalSettings, EguiPlugin, EguiSchedule,
    egui::{self, Id, RichText, ScrollArea, collapsing_header::CollapsingState},
    input::EguiInputEvent,
};

use besfa_protocol::{
    Button, EntityInfo, LANGUAGE_ENV, PANELS_ENV, PanelAction, PointerEvent, Report, SCALE_ENV,
    SpawnKind, tree_rows,
};
use besfa_ui::{MUTED, TEXT, i18n::t, panel_header, tree_row, unfolded};

use crate::{
    EditorOnly,
    inspect::{Selected, report_selection, send_report},
    preview::ModelPreview,
    scene_view::{EDITOR_CAMERA_ORDER, HeadlessView, SceneView},
    viewport::VIEWPORT_HANDLE,
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

impl PanelsPlugin {
    /// Whether the editor named a panel this plugin draws.
    pub(crate) fn asked() -> bool {
        std::env::var(PANELS_ENV).is_ok_and(|panels| panels.split(',').any(|p| p == "hierarchy"))
    }
}

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        // A game with egui of its own keeps its contexts as it set them up.
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
            scale: valid_scale(scale).unwrap_or(1.0),
            ..default()
        })
        .init_resource::<Listed>()
        // After the scene view's camera, which makes the picture a game
        // without a viewport draws into.
        .add_systems(PostStartup, spawn_camera)
        .add_systems(PanelsPass, draw)
        .add_systems(PostUpdate, keep_cameras_clear.before(CameraUpdateSystems));
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

/// Where the panels are and whose the pointer is.
#[derive(Resource, Default)]
pub(crate) struct Panels {
    /// The pixels a point of the editor's interface takes.
    scale: f32,
    /// The viewport pixels the Hierarchy covers from the left edge, as drawn
    /// last frame.
    left: f32,
    /// Where the pointer was last, in viewport pixels.
    pointer: Option<Vec2>,
    /// While a button is down: whether the press started on a panel.
    held: Option<bool>,
    /// The cameras this module moved off the panels.
    kept_clear: EntityHashSet,
}

impl Panels {
    fn over(&self, x: f32) -> bool {
        x < self.left
    }
}

/// Whether a panel lies over the viewport at `pointer`, in viewport pixels.
pub(crate) fn covers(world: &World, pointer: Vec2) -> bool {
    world
        .get_resource::<Panels>()
        .is_some_and(|panels| panels.over(pointer.x))
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

/// Where the panels are drawn, which is where the scene view is: the
/// editor's viewport, or the picture a game without one draws into.
fn target(headless: Option<&HeadlessView>) -> RenderTarget {
    match headless {
        Some(view) => RenderTarget::Image(view.0.clone().into()),
        None => RenderTarget::TextureView(VIEWPORT_HANDLE),
    }
}

fn shares_target(target: &RenderTarget, headless: Option<&HeadlessView>) -> bool {
    match (target, headless) {
        (RenderTarget::Image(image), Some(view)) => image.handle == view.0,
        (RenderTarget::TextureView(handle), None) => *handle == VIEWPORT_HANDLE,
        _ => false,
    }
}

fn spawn_camera(mut commands: Commands, headless: Option<Res<HeadlessView>>) {
    commands.spawn((
        Name::new("Panels camera"),
        EditorOnly,
        PanelsCamera,
        Camera2d,
        Camera {
            // After the scene view and the model preview.
            order: EDITOR_CAMERA_ORDER + 10,
            // Over what the cameras below drew.
            clear_color: ClearColorConfig::None,
            ..default()
        },
        target(headless.as_deref()),
        // Sees nothing of the scene: Bevy 0.20 draws no meshes for the scene
        // view while another active camera sees them.
        RenderLayers::none(),
        // bevy_egui draws into Rgba8UnormSrgb or, with Hdr, Rgba16Float. A
        // plain camera's picture has the viewport's format, BGRA, and the
        // game would quit on the mismatch.
        Hdr,
        Msaa::Off,
        EguiSchedule::new(PanelsPass),
    ));
}

/// One `pointer` command from the editor, in viewport pixels. True when the
/// panels took it, so the scene view and the game are not told.
pub(crate) fn pointer(world: &mut World, event: &PointerEvent) -> bool {
    let Some(mut panels) = world.get_resource_mut::<Panels>() else {
        return false;
    };
    let scale = panels.scale;
    let point = |x: f32, y: f32| egui::pos2(x / scale, y / scale);
    let mut events = Vec::new();
    let taken = match *event {
        PointerEvent::Move { x, y } => {
            panels.pointer = Some(Vec2::new(x, y));
            // Always, so a row stops looking hovered once the pointer leaves.
            events.push(egui::Event::PointerMoved(point(x, y)));
            panels.held.unwrap_or(panels.over(x))
        }
        PointerEvent::Down { x, y, button } => {
            panels.pointer = Some(Vec2::new(x, y));
            let over = panels.over(x);
            let on_panel = *panels.held.get_or_insert(over);
            if on_panel {
                events.push(egui::Event::PointerMoved(point(x, y)));
                events.push(pressed(point(x, y), button, true));
            }
            on_panel
        }
        PointerEvent::Up { x, y, button } => {
            panels.pointer = Some(Vec2::new(x, y));
            let over = panels.over(x);
            let on_panel = panels.held.take().unwrap_or(over);
            if on_panel {
                events.push(pressed(point(x, y), button, false));
            }
            on_panel
        }
        PointerEvent::Scroll { delta } => {
            let on_panel = panels.pointer.is_some_and(|at| panels.over(at.x));
            if on_panel {
                events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    // The editor's scroll is positive downwards; egui moves
                    // what is shown down on a positive one.
                    delta: egui::vec2(0.0, -delta / scale),
                    phase: egui::TouchPhase::Move,
                    modifiers: default(),
                });
            }
            on_panel
        }
    };
    let mut cameras = world.query_filtered::<Entity, With<PanelsCamera>>();
    if let Ok(context) = cameras.single(world) {
        for event in events {
            world.write_message(EguiInputEvent { context, event });
        }
    }
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

/// Draws the panels and remembers where, for [`pointer`] and the cameras.
fn draw(
    mut context: Single<&mut EguiContext, With<PanelsCamera>>,
    mut panels: ResMut<Panels>,
    listed: Res<Listed>,
    mut selected: ResMut<Selected>,
    view: Option<Res<SceneView>>,
    preview: Option<Res<ModelPreview>>,
    mut styled: Local<bool>,
) {
    let ctx = context.get_mut();
    if !std::mem::replace(&mut *styled, true) {
        besfa_ui::style(ctx);
    }
    if ctx.pixels_per_point() != panels.scale {
        ctx.set_pixels_per_point(panels.scale);
    }
    let mut root = egui::Ui::new(
        ctx.clone(),
        "panels".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    // The model preview shows one model, not the scene the Hierarchy lists.
    if preview.is_some_and(|preview| preview.showing) {
        panels.left = 0.0;
        return;
    }
    // Saving a prefab is for edit mode, as the scene is.
    let editing = view.is_some_and(|view| !view.stopped);
    let panel = egui::Panel::left("hierarchy")
        .default_size(220.0)
        .show(&mut root, |ui| hierarchy(ui, &listed, selected.0, editing));
    panels.left = panel.response.rect.right() * panels.scale;
    match panel.inner {
        // The game reports the selected entity's components to the editor.
        Some(Click::Select(entity)) => {
            selected.0 = entity;
            report_selection(entity);
        }
        Some(Click::Ask(action)) => send_report(&Report::Panel(action)),
        None => {}
    }
}

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
                    click = Some(Click::Ask(PanelAction::Spawn { kind }));
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

/// Keeps the scene view, the model preview and the game's cameras to the
/// part of the viewport the panels leave, so nothing is drawn under them.
/// A camera that set a viewport of its own keeps it, and one that stops
/// drawing into the viewport gets its whole target back: a game camera is
/// switched off again before the scene can be saved, so the file never
/// holds the editor's layout.
// ponytail: a game with split-screen viewports of its own draws under the
// panel; scale its viewports into the rest if a game needs that.
fn keep_cameras_clear(
    mut panels: ResMut<Panels>,
    headless: Option<Res<HeadlessView>>,
    mut cameras: Query<(Entity, &mut Camera, &RenderTarget), Without<PanelsCamera>>,
) {
    let left = panels.left.ceil() as u32;
    for (entity, mut camera, target) in &mut cameras {
        let ours = panels.kept_clear.contains(&entity);
        let in_viewport = camera.is_active && shares_target(target, headless.as_deref());
        let rest = camera
            .physical_target_size()
            .filter(|size| in_viewport && left > 0 && size.x > left)
            .map(|size| Viewport {
                physical_position: UVec2::new(left, 0),
                physical_size: UVec2::new(size.x - left, size.y),
                ..default()
            });
        match rest {
            Some(rest) if ours || camera.viewport.is_none() => {
                let unchanged = camera.viewport.as_ref().is_some_and(|viewport| {
                    viewport.physical_position == rest.physical_position
                        && viewport.physical_size == rest.physical_size
                });
                if !unchanged {
                    camera.viewport = Some(rest);
                }
                panels.kept_clear.insert(entity);
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

    fn panels(left: f32) -> World {
        let mut world = World::new();
        world.insert_resource(Panels {
            scale: 2.0,
            left,
            ..default()
        });
        world
    }

    #[test]
    fn a_press_belongs_to_where_it_started() {
        let mut world = panels(200.0);
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

        // Started on the panel: the scene never hears of it, even outside.
        assert!(pointer(&mut world, &down(50.0)));
        assert!(pointer(&mut world, &moved(600.0)));
        assert!(pointer(&mut world, &up(600.0)));
        // Started on the scene: a drag across the panel stays the scene's.
        assert!(!pointer(&mut world, &down(600.0)));
        assert!(!pointer(&mut world, &moved(50.0)));
        assert!(!pointer(&mut world, &up(50.0)));
        // No button down: whatever is under the pointer.
        assert!(pointer(&mut world, &moved(50.0)));
        assert!(pointer(&mut world, &PointerEvent::Scroll { delta: 30.0 }));
        assert!(!pointer(&mut world, &moved(600.0)));
        assert!(!pointer(&mut world, &PointerEvent::Scroll { delta: 30.0 }));
    }

    #[test]
    fn without_panels_the_pointer_is_the_scenes() {
        let mut world = World::new();
        assert!(!pointer(&mut world, &PointerEvent::Move { x: 1.0, y: 1.0 }));
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
