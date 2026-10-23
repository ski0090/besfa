//! Messages between the Besfa editor and a game running
//! `besfa_editor_plugin`, one JSON object per line.
//!
//! The editor writes [`Request`]s to the game's stdin. The game writes
//! [`Report`]s to its stdout, each line starting with [`REPORT_PREFIX`],
//! which tells them from its logs.
//!
//! A game keeps the plugin revision its Cargo.lock pins, so the editor often
//! talks to an older plugin: a message only ever gains fields that the other
//! side may leave out, and [`VERSION`] counts what a plugin understands.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What this revision of the protocol understands. A plugin from before
/// versions sends no [`Report::Hello`]; the editor treats that as 0.
///
/// 1. [`Report::Hello`], and requests with a `seq` answered by
///    [`Report::Done`].
/// 2. The model preview: [`Command::Preview`] and the commands after it,
///    answered by [`Report::Preview`].
/// 3. The preview's views and screenshots: [`Command::PreviewView`] and
///    [`Command::PreviewScreenshot`], answered by [`Report::Screenshot`];
///    what a previewed model is made of, [`Command::PreviewOverlays`],
///    [`Command::PreviewSelect`] for a part's components,
///    [`Command::Reimport`], answered by [`Report::Reimported`], and
///    [`Command::Thumbnail`], answered by [`Report::Thumbnail`].
/// 4. Pictures of the scene, [`Command::Screenshot`], answered by
///    [`Report::Screenshot`]; [`Placement`] for [`Command::Spawn`] and
///    [`Command::Instantiate`], and [`Command::Parent`].
/// 5. Terrain: [`SpawnKind::Terrain`], [`Command::TerrainStroke`] answered
///    by [`Report::TerrainPainted`], [`Command::TerrainRestore`],
///    [`Command::TerrainForget`], and [`Command::Heights`] answered by
///    [`Report::Heights`].
/// 6. The scene view's terrain brush, [`Command::TerrainBrush`].
/// 7. Putting things on the ground: [`Command::DropToSurface`] answered by
///    [`Report::Moved`], and [`Command::Scatter`] and
///    [`Command::ScatterErase`] answered by [`Report::Scattered`].
/// 8. The scene view's scatter brush, [`Command::ScatterBrush`].
/// 9. [`Command::TerrainFill`].
/// 10. The sky: [`SpawnKind::Sky`] and [`EntityInfo::sky`].
/// 11. Placing where the scene view points, [`Placement::pointer`] and
///     [`Command::Ghost`], and snapping handles, [`Command::Snap`].
/// 12. Markers in pictures of the scene, [`Command::Screenshot`]'s `markers`.
pub const VERSION: u32 = 12;

/// The game's cargo feature that brings in `besfa_editor_plugin`, in games
/// that depend on `besfa_components` and leave the editor out of release
/// builds.
pub const EDITOR_FEATURE: &str = "besfa_editor";

/// The features to run the game at `manifest` (its `Cargo.toml`) with in
/// the editor: Bevy's dynamic linking, which makes rebuilds much faster and
/// which a plain `cargo build` leaves out, and [`EDITOR_FEATURE`] when the
/// game declares it. A game from before it depends on the editor plugin
/// always, and cargo refuses a feature a game does not declare.
pub fn run_features(manifest: &str) -> String {
    let mut in_features = false;
    let declared = manifest.lines().map(str::trim).any(|line| {
        if line.starts_with('[') {
            in_features = line == "[features]";
            return false;
        }
        in_features
            && line
                .strip_prefix(EDITOR_FEATURE)
                .is_some_and(|rest| rest.trim_start().starts_with('='))
    });
    if declared {
        format!("bevy/dynamic_linking,{EDITOR_FEATURE}")
    } else {
        "bevy/dynamic_linking".into()
    }
}

/// Starts every report line.
pub const REPORT_PREFIX: &str = "@besfa ";

/// A command from the editor. With a `seq`, the game answers with a
/// [`Report::Done`] carrying it once the command has run.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Request {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(flatten)]
    pub command: Command,
}

