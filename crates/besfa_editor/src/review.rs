//! What the model panel changed on the previewed model: components set,
//! added or removed on its parts and root, and named poses. Changes are kept
//! by part path, so they apply again whenever the model loads, in a new
//! game process or after the file is exported again.

use std::collections::BTreeMap;

use besfa_protocol::Command;
use serde_json::Value;

use crate::inspector::TRANSFORM;

/// The model's root, in place of a part path.
pub const ROOT: &str = "";

/// A change to one component of a part.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Set(Value),
    /// Added with its default value.
    Insert,
    Remove,
}

/// A part's component, by part path and component type path.
type Key = (String, String);

#[derive(Default, Debug, PartialEq)]
pub struct Edits {
    /// The latest change to each component.
    pub changes: BTreeMap<Key, Change>,
    /// What each component changed so far was as the model loaded; None
    /// when the part did not have it. Kept after a revert, for poses.
    pub originals: BTreeMap<Key, Option<Value>>,
    pub poses: Vec<Pose>,
}

/// Transforms for some parts, by part path.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub name: String,
    pub transforms: BTreeMap<String, Value>,
}

impl Edits {
    /// Notes `change` to the `component` of the part at `path`, which was
    /// `before` it. A change back to how the part was forgets it.
    pub fn record(&mut self, path: &str, component: &str, before: Option<Value>, change: Change) {
        let key = (path.to_owned(), component.to_owned());
        let original = self.originals.entry(key.clone()).or_insert(before).clone();
        let back = match &change {
            Change::Set(value) => original.as_ref() == Some(value),
            Change::Remove => original.is_none(),
            Change::Insert => false,
        };
        if back {
            self.changes.remove(&key);
        } else {
            self.changes.insert(key, change);
        }
    }

    /// The commands that make the model this way again, for parts whose ids
    /// `id` knows by path.
    pub fn replay(&self, id: impl Fn(&str) -> Option<u64>) -> Vec<Command> {
        self.changes
            .iter()
            .filter_map(|((path, component), change)| {
                Some(command(id(path)?, component, change.clone()))
            })
            .collect()
    }

    /// Takes back the changes `which` picks and forgets them, returning the
    /// commands that do it.
    fn revert(
        &mut self,
        id: &impl Fn(&str) -> Option<u64>,
        which: impl Fn(&Key) -> bool,
    ) -> Vec<Command> {
        let keys: Vec<Key> = self
            .changes
            .keys()
            .filter(|key| which(key))
            .cloned()
            .collect();
        let mut commands = Vec::new();
        for key in keys {
            self.changes.remove(&key);
            let original = self.originals.get(&key).cloned().flatten();
            if let Some(id) = id(&key.0) {
                let change = original.map_or(Change::Remove, Change::Set);
                commands.push(command(id, &key.1, change));
            }
        }
        commands
    }

    /// Every part back the way it loaded.
    pub fn revert_all(&mut self, id: impl Fn(&str) -> Option<u64>) -> Vec<Command> {
        self.revert(&id, |_| true)
    }

    /// Every part's transform back the way it loaded.
    pub fn rest(&mut self, id: impl Fn(&str) -> Option<u64>) -> Vec<Command> {
        self.revert(&id, |(_, component)| component == TRANSFORM)
    }

    /// Keeps the parts' changed transforms as a pose named `name`, in place
    /// of one with that name.
    pub fn save_pose(&mut self, name: &str) {
        let transforms = self
            .changes
            .iter()
            .filter(|((_, component), _)| component == TRANSFORM)
            .filter_map(|((path, _), change)| match change {
                Change::Set(value) => Some((path.clone(), value.clone())),
                _ => None,
            })
            .collect();
        let pose = Pose {
            name: name.to_owned(),
            transforms,
        };
        match self.poses.iter_mut().find(|pose| pose.name == name) {
            Some(old) => *old = pose,
            None => self.poses.push(pose),
        }
    }

