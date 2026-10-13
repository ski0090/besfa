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
pub const VERSION: u32 = 3;

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
    /// Spawns a scene entity, reports it and selects it.
    Spawn {
        kind: SpawnKind,
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
    /// directory, then reports and selects it.
    Instantiate {
        path: String,
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
    /// The outcome of `preview_screenshot`.
    Screenshot { path: String, error: Option<String> },
    /// The outcome of `thumbnail`.
    Thumbnail { path: String, error: Option<String> },
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
    fn labels_entities_and_names_crates() {
        let entity = |name: Option<&str>| EntityInfo {
            id: 4294967294,
            name: name.map(Into::into),
            parent: None,
            scene: true,
            asset: None,
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