/// What the editor asks of the game. Entities are the ids the game reports,
/// and components their reflected type paths, as in the scene file.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Unpauses virtual time once, so a game that pauses itself stays
    /// paused, and hands the view to the game's cameras.
    Play,
    /// Picks the entity whose components the game reports; none clears it.
    Select {
        id: Option<u64>,
    },
    /// Writes the scene file and reports whether that worked.
    Save,
    /// Replaces a component's whole value, adding the component if the
    /// entity has none. A field left out takes the type's default.
    Set {
        id: u64,
        component: String,
        value: Value,
    },
    /// Adds a component with its default value.
    Insert {
        id: u64,
        component: String,
    },
    Remove {
        id: u64,
        component: String,
    },
    /// Spawns a scene entity where `place` says, reports it and selects it.
    Spawn {
        kind: SpawnKind,
        #[serde(flatten)]
        place: Placement,
    },
    /// Copies an entity, reports the copy and selects it.
    Duplicate {
        id: u64,
    },
    /// Hides an entity and its children until `restore`.
    Delete {
        id: u64,
    },
    Restore {
        id: u64,
    },
    /// Drops a deleted entity for good, once no undo can restore it.
    Despawn {
        id: u64,
    },
    /// Spawns an entity showing the asset at `path`, relative to the asset
    /// directory, where `place` says, then reports and selects it.
    Instantiate {
        path: String,
        #[serde(flatten)]
        place: Placement,
    },
    /// Writes an entity and its scene children to `path`, relative to the
    /// asset directory, as a prefab.
    SavePrefab {
        id: u64,
        path: String,
    },
    /// Writes what a placed prefab shows, changes made inside it included,
    /// back to its file; every instance of it then shows the new file.
    ApplyPrefab {
        id: u64,
    },
    /// Moves an entity under `parent`, or to the top of the scene without
    /// one, keeping where it is in the world.
    Parent {
        id: u64,
        parent: Option<u64>,
    },
    /// The viewport's pointer: the scene view's in edit mode, the game's
    /// mouse once it plays.
    Pointer(PointerEvent),
    /// A key pressed or released in the viewport while the game plays,
    /// named the way Bevy's `KeyCode` is, with the text it typed.
    Key {
        code: String,
        pressed: bool,
        text: Option<String>,
    },
    /// The viewport lost the keyboard, so held keys are let go.
    FocusLost,
    /// Picks the scene view's handle tool.
    Tool {
        tool: Tool,
    },
    /// Draws the ground grid `grid` meters apart and, while `on`, moves
    /// dragged handles to it: moving lines up with the grid, rotating turns
    /// in steps of 15 degrees and scaling goes in steps of 0.1.
    Snap {
        grid: f32,
        on: bool,
    },
    /// Shows the asset at `path`, relative to the asset directory, see-through
    /// where [`Placement::pointer`] at `pointer` would place it, while it is
    /// dragged over the scene view. Without a path it goes.
    Ghost {
        path: Option<String>,
        #[serde(default)]
        pointer: [f32; 2],
    },
    /// Moves the scene view to the selected entity.
    Focus,
    /// Flies the scene view while the right button is held: each axis -1 to
    /// 1, faster with `fast`.
    Fly {
        right: f32,
        up: f32,
        forward: f32,
        fast: bool,
    },
    /// Renders into the editor's shared texture of this name from now on,
    /// after the editor resized the viewport.
    Viewport {
        name: String,
    },
    /// Shows the glTF model at `path`, relative to the asset directory, in
    /// the model preview instead of the scene view, loading it unless it is
    /// already the one previewed, and reports it once it has loaded. Without
    /// a path the scene view shows again and the model is dropped. Edit mode
    /// only; `play` drops the preview too.
    Preview {
        path: Option<String>,
    },
    /// Poses the previewed model with clip `clip`, in the order reported, at
    /// `time` seconds, moving on at `speed` times real time: 0 holds it. See
    /// [`clip_time`]. Without a clip the model takes the pose it loaded in.
    PreviewAnimation {
        clip: Option<usize>,
        time: f32,
        speed: f32,
        looping: bool,
    },
    /// Hides the previewed model's parts in `hidden` with everything under
    /// them; with `solo`, only that part and what is under it shows.
    PreviewParts {
        hidden: Vec<u64>,
        solo: Option<u64>,
    },
    /// Lights the preview.
    PreviewLight {
        light: LightPreset,
    },
    /// Looks at the previewed model from `view`, framed to it. Framing it
    /// again (`focus`) keeps to this view.
    PreviewView {
        view: View,
    },
    /// Saves what the preview shows to `path`, a PNG file relative to the
    /// project, and reports it as `screenshot`.
    PreviewScreenshot {
        path: String,
    },
    /// What the preview draws over the model.
    PreviewOverlays(Overlays),
    /// Picks the previewed model's part, or its root, whose components the
    /// game reports in an `entity` report as it does the selection's. The
    /// scene's selection stays.
    PreviewSelect {
        id: Option<u64>,
    },
    /// Loads the model file at `path`, relative to the asset directory,
    /// again after it changed on disk, and reports it as `reimported` once
    /// every model showing it, placed or previewed, has spawned it anew.
    Reimport {
        path: String,
    },
    /// Renders the glTF model at `path`, relative to the asset directory,
    /// on its own and writes the picture to `file`, a PNG file relative to
    /// the project; reports it as `thumbnail`. One model at a time, in the
    /// order asked.
    Thumbnail {
        path: String,
        file: String,
    },
    /// Paints one stroke on the terrain `id`: brush dabs `radius` meters wide
    /// along `points`, world X and Z, joined by straight lines. Within a
    /// stroke the brush counts where it reached most, not how often, so a
    /// line raises a ridge of an even height, and painting over a place
    /// again takes another stroke. `strength` is meters at the brush's
    /// middle to raise or lower, and for the other tools how far, 0 to 1,
    /// the ground goes toward smooth, the stroke's first point's height, or
    /// the surface `layer`. Reports `terrain_painted` with the stroke's
    /// number, which undoes and redoes it.
    TerrainStroke {
        id: u64,
        tool: TerrainTool,
        points: Vec<[f32; 2]>,
        radius: f32,
        strength: f32,
        /// The surface layer `paint` covers the ground with.
        layer: Option<usize>,
    },
    /// Paints the selected terrain with `brush` in the scene view: the left
    /// button paints instead of selecting, each press to release one
    /// stroke reported as `terrain_painted`, and a ring shows the brush on
    /// the ground. Without a brush the left button selects again.
    TerrainBrush {
        brush: Option<TerrainBrush>,
    },
    /// Brings the whole terrain `id` to `height`, meters in the world, and
    /// covers it with surface `layer`, or does one of the two; reported as
    /// a stroke, `terrain_painted`, so it undoes the same way.
    TerrainFill {
        id: u64,
        height: Option<f32>,
        layer: Option<usize>,
    },
    /// Puts the ground a stroke changed back as it was `before` it, or as
    /// the stroke left it.
    TerrainRestore {
        stroke: u64,
        before: bool,
    },
    /// Drops what the game keeps to undo a stroke, once nothing will.
    TerrainForget {
        stroke: u64,
    },
    /// Reports `heights`: at each of `points`, world X and Z, the height of
    /// the highest terrain there, or none where no terrain is.
    Heights {
        points: Vec<[f32; 2]>,
    },
    /// Brings each of `ids` down, or up out of the ground, onto the surface
    /// right below where it stands, keeping which way it faces. A group,
    /// an entity with no mesh or asset of its own over scene entities,
    /// drops each thing in it instead. Reports `moved`; one with no surface
    /// below stays and fails the command.
    DropToSurface {
        ids: Vec<u64>,
    },
    /// Places up to `count` of `models` within `radius` meters of `center`,
    /// world X and Z, on the ground: on terrain where there is one, else
    /// on what is below, else at height 0. Each is at least `spacing`
    /// meters from the others and from what was scattered before, goes
    /// under the scene's Scatter group, and is marked scattered. The same
    /// `seed` on the same ground among the same scattered things places
    /// the same. Reports `scattered`.
    Scatter {
        models: Vec<ScatterModel>,
        center: [f32; 2],
        radius: f32,
        count: u32,
        #[serde(default)]
        spacing: f32,
        /// The smallest and largest size, the same along each axis.
        #[serde(default = "unit_scale")]
        scale: [f32; 2],
        /// Turned a random way about the up axis.
        #[serde(default)]
        rotate: bool,
        /// Tilted to the ground's slope.
        #[serde(default)]
        align: bool,
        #[serde(default)]
        seed: u64,
    },
    /// Scatters with `brush` in the scene view: the left button places, or
    /// erases, where it presses and drags instead of selecting, each press
    /// to release reported as one `scattered`. Without a brush the left
    /// button selects again.
    ScatterBrush {
        brush: Option<ScatterBrush>,
    },
    /// Deletes the scattered things within `radius` meters of `center`,
    /// world X and Z; things placed by hand stay. Reports `scattered`.
    ScatterErase {
        center: [f32; 2],
        radius: f32,
    },
    /// Renders the scene as the scene view shows it, without its grid and
    /// handles, off screen, and writes the picture to `file`, a PNG file
    /// relative to the project; reports it as `screenshot`. Looks from `eye`
    /// at `target`, or with `top`, straight down on `target` over a square
    /// `top` meters wide, north (-Z) up. One picture at a time, in the order
    /// asked, once the scene's models have loaded.
    Screenshot {
        file: String,
        target: [f32; 3],
        eye: Option<[f32; 3]>,
        top: Option<f32>,
        /// Pixels; 1024 wide by default, and 768 high, or as high as wide
        /// with `top`.
        width: Option<u32>,
        height: Option<u32>,
        /// Also the icons and rings the scene view draws for what has no
        /// mesh to show it, as spawn points; since protocol 12.
        markers: Option<bool>,
    },
}