    /// Puts the model in `pose`: the rest back first, then the pose's
    /// transforms.
    pub fn apply_pose(&mut self, pose: usize, id: impl Fn(&str) -> Option<u64>) -> Vec<Command> {
        let Some(pose) = self.poses.get(pose).cloned() else {
            return Vec::new();
        };
        let mut commands = self.rest(&id);
        for (path, value) in pose.transforms {
            let key = (path.clone(), TRANSFORM.to_owned());
            let before = self.originals.get(&key).cloned().flatten();
            self.record(&path, TRANSFORM, before, Change::Set(value.clone()));
            if let Some(id) = id(&path) {
                commands.push(command(id, TRANSFORM, Change::Set(value)));
            }
        }
        commands
    }
}

fn command(id: u64, component: &str, change: Change) -> Command {
    let component = component.to_owned();
    match change {
        Change::Set(value) => Command::Set {
            id,
            component,
            value,
        },
        Change::Insert => Command::Insert { id, component },
        Change::Remove => Command::Remove { id, component },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ids(path: &str) -> Option<u64> {
        match path {
            ROOT => Some(1),
            "Chest/Lid" => Some(2),
            _ => None,
        }
    }

    #[test]
    fn replays_reverts_and_forgets_changes_back_to_the_original() {
        let rest = json!({"translation": [0, 0, 0]});
        let open = json!({"translation": [0, 1, 0]});
        let mut edits = Edits::default();
        edits.record(
            "Chest/Lid",
            TRANSFORM,
            Some(rest.clone()),
            Change::Set(open.clone()),
        );
        edits.record(ROOT, "game::Look", None, Change::Insert);
        assert_eq!(
            edits.replay(ids),
            [
                Command::Insert {
                    id: 1,
                    component: "game::Look".into()
                },
                Command::Set {
                    id: 2,
                    component: TRANSFORM.into(),
                    value: open.clone()
                },
            ]
        );

        // Set back by hand: nothing left to replay for it.
        edits.record(
            "Chest/Lid",
            TRANSFORM,
            Some(open.clone()),
            Change::Set(rest.clone()),
        );
        assert_eq!(edits.changes.len(), 1);

        edits.record(
            "Chest/Lid",
            TRANSFORM,
            Some(rest.clone()),
            Change::Set(open),
        );
        assert_eq!(
            edits.revert_all(ids),
            [
                Command::Remove {
                    id: 1,
                    component: "game::Look".into()
                },
                Command::Set {
                    id: 2,
                    component: TRANSFORM.into(),
                    value: rest
                },
            ]
        );
        assert!(edits.changes.is_empty());
    }

    #[test]
    fn saves_and_applies_poses_from_the_rest() {
        let rest = json!({"rotation": [0, 0, 0, 1]});
        let open = json!({"rotation": [0.7, 0, 0, 0.7]});
        let mut edits = Edits::default();
        edits.record(
            "Chest/Lid",
            TRANSFORM,
            Some(rest.clone()),
            Change::Set(open.clone()),
        );
        edits.record(ROOT, "game::Look", None, Change::Insert);
        edits.save_pose("Open");
        assert_eq!(edits.poses[0].transforms.len(), 1, "transforms only");

        let lid = |edits: &Edits| {
            edits
                .changes
                .get(&("Chest/Lid".into(), TRANSFORM.into()))
                .cloned()
        };
        let commands = edits.rest(ids);
        assert_eq!(commands.len(), 1);
        assert_eq!(lid(&edits), None);
        assert_eq!(edits.changes.len(), 1, "other changes stay");

        let commands = edits.apply_pose(0, ids);
        assert_eq!(
            commands,
            [Command::Set {
                id: 2,
                component: TRANSFORM.into(),
                value: open.clone()
            }]
        );
        assert_eq!(lid(&edits), Some(Change::Set(open)));
        // Still knows the rest to go back to.
        assert_eq!(
            edits.rest(ids),
            [Command::Set {
                id: 2,
                component: TRANSFORM.into(),
                value: rest
            }]
        );
    }
}
