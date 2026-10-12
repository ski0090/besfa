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
pub const VERSION: u32 = 1;

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
}

impl Command {
    /// Sent many times a second while the pointer moves or keys are held,
    /// so not worth an answer each.
    pub fn is_input(&self) -> bool {
        matches!(
            self,
            Command::Pointer(_) | Command::Key { .. } | Command::FocusLost | Command::Fly { .. }
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