/// Where a new entity goes and what it is called. What is left out stays as
/// the entity is made: at the origin, unturned, its own size and name, at
/// the top of the scene.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Placement {
    /// Where in the world, in meters, also under a parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation: Option<[f32; 3]>,
    /// Without `translation`, where the scene view shows it, in viewport
    /// pixels: on the surface seen there, or on the ground at height 0,
    /// standing upright either way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pointer: Option<[f32; 2]>,
    /// Turned about the up axis, in degrees, counterclockwise seen from
    /// above: 90 turns its front, -Z, to -X.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yaw: Option<f32>,
    /// Size, the same along each axis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The scene entity it goes under, keeping `translation` and `yaw` in
    /// the world.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<u64>,
}

/// What the model preview can draw over the model, over everything else.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Overlays {
    /// A line from each joint to its parent joint.
    pub skeleton: bool,
    /// Each marker's axes.
    pub markers: bool,
    /// The model's bounding box.
    pub bounds: bool,
}

impl Command {
    /// Sent many times a second while the pointer moves or keys are held,
    /// so not worth an answer each.
    pub fn is_input(&self) -> bool {
        matches!(
            self,
            Command::Pointer(_)
                | Command::Key { .. }
                | Command::FocusLost
                | Command::Fly { .. }
                | Command::Ghost { .. }
                // Sent while the time slider is dragged.
                | Command::PreviewAnimation { .. }
        )
    }
}

