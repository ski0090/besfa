//! What the running game reports about its world, read from its output, and
//! the changes the editor made to it, for undo.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde_json::Value;

/// A game output line that is a report rather than a log starts with this;
/// a JSON object with a `type` follows. Written by `besfa_editor_plugin`.
const REPORT_PREFIX: &str = "@besfa ";

/// The crate a Rust type path starts with: `bevy_transform::...::Transform`
/// is in `bevy_transform`. Trait method names open with `<`, as in
/// `<bevy_a::B as C>::run`.
pub fn crate_of(path: &str) -> &str {
    let path = path.strip_prefix('<').unwrap_or(path);
    path.split("::").next().unwrap_or(path)
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct SceneEntity {
    /// Bevy's entity bits; what `select` takes.
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

impl SceneEntity {
    /// Unnamed entities are shown the way Bevy prints them: `4v0`, the index
    /// (stored inverted in the low bits) and the generation.
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) => name.clone(),
            None => format!("Entity {}v{}", !self.id & 0xFFFF_FFFF, self.id >> 32),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct EntityComponent {
    pub name: String,
    /// The reflected type path, which commands name the component by.
    pub path: String,
    pub mutable: bool,
    /// The component on the same entity that required this one, if any.
    pub required_by: Option<String>,
    /// False for components computed from others, which the scene file
    /// leaves out and the editor shows read-only.
    #[serde(default = "yes")]
    pub saved: bool,
    /// The reflected value; None for types without `Reflect`.
    #[serde(default)]
    pub value: Option<Value>,
}

impl EntityComponent {
    pub fn crate_name(&self) -> &str {
        crate_of(&self.path)
    }

    /// The editor can change it: reflected, mutable, and not computed.
    pub fn editable(&self) -> bool {
        self.saved && self.mutable && self.value.is_some()
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct GameSystem {
    pub schedule: String,
    pub name: String,
}

/// A component type the editor can add to an entity.
#[derive(Deserialize, Clone, Debug)]
pub struct ComponentType {
    pub name: String,
    pub path: String,
}

/// What the running game reports about its world, and whether the editor
/// changed it since the last save.
#[derive(Default)]
pub struct Scene {
    pub entities: Vec<SceneEntity>,
    /// The entity whose components are shown; the game reports them.
    pub selected: Option<u64>,
    pub components: Vec<EntityComponent>,
    pub systems: Vec<GameSystem>,
    /// Components the game can add with a default value.
    pub addable: Vec<ComponentType>,
    /// The game's own crate, listed before Bevy's.
    pub crate_name: Option<String>,
    /// The editor changed the scene since it was loaded or last saved.
    pub dirty: bool,
    /// The `set`, `insert` and `remove` commands sent to this game. Its
    /// `entity` reports count the ones it took, so one written before the
    /// latest change is skipped instead of taking that change back on screen.
    changes: u64,
}

impl Scene {
    pub fn selected_entity(&self) -> Option<&SceneEntity> {
        self.entities
            .iter()
            .find(|entity| Some(entity.id) == self.selected)
    }

    /// The placed file `entity` is part of: the entity itself or its nearest
    /// ancestor that shows one.
    pub fn placement<'a>(&'a self, entity: &'a SceneEntity) -> Option<&'a SceneEntity> {
        let by_id: HashMap<u64, &SceneEntity> = self.entities.iter().map(|e| (e.id, e)).collect();
        let mut current = Some(entity);
        // Bounded, so a parent cycle, a game bug, cannot hang the editor.
        for _ in 0..=self.entities.len() {
            let entity = current?;
            if entity.asset.is_some() {
                return Some(entity);
            }
            current = entity.parent.and_then(|parent| by_id.get(&parent).copied());
        }
        None
    }

    /// Takes `line` if it is a report and returns it, for what the page does
    /// about it. Logs, and lines that only look like a report, are left for
    /// the log panel.
    pub fn handle(&mut self, line: &str) -> Option<Value> {
        let report: Value = serde_json::from_str(line.strip_prefix(REPORT_PREFIX)?).ok()?;
        // Game output is not trusted to be well formed.
        self.apply(&report).ok()?;
        Some(report)
    }

    fn apply(&mut self, report: &Value) -> serde_json::Result<()> {
        match report["type"].as_str() {
            Some("entities") => self.entities = Deserialize::deserialize(&report["entities"])?,
            Some("entity") => {
                // A report for an earlier selection, or from before the
                // latest change, may still be in the pipe. Older plugins send
                // no count.
                let changes = report["changes"].as_u64().unwrap_or(self.changes);
                if report["id"].as_u64() == self.selected && changes >= self.changes {
                    self.components = Deserialize::deserialize(&report["components"])?;
                }
            }
            Some("systems") => {
                self.systems = Deserialize::deserialize(&report["systems"])?;
                self.crate_name = report["crate"].as_str().map(Into::into);
            }
            Some("components") => self.addable = Deserialize::deserialize(&report["components"])?,
            // The game picked an entity itself: a new one, or none once the
            // selected one is gone.
            Some("selected") => self.select(report["id"].as_u64()),
            Some("saved") if report["error"].is_null() => self.dirty = false,
            // A drag in the scene view changed a component.
            Some("edited") => self.dirty = true,
            _ => {}
        }
        Ok(())
    }

    pub fn select(&mut self, id: Option<u64>) {
        if id != self.selected {
            self.selected = id;
            self.components.clear();
        }
    }

    /// Notes a `command` sent to the game. A `set` on the selected entity
    /// shows its value at once, so a change made before the game reports
    /// back starts from it, and so does the change's undo.
    pub fn sent(&mut self, command: &Value) {
        let kind = command["command"].as_str();
        if !matches!(kind, Some("set" | "insert" | "remove")) {
            return;
        }
        self.changes += 1;
        if kind == Some("set") && command["id"].as_u64() == self.selected {
            let value = Some(command["value"].clone()).filter(|value| !value.is_null());
            for component in &mut self.components {
                if command["component"] == component.path.as_str() {
                    component.value = value.clone();
                }
            }
        }
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// A new game process: what the old one reported no longer holds, and it
    /// starts from the scene file. The selection stays, so the same entity is
    /// picked up after a Stop.
    pub fn reset(&mut self) {
        *self = Self {
            selected: self.selected,
            ..Self::default()
        };
    }
}

/// Entities in tree order with their depth. An entity whose parent is not
/// reported is a root.
pub fn tree_rows(entities: &[SceneEntity]) -> Vec<(&SceneEntity, usize)> {
    let ids: HashSet<u64> = entities.iter().map(|entity| entity.id).collect();
    let mut children: HashMap<Option<u64>, Vec<&SceneEntity>> = HashMap::new();
    for entity in entities {
        let parent = entity.parent.filter(|parent| ids.contains(parent));
        children.entry(parent).or_default().push(entity);
    }
    let mut rows = Vec::new();
    // A parent cycle would be a game bug; this keeps it from hanging the editor.
    let mut seen = HashSet::new();
    let mut stack: Vec<(&SceneEntity, usize)> = children
        .get(&None)
        .into_iter()
        .flatten()
        .rev()
        .map(|entity| (*entity, 0))
        .collect();
    while let Some((entity, depth)) = stack.pop() {
        if seen.insert(entity.id) {
            rows.push((entity, depth));
            let below = children.get(&Some(entity.id)).into_iter().flatten().rev();
            stack.extend(below.map(|child| (*child, depth + 1)));
        }
    }
    rows
}

/// A change to the scene as the game commands that make and unmake it.
pub struct Edit {
    pub undo: Vec<Value>,
    pub redo: Vec<Value>,
}

/// The scene changes made in this edit session, for undo and redo. The
/// commands name entities by id, which stays valid for the session: the
/// game hides deleted entities instead of despawning them.
#[derive(Default)]
pub struct EditHistory {
    done: Vec<Edit>,
    undone: Vec<Edit>,
}

impl EditHistory {
    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Records `edit`, which forgets what was undone; returns the forgotten
    /// edits.
    pub fn add(&mut self, edit: Edit) -> Vec<Edit> {
        self.done.push(edit);
        std::mem::take(&mut self.undone)
    }

    /// The commands that undo the last change, if any.
    pub fn undo(&mut self) -> Option<Vec<Value>> {
        let edit = self.done.pop()?;
        let commands = edit.undo.clone();
        self.undone.push(edit);
        Some(commands)
    }

    /// The commands that redo the last undone change, if any.
    pub fn redo(&mut self) -> Option<Vec<Value>> {
        let edit = self.undone.pop()?;
        let commands = edit.redo.clone();
        self.done.push(edit);
        Some(commands)
    }

    /// A new game process: the ids belong to the old one.
    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_the_entity_component_and_system_reports() {
        let mut scene = Scene::default();
        assert!(
            scene
                .handle(
                    r#"@besfa {"type":"entities","entities":[{"id":4294967295,"name":"Cube","parent":null},{"id":4294967294,"name":null,"parent":4294967295}]}"#
                )
                .is_some()
        );
        let labels: Vec<_> = scene.entities.iter().map(SceneEntity::label).collect();
        assert_eq!(labels, ["Cube", "Entity 1v0"]);
        assert_eq!(scene.entities[1].parent, Some(4294967295));

        scene.select(Some(4294967295));
        scene.handle(
            r#"@besfa {"type":"entity","id":4294967295,"components":[{"name":"Transform","path":"bevy_transform::components::transform::Transform","mutable":true,"required_by":null,"value":{"translation":[1.0,2.0,3.0]}},{"name":"Spin","path":"my_game::Spin","mutable":true,"required_by":null,"value":null}]}"#,
        ).unwrap();
        let crates: Vec<_> = scene
            .components
            .iter()
            .map(EntityComponent::crate_name)
            .collect();
        assert_eq!(crates, ["bevy_transform", "my_game"]);
        assert_eq!(
            scene.components[0].value,
            Some(json!({"translation": [1.0, 2.0, 3.0]}))
        );
        assert_eq!(scene.components[1].value, None);

        scene.handle(
            r#"@besfa {"type":"systems","crate":"my_game","systems":[{"schedule":"Update","name":"my_game::spin"},{"schedule":"PostUpdate","name":"<bevy_a::B as C>::run"}]}"#,
        ).unwrap();
        assert_eq!(scene.crate_name.as_deref(), Some("my_game"));
        let crates: Vec<_> = scene.systems.iter().map(|s| crate_of(&s.name)).collect();
        assert_eq!(crates, ["my_game", "bevy_a"]);
    }

    #[test]
    fn ignores_reports_for_an_entity_that_is_no_longer_selected() {
        let mut scene = Scene::default();
        scene.select(Some(1));
        scene.handle(r#"@besfa {"type":"entity","id":2,"components":[]}"#);
        assert!(scene.components.is_empty());
        scene.handle(
            r#"@besfa {"type":"entity","id":1,"components":[{"name":"A","path":"a::A","mutable":true}]}"#,
        );
        assert_eq!(scene.components[0].name, "A");
        scene.select(None);
        assert!(scene.components.is_empty());
    }

    #[test]
    fn skips_a_report_written_before_the_latest_change() {
        let mut scene = Scene::default();
        scene.select(Some(1));
        let report = |changes, value| {
            format!(
                r#"@besfa {{"type":"entity","id":1,"changes":{changes},"components":[{{"name":"A","path":"a::A","mutable":true,"value":{value}}}]}}"#
            )
        };
        scene.handle(&report(0, 1)).unwrap();
        scene.sent(&json!({"command": "set", "id": 1, "component": "a::A", "value": 2}));
        assert_eq!(scene.components[0].value, Some(json!(2)), "shown at once");
        scene.handle(&report(0, 1)).unwrap();
        assert_eq!(
            scene.components[0].value,
            Some(json!(2)),
            "stale report skipped"
        );
        scene.handle(&report(1, 3)).unwrap();
        assert_eq!(scene.components[0].value, Some(json!(3)));
    }

    #[test]
    fn leaves_logs_and_malformed_reports_to_the_log_panel() {
        let mut scene = Scene::default();
        assert!(scene.handle("INFO my_game: Playing.").is_none());
        assert!(scene.handle("@besfa not json").is_none());
        assert!(
            scene
                .handle(r#"@besfa {"type":"entities","entities":"?"}"#)
                .is_none()
        );
    }

    #[test]
    fn reset_keeps_the_selection_for_the_next_game_process() {
        let mut scene = Scene::default();
        scene.select(Some(7));
        scene.handle(r#"@besfa {"type":"entities","entities":[{"id":7}]}"#);
        scene.mark_dirty();
        scene.reset();
        assert!(scene.entities.is_empty());
        assert_eq!(scene.selected, Some(7));
        assert!(!scene.dirty, "the new process reads the file");
    }

    #[test]
    fn reads_what_the_editor_can_change_and_what_the_game_did() {
        let mut scene = Scene::default();
        scene.select(Some(1));
        scene.handle(
            r#"@besfa {"type":"entity","id":1,"components":[{"name":"A","path":"a::A","mutable":true}]}"#,
        );
        scene.handle(
            r#"@besfa {"type":"components","components":[{"name":"MeshShape","path":"besfa_editor_plugin::scene::MeshShape"}]}"#,
        );
        assert_eq!(crate_of(&scene.addable[0].path), "besfa_editor_plugin");
        // The game re-reporting the same selection keeps the components.
        scene.handle(r#"@besfa {"type":"selected","id":1}"#);
        assert_eq!(scene.components.len(), 1);
        scene.handle(r#"@besfa {"type":"selected","id":9}"#);
        assert_eq!(scene.selected, Some(9));
        assert!(scene.components.is_empty());
        scene.mark_dirty();
        scene.handle(r#"@besfa {"type":"saved","error":"denied"}"#);
        assert!(scene.dirty, "a failed save keeps the mark");
        scene.handle(r#"@besfa {"type":"saved","error":null}"#);
        assert!(!scene.dirty);
        scene.handle(r#"@besfa {"type":"entities","entities":[{"id":9,"scene":false},{"id":3}]}"#);
        assert!(!scene.selected_entity().unwrap().scene);
        assert!(scene.entities[1].scene);
    }

    #[test]
    fn finds_the_placed_file_and_lists_the_tree() {
        let mut scene = Scene::default();
        scene.handle(
            r#"@besfa {"type":"entities","entities":[{"id":1,"asset":"prefabs/a.scn.ron"},{"id":2,"parent":1},{"id":3,"parent":2},{"id":4},{"id":5,"parent":6},{"id":6,"parent":5}]}"#,
        ).unwrap();
        let deep = &scene.entities[2];
        assert_eq!(scene.placement(deep).map(|e| e.id), Some(1));
        assert_eq!(scene.placement(&scene.entities[3]), None);
        assert_eq!(scene.placement(&scene.entities[4]), None, "a cycle ends");
        let rows: Vec<_> = tree_rows(&scene.entities)
            .iter()
            .map(|(e, d)| (e.id, *d))
            .collect();
        // The cycle has no root, so it is not listed.
        assert_eq!(rows, [(1, 0), (2, 1), (3, 2), (4, 0)]);
    }

    #[test]
    fn undoes_redoes_and_forgets_what_was_undone_on_a_new_change() {
        let mut history = EditHistory::default();
        let edit = |n: i32| Edit {
            undo: vec![json!({"undo": n})],
            redo: vec![json!({"redo": n})],
        };
        assert!(history.undo().is_none());
        history.add(edit(1));
        history.add(edit(2));
        assert_eq!(history.undo(), Some(vec![json!({"undo": 2})]));
        assert!(history.can_redo());
        assert_eq!(history.redo(), Some(vec![json!({"redo": 2})]));
        assert!(history.redo().is_none());
        history.undo();
        let forgotten = history.add(edit(3));
        assert_eq!(
            forgotten[0].undo,
            [json!({"undo": 2})],
            "what was undone is forgotten"
        );
        assert!(!history.can_redo());
        assert_eq!(history.undo(), Some(vec![json!({"undo": 3})]));
        assert_eq!(history.undo(), Some(vec![json!({"undo": 1})]));
        history.clear();
        assert!(!history.can_undo() && !history.can_redo());
    }
}
