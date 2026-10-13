//! What the running game reports about its world, read from its output, and
//! the changes the editor made to it, for undo.

use std::collections::{HashMap, HashSet};

use besfa_protocol::{Command, ComponentData, ComponentType, EntityInfo, Report, SystemInfo};

use crate::inspector::Inspected;

/// What the running game reports about its world, and whether the editor
/// changed it since the last save.
#[derive(Default)]
pub struct Scene {
    pub entities: Vec<EntityInfo>,
    /// The entity whose components are shown; the game reports them.
    pub selected: Option<u64>,
    pub components: Vec<ComponentData>,
    pub systems: Vec<SystemInfo>,
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
    /// The selected entity, for the inspector.
    pub fn inspected(&self) -> Inspected<'_> {
        let entity = self.selected_entity();
        Inspected {
            selected: self.selected,
            entity,
            placement: entity.and_then(|entity| self.placement(entity)),
            components: &self.components,
            systems: &self.systems,
            addable: &self.addable,
            crate_name: self.crate_name.as_deref(),
        }
    }

    /// The count of `set`, `insert` and `remove` commands sent to this game.
    pub fn changes(&self) -> u64 {
        self.changes
    }

    pub fn selected_entity(&self) -> Option<&EntityInfo> {
        self.entities
            .iter()
            .find(|entity| Some(entity.id) == self.selected)
    }

    /// The placed file `entity` is part of: the entity itself or its nearest
    /// ancestor that shows one.
    pub fn placement<'a>(&'a self, entity: &'a EntityInfo) -> Option<&'a EntityInfo> {
        let by_id: HashMap<u64, &EntityInfo> = self.entities.iter().map(|e| (e.id, e)).collect();
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
    pub fn handle(&mut self, line: &str) -> Option<Report> {
        // Game output is not trusted to be well formed.
        let report = Report::parse(line)?;
        match &report {
            Report::Entities { entities } => self.entities = entities.clone(),
            // A report for an earlier selection, or from before the latest
            // change, may still be in the pipe. Early plugins send no count.
            Report::Entity {
                id,
                changes,
                components,
            } => {
                if Some(*id) == self.selected && changes.unwrap_or(self.changes) >= self.changes {
                    self.components = components.clone();
                }
            }
            Report::Systems {
                game_crate,
                systems,
            } => {
                self.systems = systems.clone();
                self.crate_name = game_crate.clone();
            }
            Report::Components { components } => self.addable = components.clone(),
            // The game picked an entity itself: a new one, or none once the
            // selected one is gone.
            Report::Selected { id } => self.select(*id),
            Report::Saved { error: None } => self.dirty = false,
            // A drag in the scene view changed a component.
            Report::Edited { .. } => self.dirty = true,
            _ => {}
        }
        Some(report)
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
    pub fn sent(&mut self, command: &Command) {
        match command {
            Command::Set {
                id,
                component,
                value,
            } => {
                self.changes += 1;
                if Some(*id) == self.selected {
                    let value = Some(value.clone()).filter(|value| !value.is_null());
                    for reported in &mut self.components {
                        if reported.path == *component {
                            reported.value = value.clone();
                        }
                    }
                }
            }
            Command::Insert { .. } | Command::Remove { .. } => self.changes += 1,
            _ => {}
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
pub fn tree_rows(entities: &[EntityInfo]) -> Vec<(&EntityInfo, usize)> {
    let ids: HashSet<u64> = entities.iter().map(|entity| entity.id).collect();
    let mut children: HashMap<Option<u64>, Vec<&EntityInfo>> = HashMap::new();
    for entity in entities {
        let parent = entity.parent.filter(|parent| ids.contains(parent));
        children.entry(parent).or_default().push(entity);
    }
    let mut rows = Vec::new();
    // A parent cycle would be a game bug; this keeps it from hanging the editor.
    let mut seen = HashSet::new();
    let mut stack: Vec<(&EntityInfo, usize)> = children
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
    pub undo: Vec<Command>,
    pub redo: Vec<Command>,
    /// The request that made the change, while the game may still say it
    /// failed.
    pub seq: Option<u64>,
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
    pub fn undo(&mut self) -> Option<Vec<Command>> {
        let edit = self.done.pop()?;
        let commands = edit.undo.clone();
        self.undone.push(edit);
        Some(commands)
    }

    /// The commands that redo the last undone change, if any.
    pub fn redo(&mut self) -> Option<Vec<Command>> {
        let edit = self.undone.pop()?;
        let commands = edit.redo.clone();
        self.done.push(edit);
        Some(commands)
    }

    /// Forgets the change request `seq` made: the game could not make it,
    /// so there is nothing to undo.
    pub fn discard(&mut self, seq: u64) {
        self.done.retain(|edit| edit.seq != Some(seq));
        self.undone.retain(|edit| edit.seq != Some(seq));
    }

    /// A new game process: the ids belong to the old one.
    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }
}

#[cfg(test)]
mod tests {
    use besfa_protocol::crate_of;
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
        let labels: Vec<_> = scene.entities.iter().map(EntityInfo::label).collect();
        assert_eq!(labels, ["Cube", "Entity 1v0"]);
        assert_eq!(scene.entities[1].parent, Some(4294967295));

        scene.select(Some(4294967295));
        scene.handle(
            r#"@besfa {"type":"entity","id":4294967295,"components":[{"name":"Transform","path":"bevy_transform::components::transform::Transform","mutable":true,"required_by":null,"value":{"translation":[1.0,2.0,3.0]}},{"name":"Spin","path":"my_game::Spin","mutable":true,"required_by":null,"value":null}]}"#,
        ).unwrap();
        let crates: Vec<_> = scene
            .components
            .iter()
            .map(ComponentData::crate_name)
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
        scene.sent(&Command::Set {
            id: 1,
            component: "a::A".into(),
            value: json!(2),
        });
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
        // A newer plugin's report is still a report.
        assert_eq!(
            scene.handle(r#"@besfa {"type":"from_the_future"}"#),
            Some(Report::Unknown)
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
    fn undoes_redoes_and_forgets_what_was_undone_or_failed() {
        let mut history = EditHistory::default();
        let edit = |n: u64| Edit {
            undo: vec![Command::Restore { id: n }],
            redo: vec![Command::Delete { id: n }],
            seq: Some(n),
        };
        assert!(history.undo().is_none());
        history.add(edit(1));
        history.add(edit(2));
        assert_eq!(history.undo(), Some(vec![Command::Restore { id: 2 }]));
        assert!(history.can_redo());
        assert_eq!(history.redo(), Some(vec![Command::Delete { id: 2 }]));
        assert!(history.redo().is_none());
        history.undo();
        let forgotten = history.add(edit(3));
        assert_eq!(
            forgotten[0].undo,
            [Command::Restore { id: 2 }],
            "what was undone is forgotten"
        );
        assert!(!history.can_redo());

        history.discard(3);
        assert_eq!(
            history.undo(),
            Some(vec![Command::Restore { id: 1 }]),
            "a failed change leaves nothing to undo"
        );
        history.clear();
        assert!(!history.can_undo() && !history.can_redo());
    }
}