/// What the Hierarchy's add menu creates.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpawnKind {
    Empty,
    Cube,
    Sphere,
    Plane,
    Light,
    Camera,
    /// Flat ground 256 meters a side covered by its first surface layer,
    /// whose heights and layers go to a file of their own.
    Terrain,
    /// The scene's sky, a gradient to start with.
    Sky,
}

/// A model or prefab to scatter, relative to the asset directory, and how
/// often it comes up against the others.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ScatterModel {
    pub path: String,
    #[serde(default = "one")]
    pub weight: f32,
}

fn one() -> f32 {
    1.0
}

fn unit_scale() -> [f32; 2] {
    [1.0, 1.0]
}

/// The scene view's scatter brush; its fields mean what they do in
/// [`Command::Scatter`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ScatterBrush {
    pub models: Vec<ScatterModel>,
    pub radius: f32,
    /// Things per 100 square meters under the brush; where some are
    /// already, it adds only what is missing.
    pub density: f32,
    pub spacing: f32,
    pub scale: [f32; 2],
    pub rotate: bool,
    pub align: bool,
    /// Deletes what was scattered under the brush instead.
    pub erase: bool,
}

/// An entity's Transform before and after a change the game made, in the
/// shape `set` takes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Move {
    pub id: u64,
    pub before: Value,
    pub after: Value,
}

/// The scene view's terrain brush; its fields mean what they do in
/// [`Command::TerrainStroke`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct TerrainBrush {
    pub tool: TerrainTool,
    pub radius: f32,
    pub strength: f32,
    pub layer: Option<usize>,
}

/// What a terrain stroke does to the ground under the brush.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerrainTool {
    Raise,
    Lower,
    /// Rounds off bumps and dents.
    Smooth,
    /// Brings the ground to the height where the stroke began.
    Flatten,
    /// Covers the ground with a surface layer.
    Paint,
}

/// What dragging a handle does to the selected entity.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    #[default]
    Translate,
    Rotate,
    Scale,
}

/// How the model preview is lit.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LightPreset {
    /// A key light from the front, above and to the side, with soft fill.
    #[default]
    Studio,
    /// A high, warm sun and bright sky.
    Daylight,
    /// Dim, cool moonlight from behind, which shows outlines and rims.
    Night,
}

/// Where the preview looks at the model from. Models face Bevy's forward,
/// -Z, so the front view looks at the model from -Z, and the model's right
/// is +X.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum View {
    Front,
    Back,
    Left,
    Right,
    Top,
    /// From the front, above and to the model's right.
    #[default]
    ThreeQuarter,
}

/// Where a previewed clip `duration` seconds long is `seconds` of real time
/// after `time`, played at `speed`: wrapped around when `looping`, held at
/// either end otherwise. The editor and the game both move the clock this
/// way, so neither has to report it.
pub fn clip_time(time: f32, seconds: f32, speed: f32, duration: f32, looping: bool) -> f32 {
    if duration <= 0.0 {
        return 0.0;
    }
    let time = time + seconds * speed;
    if looping {
        time.rem_euclid(duration)
    } else {
        time.clamp(0.0, duration)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    Left,
    Middle,
    Right,
}

/// One pointer event from the viewport, in viewport pixels. Scrolling is in
/// pixels too: positive scrolls down, which zooms out.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PointerEvent {
    Down { x: f32, y: f32, button: Button },
    Move { x: f32, y: f32 },
    Up { x: f32, y: f32, button: Button },
    Scroll { delta: f32 },
}

/// What the game tells the editor.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Report {
    /// The first line, with the protocol [`VERSION`] the plugin speaks.
    Hello { version: u32 },
    /// The scene's entities, whenever they change.
    Entities { entities: Vec<EntityInfo> },
    /// The selected entity's components, whenever they change.
    Entity {
        id: u64,
        /// The `set`, `insert` and `remove` commands taken so far, applied
        /// or not, so the editor can tell a report written before its latest
        /// change. Absent from early plugins.
        #[serde(default)]
        changes: Option<u64>,
        components: Vec<ComponentData>,
    },
    /// Every system, once, with the game's crate to put its own first.
    Systems {
        #[serde(default, rename = "crate")]
        game_crate: Option<String>,
        systems: Vec<SystemInfo>,
    },
    /// The components the editor can add, once.
    Components { components: Vec<ComponentType> },
    /// An entity the editor asked for exists now.
    Spawned { id: u64 },
    /// The game picked an entity itself, or none once the selected one is
    /// gone.
    Selected { id: Option<u64> },
    /// The outcome of `save`.
    Saved { error: Option<String> },
    /// The outcome of `save_prefab` and `apply_prefab`.
    PrefabSaved {
        path: Option<String>,
        error: Option<String>,
    },
    /// A handle dragged in the scene view changed a component.
    Edited {
        id: u64,
        component: String,
        before: Value,
        after: Value,
    },
    /// The request with this `seq` has run; it failed if there is an error.
    Done { seq: u64, error: Option<String> },
    /// The model `preview` asked for, once it has loaded and every time it is
    /// shown again, or why it could not load.
    Preview {
        path: String,
        #[serde(flatten)]
        model: PreviewModel,
    },
    /// The outcome of `preview_screenshot` and `screenshot`.
    Screenshot { path: String, error: Option<String> },
    /// The outcome of `thumbnail`.
    Thumbnail { path: String, error: Option<String> },
    /// A terrain stroke is done, from a command or the scene view's brush:
    /// `stroke` names it for `terrain_restore` and `terrain_forget`.
    TerrainPainted { id: u64, stroke: u64 },
    /// What `drop_to_surface` moved.
    Moved { moves: Vec<Move> },
    /// What `scatter` and `scatter_erase` placed and deleted.
    Scattered { added: Vec<u64>, removed: Vec<u64> },
    /// The answer to `heights`, a height or none for each point.
    Heights { heights: Vec<Option<f32>> },
    /// The outcome of `reimport`.
    Reimported { path: String, error: Option<String> },
    /// A report from a newer plugin than this protocol knows.
    #[serde(other)]
    Unknown,
}

impl Report {
    /// The report in `line`, or None for a log line.
    pub fn parse(line: &str) -> Option<Self> {
        serde_json::from_str(line.strip_prefix(REPORT_PREFIX)?).ok()
    }

    /// The report as one stdout line.
    pub fn line(&self) -> String {
        format!("{REPORT_PREFIX}{}", serde_json::to_string(self).unwrap())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EntityInfo {
    /// Bevy's entity bits; what commands take.
    pub id: u64,
    pub name: Option<String>,
    pub parent: Option<u64>,
    /// From the scene file or the editor, so saving keeps it. Entities the
    /// game's code spawns while it runs are not saved.
    #[serde(default = "yes")]
    pub scene: bool,
    /// The model, scene or prefab file a placed entity shows, which the game
    /// spawns under it; saving keeps only this path.
    pub asset: Option<String>,
    /// Carries the scene's sky.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sky: bool,
}

impl EntityInfo {
    /// Unnamed entities are shown the way Bevy prints them: `4v0`, the index
    /// (stored inverted in the low bits) and the generation.
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) => name.clone(),
            None => format!("Entity {}v{}", !self.id & 0xFFFF_FFFF, self.id >> 32),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ComponentData {
    pub name: String,
    /// The reflected type path, which commands name the component by.
    pub path: String,
    pub mutable: bool,
    /// The component on the same entity that required this one, if any.
    pub required_by: Option<String>,
    /// False for components computed from others, which the scene file
    /// leaves out.
    #[serde(default = "yes")]
    pub saved: bool,
    /// The reflected value; None for types without `Reflect`.
    #[serde(default)]
    pub value: Option<Value>,
    /// What the editor needs of the value's shape beyond its JSON.
    #[serde(flatten)]
    pub shapes: Shapes,
}

/// What a component value's type says of its fields, each by its path in
/// the value: keys joined by `/`, list items as `*`, the whole value as ``.
/// Absent from early plugins.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Shapes {
    /// Enums of plain variants, with the variants to choose from.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub choices: std::collections::BTreeMap<String, Vec<String>>,
    /// Options, with the value one that is None starts as when given one.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub options: std::collections::BTreeMap<String, Value>,
    /// Lists, with the value a new item starts as.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub items: std::collections::BTreeMap<String, Value>,
}

impl ComponentData {
    pub fn crate_name(&self) -> &str {
        crate_of(&self.path)
    }

    /// The editor can change it: reflected, mutable, and not computed.
    pub fn editable(&self) -> bool {
        self.saved && self.mutable && self.value.is_some()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SystemInfo {
    pub schedule: String,
    pub name: String,
}

/// What the game found in a previewed model.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct PreviewModel {
    /// The entity the model spawned under, which takes components like a
    /// placed model's root does.
    pub root: Option<u64>,
    pub clips: Vec<ClipInfo>,
    /// The model's nodes and meshes; the root's children have no parent.
    pub parts: Vec<EntityInfo>,
    /// Width, height and depth of its bounds, in meters.
    pub size: Option<[f32; 3]>,
    pub error: Option<String>,
    /// What it is made of; absent from plugins before protocol 3.
    pub stats: Option<ModelStats>,
    /// The parts that are skin joints.
    pub joints: Vec<u64>,
    /// The parts that mark a place, like a grip or a muzzle: leaves without
    /// a mesh that are not joints, lights or cameras.
    pub markers: Vec<Marker>,
}

/// What a previewed model is made of.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModelStats {
    pub triangles: u64,
    pub vertices: u64,
    /// Mesh primitives: a glTF mesh has one per material.
    pub meshes: u32,
    pub materials: Vec<MaterialInfo>,
}

/// A material of a previewed model.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MaterialInfo {
    pub name: Option<String>,
    /// The base color, sRGB with alpha, each 0 to 1.
    pub color: [f32; 4],
    /// Which textures it has: `base_color`, `normal`, `metallic_roughness`,
    /// `emissive`, `occlusion`.
    pub textures: Vec<String>,
    /// How many of the model's meshes use it.
    pub meshes: u32,
}

/// A part of a previewed model that marks a place.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Marker {
    pub id: u64,
    /// Where it is in the model's space, in meters, as the model loaded.
    pub position: [f32; 3],
}

/// An animation clip of a previewed model.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClipInfo {
    pub name: String,
    /// In seconds.
    pub duration: f32,
}

/// A component type the editor can add to an entity.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ComponentType {
    pub name: String,
    pub path: String,
}

fn yes() -> bool {
    true
}

/// The crate a Rust type path starts with: `bevy_transform::...::Transform`
/// is in `bevy_transform`. Trait method names open with `<`, as in
/// `<bevy_a::B as C>::run`.
pub fn crate_of(path: &str) -> &str {
    let path = path.strip_prefix('<').unwrap_or(path);
    path.split("::").next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn request(line: &str) -> Request {
        serde_json::from_str(line).unwrap()
    }

    #[test]
    fn reads_requests_with_and_without_a_seq() {
        assert_eq!(
            request(
                r#"{"seq":3,"command":"set","id":4,"component":"a::B<c::D, e::F>","value":{"x":1}}"#
            ),
            Request {
                seq: Some(3),
                command: Command::Set {
                    id: 4,
                    component: "a::B<c::D, e::F>".into(),
                    value: json!({"x": 1}),
                },
            }
        );
        // What editors from before versions send.
        assert_eq!(request(r#"{"command":"save"}"#).seq, None);
        assert_eq!(
            request(r#"{"command":"pointer","event":"scroll","delta":-120}"#).command,
            Command::Pointer(PointerEvent::Scroll { delta: -120.0 })
        );
        assert_eq!(
            request(r#"{"command":"select"}"#).command,
            Command::Select { id: None }
        );
        assert!(serde_json::from_str::<Request>("play").is_err());
        assert_eq!(
            request(r#"{"command":"preview_light","light":"night"}"#).command,
            Command::PreviewLight {
                light: LightPreset::Night
            }
        );
        assert_eq!(
            request(r#"{"seq":1,"command":"preview"}"#).command,
            Command::Preview { path: None }
        );
        assert_eq!(
            request(r#"{"command":"preview_view","view":"three_quarter"}"#).command,
            Command::PreviewView {
                view: View::ThreeQuarter
            }
        );
        assert_eq!(
            request(
                r#"{"command":"preview_overlays","skeleton":true,"markers":false,"bounds":true}"#
            )
            .command,
            Command::PreviewOverlays(Overlays {
                skeleton: true,
                markers: false,
                bounds: true
            })
        );
        // An entity made where it is asked for; editors before version 4
        // send no placement.
        assert_eq!(
            request(r#"{"command":"instantiate","path":"a.glb"}"#).command,
            Command::Instantiate {
                path: "a.glb".into(),
                place: Placement::default(),
            }
        );
        let placed = Command::Instantiate {
            path: "a.glb".into(),
            place: Placement {
                translation: Some([1.0, 0.0, -2.0]),
                yaw: Some(90.0),
                scale: Some(1.5),
                name: Some("Oak".into()),
                parent: Some(7),
                ..Placement::default()
            },
        };
        assert_eq!(
            request(
                r#"{"command":"instantiate","path":"a.glb","translation":[1,0,-2],"yaw":90,"scale":1.5,"name":"Oak","parent":7}"#
            )
            .command,
            placed
        );
        assert_eq!(
            request(r#"{"command":"spawn","kind":"cube","pointer":[640,360]}"#).command,
            Command::Spawn {
                kind: SpawnKind::Cube,
                place: Placement {
                    pointer: Some([640.0, 360.0]),
                    ..Placement::default()
                },
            }
        );
        assert_eq!(
            request(r#"{"command":"ghost","path":"a.glb","pointer":[3,4]}"#).command,
            Command::Ghost {
                path: Some("a.glb".into()),
                pointer: [3.0, 4.0],
            }
        );
        assert_eq!(
            request(r#"{"command":"ghost"}"#).command,
            Command::Ghost {
                path: None,
                pointer: [0.0, 0.0],
            }
        );
        assert_eq!(
            request(r#"{"command":"snap","grid":0.5,"on":true}"#).command,
            Command::Snap {
                grid: 0.5,
                on: true
            }
        );
        let line = serde_json::to_string(&Request {
            seq: None,
            command: Command::Spawn {
                kind: SpawnKind::Empty,
                place: Placement::default(),
            },
        })
        .unwrap();
        assert_eq!(
            line, r#"{"command":"spawn","kind":"empty"}"#,
            "nothing to place"
        );
        assert_eq!(
            request(
                r#"{"command":"terrain_stroke","id":2,"tool":"raise","points":[[1,2],[3,4]],"radius":5,"strength":2}"#
            )
            .command,
            Command::TerrainStroke {
                id: 2,
                tool: TerrainTool::Raise,
                points: vec![[1.0, 2.0], [3.0, 4.0]],
                radius: 5.0,
                strength: 2.0,
                layer: None,
            }
        );
        assert_eq!(
            request(
                r#"{"command":"scatter","models":[{"path":"a.glb"}],"center":[1,2],"radius":5,"count":3}"#
            )
            .command,
            Command::Scatter {
                models: vec![ScatterModel {
                    path: "a.glb".into(),
                    weight: 1.0,
                }],
                center: [1.0, 2.0],
                radius: 5.0,
                count: 3,
                spacing: 0.0,
                scale: [1.0, 1.0],
                rotate: false,
                align: false,
                seed: 0,
            }
        );
        assert_eq!(
            request(r#"{"command":"terrain_brush","brush":null}"#).command,
            Command::TerrainBrush { brush: None }
        );
        assert_eq!(
            request(r#"{"command":"spawn","kind":"terrain"}"#).command,
            Command::Spawn {
                kind: SpawnKind::Terrain,
                place: Placement::default(),
            }
        );
        assert_eq!(
            request(r#"{"command":"parent","id":3}"#).command,
            Command::Parent {
                id: 3,
                parent: None
            }
        );
        assert_eq!(
            request(r#"{"command":"screenshot","file":"top.png","target":[0,0,0],"top":200}"#)
                .command,
            Command::Screenshot {
                file: "top.png".into(),
                target: [0.0; 3],
                eye: None,
                top: Some(200.0),
                width: None,
                height: None,
                markers: None,
            }
        );
    }

    #[test]
    fn previewed_clips_wrap_when_looping_and_hold_otherwise() {
        assert_eq!(clip_time(0.5, 1.0, 1.0, 2.0, true), 1.5);
        assert_eq!(clip_time(1.5, 1.0, 1.0, 2.0, true), 0.5);
        assert_eq!(clip_time(0.5, 1.0, -1.0, 2.0, true), 1.5, "backwards");
        assert_eq!(clip_time(1.5, 1.0, 1.0, 2.0, false), 2.0);
        assert_eq!(clip_time(1.5, 9.0, 0.0, 2.0, false), 1.5, "held");
        assert_eq!(clip_time(1.0, 1.0, 1.0, 0.0, true), 0.0, "a pose");
    }

    #[test]
    fn writes_input_exactly_as_before_versions() {
        let line = |command| serde_json::to_string(&Request { seq: None, command }).unwrap();
        assert_eq!(line(Command::Play), r#"{"command":"play"}"#);
        assert_eq!(
            line(Command::Pointer(PointerEvent::Down {
                x: 1.0,
                y: 2.0,
                button: Button::Left
            })),
            r#"{"command":"pointer","event":"down","x":1.0,"y":2.0,"button":"left"}"#
        );
        let save = Request {
            seq: Some(7),
            command: Command::Save,
        };
        assert_eq!(
            serde_json::to_string(&save).unwrap(),
            r#"{"seq":7,"command":"save"}"#
        );
        assert!(
            Command::Fly {
                right: 0.0,
                up: 0.0,
                forward: 1.0,
                fast: false
            }
            .is_input()
        );
        assert!(!Command::Save.is_input());
    }

    #[test]
    fn reads_reports_old_and_new() {
        // An early plugin: no change count, and key order of its own.
        let report = Report::parse(
            r#"@besfa {"components":[{"name":"A","path":"a::A","mutable":true}],"id":1,"type":"entity"}"#,
        );
        let Some(Report::Entity {
            changes: None,
            components,
            ..
        }) = report
        else {
            panic!("{report:?}");
        };
        assert!(components[0].saved, "saved by default");
        assert_eq!(components[0].value, None);
        assert_eq!(
            Report::parse(r#"@besfa {"type":"from_the_future","x":1}"#),
            Some(Report::Unknown)
        );
        assert_eq!(Report::parse("INFO my_game: Playing."), None);
        assert_eq!(Report::parse("@besfa not json"), None);

        let done = Report::Done {
            seq: 2,
            error: Some("no".into()),
        };
        assert_eq!(
            done.line(),
            r#"@besfa {"type":"done","seq":2,"error":"no"}"#
        );
        assert_eq!(Report::parse(&done.line()), Some(done));
        let preview = Report::Preview {
            path: "models/hero.glb".into(),
            model: PreviewModel {
                clips: vec![ClipInfo {
                    name: "Walk".into(),
                    duration: 1.25,
                }],
                size: Some([0.5, 1.8, 0.3]),
                stats: Some(ModelStats {
                    triangles: 12,
                    ..ModelStats::default()
                }),
                markers: vec![Marker {
                    id: 7,
                    position: [0.0, 1.0, -0.5],
                }],
                ..PreviewModel::default()
            },
        };
        assert!(
            preview.line().contains(r#""clips":[{"name":"Walk""#),
            "flat"
        );
        assert_eq!(Report::parse(&preview.line()), Some(preview));
        // What a protocol 2 plugin sends.
        assert_eq!(
            Report::parse(r#"@besfa {"type":"preview","path":"a.glb","error":"no file"}"#),
            Some(Report::Preview {
                path: "a.glb".into(),
                model: PreviewModel {
                    error: Some("no file".into()),
                    ..PreviewModel::default()
                },
            })
        );
        let systems = Report::Systems {
            game_crate: Some("game".into()),
            systems: vec![],
        };
        assert!(systems.line().contains(r#""crate":"game""#));
    }

    #[test]
    fn runs_the_editor_feature_only_where_the_game_declares_it() {
        let old = "[dependencies]\nbesfa_editor_plugin = { git = \"x\" }\n";
        assert_eq!(run_features(old), "bevy/dynamic_linking");
        let new = "[dependencies]\nbesfa_components = { git = \"x\" }\n\n[features]\nbesfa_editor = [\"dep:besfa_editor_plugin\"]\n";
        assert_eq!(run_features(new), "bevy/dynamic_linking,besfa_editor");
        let elsewhere = "[package.metadata]\nbesfa_editor = 1\n";
        assert_eq!(run_features(elsewhere), "bevy/dynamic_linking");
    }

    #[test]
    fn labels_entities_and_names_crates() {
        let entity = |name: Option<&str>| EntityInfo {
            id: 4294967294,
            name: name.map(Into::into),
            parent: None,
            scene: true,
            asset: None,
            sky: false,
        };
        assert_eq!(entity(Some("Cube")).label(), "Cube");
        assert_eq!(entity(None).label(), "Entity 1v0");
        assert_eq!(
            crate_of("bevy_transform::components::transform::Transform"),
            "bevy_transform"
        );
        assert_eq!(crate_of("<bevy_a::B as C>::run"), "bevy_a");
    }
}
