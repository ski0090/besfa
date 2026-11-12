//! The Organization tab's left tree: folders holding the CCGS roles and the
//! user's saved agents. Rows drag between folders, folders can be added,
//! renamed and removed, and the tree is kept in the project.

use eframe::egui::{self, Id, RichText, Sense, Stroke, Ui, Vec2};
use serde::{Deserialize, Serialize};

use super::{Department, Effort, Node, ROLES, find_role, title};
use crate::{agent::Provider, i18n::t};

/// Where an entry is: its index in the root list, then in its folder, and so
/// on down.
type Path = Vec<usize>;

/// The folder a node saved from the canvas goes to.
const MY_AGENTS: &str = "My agents";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Entry {
    Folder(Folder),
    /// A CCGS role, by its file name.
    Role(String),
    /// One of the user's own agents, as it was when saved.
    Agent(Saved),
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Folder {
    pub name: String,
    pub open: bool,
    pub entries: Vec<Entry>,
}

/// What a click on a saved agent adds to the graph.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Saved {
    pub name: String,
    pub instructions: String,
    pub provider: Provider,
    pub model: String,
    pub effort: Effort,
}

impl Saved {
    pub fn of(node: &Node) -> Self {
        Self {
            name: node.name.clone(),
            instructions: node.instructions.clone(),
            provider: node.provider,
            model: node.model.clone(),
            effort: node.effort,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Tree {
    pub entries: Vec<Entry>,
}

/// The CCGS departments, each with its roles.
impl Default for Tree {
    fn default() -> Self {
        let entries = Department::ALL
            .into_iter()
            .map(|department| {
                Entry::Folder(Folder {
                    name: department.key().into(),
                    open: department == Department::Leadership,
                    entries: ROLES
                        .iter()
                        .filter(|role| role.department == department)
                        .map(|role| Entry::Role(role.name.into()))
                        .collect(),
                })
            })
            .collect();
        Self { entries }
    }
}

impl Tree {
    fn list_mut(&mut self, folder: &[usize]) -> Option<&mut Vec<Entry>> {
        let mut list = &mut self.entries;
        for &index in folder {
            match list.get_mut(index)? {
                Entry::Folder(folder) => list = &mut folder.entries,
                _ => return None,
            }
        }
        Some(list)
    }

    fn entry_mut(&mut self, path: &[usize]) -> Option<&mut Entry> {
        let (&index, parent) = path.split_last()?;
        self.list_mut(parent)?.get_mut(index)
    }

    /// Moves the entry at `from` to place `at` of the folder `into` (the
    /// root when empty), or to its end when `at` is past it. Refused for a
    /// folder into itself or what is inside it.
    pub fn move_to(&mut self, from: &[usize], into: &[usize], at: usize) -> bool {
        let Some((&index, parent)) = from.split_last() else {
            return false;
        };
        if into.starts_with(from) || self.list_mut(into).is_none() {
            return false;
        }
        let mut into = into.to_vec();
        let mut at = at;
        // Taking it out moves what comes after it in its folder up one.
        if into == parent {
            if at > index {
                at -= 1;
            }
        } else if into.starts_with(parent) && into[parent.len()] > index {
            into[parent.len()] -= 1;
        }
        let Some(list) = self.list_mut(parent).filter(|list| index < list.len()) else {
            return false;
        };
        let entry = list.remove(index);
        let list = self.list_mut(&into).expect("the target was checked");
        list.insert(at.min(list.len()), entry);
        true
    }

    /// Takes the entry at `path` out. A folder's entries stay, in its place.
    pub fn remove(&mut self, path: &[usize]) {
        let Some((&index, parent)) = path.split_last() else {
            return;
        };
        let Some(list) = self.list_mut(parent).filter(|list| index < list.len()) else {
            return;
        };
        if let Entry::Folder(folder) = list.remove(index) {
            list.splice(index..index, folder.entries);
        }
    }

    /// An empty folder at the end of the folder `parent`, which opens.
    pub fn new_folder(&mut self, parent: &[usize]) {
        let folder = Entry::Folder(Folder {
            name: t("New folder").into(),
            open: true,
            ..Default::default()
        });
        if let Some(list) = self.list_mut(parent) {
            list.push(folder);
        }
        if let Some(Entry::Folder(parent)) = self.entry_mut(parent) {
            parent.open = true;
        }
    }

    /// Adds `agent` to the "My agents" folder, which it makes if needed.
    pub fn add_agent(&mut self, agent: Saved) {
        let at = self
            .entries
            .iter()
            .position(|entry| matches!(entry, Entry::Folder(folder) if folder.name == MY_AGENTS))
            .unwrap_or_else(|| {
                self.entries.push(Entry::Folder(Folder {
                    name: MY_AGENTS.into(),
                    ..Default::default()
                }));
                self.entries.len() - 1
            });
        if let Entry::Folder(folder) = &mut self.entries[at] {
            folder.open = true;
            folder.entries.push(Entry::Agent(agent));
        }
    }

    /// Draws the tree, and returns the role or agent that was clicked.
    /// `running` keeps clicks from adding to a graph that is running.
    pub fn show(&mut self, ui: &mut Ui, running: bool) -> Option<Entry> {
        let mut edits = Edits::default();
        rows(ui, &mut self.entries, &mut Vec::new(), running, &mut edits);
        // Room below the last row to drop onto, for the end of the root.
        let tail = ui.allocate_response(Vec2::new(ui.available_width(), 40.0), Sense::hover());
        if tail.dnd_hover_payload::<Path>().is_some() {
            let stroke = Stroke::new(2.0, ui.visuals().selection.bg_fill);
            ui.painter()
                .hline(tail.rect.x_range(), tail.rect.top(), stroke);
        }
        if let Some(from) = tail.dnd_release_payload::<Path>() {
            edits.moved = Some((from.to_vec(), Vec::new(), usize::MAX));
        }
        if let Some((from, into, at)) = edits.moved {
            self.move_to(&from, &into, at);
        } else if let Some(parent) = edits.new_folder {
            self.new_folder(&parent);
        } else if let Some(path) = edits.removed {
            self.remove(&path);
        }
        edits.add
    }
}

#[derive(Default)]
struct Edits {
    /// From, the folder to, and the place in it.
    moved: Option<(Path, Path, usize)>,
    new_folder: Option<Path>,
    removed: Option<Path>,
    add: Option<Entry>,
}

fn rows(ui: &mut Ui, entries: &mut [Entry], path: &mut Path, running: bool, edits: &mut Edits) {
    for (index, entry) in entries.iter_mut().enumerate() {
        path.push(index);
        let here = path.clone();
        match entry {
            Entry::Folder(folder) => {
                let color = Department::ALL
                    .into_iter()
                    .find(|department| department.key() == folder.name)
                    .map(Department::color);
                let arrow = if folder.open { "▾" } else { "▸" };
                let name = if folder.name.is_empty() {
                    "…"
                } else {
                    t(&folder.name)
                };
                let mut text = RichText::new(format!("{arrow} {name}"));
                if let Some(color) = color {
                    text = text.color(color);
                }
                let button = row(ui, &here, true, edits, text, "");
                if button.clicked() {
                    folder.open = !folder.open;
                }
                button.context_menu(|ui| {
                    ui.text_edit_singleline(&mut folder.name);
                    if ui.button(t("New folder")).clicked() {
                        edits.new_folder = Some(here.clone());
                        ui.close();
                    }
                    if ui
                        .button(t("Remove folder"))
                        .on_hover_text(t("Keeps what is inside."))
                        .clicked()
                    {
                        edits.removed = Some(here.clone());
                        ui.close();
                    }
                });
                if folder.open {
                    ui.indent(Id::new(("roles_tree_indent", &here)), |ui| {
                        rows(ui, &mut folder.entries, path, running, edits);
                    });
                }
            }
            Entry::Role(name) => {
                // A role this version no longer has shows plain and adds nothing.
                let found = find_role(name);
                let text = match found {
                    Some(role) => RichText::new(title(role.name)).color(role.department.color()),
                    None => RichText::new(title(name)).weak(),
                };
                let hover = found.map_or("", |role| role.summary);
                let button = row(ui, &here, false, edits, text, hover);
                if button.clicked() && !running && found.is_some() {
                    edits.add = Some(Entry::Role(name.clone()));
                }
                button.context_menu(|ui| delete(ui, &here, edits));
            }
            Entry::Agent(saved) => {
                let hover = saved.instructions.lines().next().unwrap_or_default();
                let button = row(ui, &here, false, edits, RichText::new(&saved.name), hover);
                if button.clicked() && !running {
                    edits.add = Some(Entry::Agent(saved.clone()));
                }
                button.context_menu(|ui| {
                    ui.text_edit_singleline(&mut saved.name);
                    delete(ui, &here, edits);
                });
            }
        }
        path.pop();
    }
}

fn delete(ui: &mut Ui, here: &[usize], edits: &mut Edits) {
    if ui.button(t("Delete")).clicked() {
        edits.removed = Some(here.to_vec());
        ui.close();
    }
}

/// One row, a button to click, to drag from and to drop onto: a folder takes
/// what is dropped on its lower part into it, any other row takes it before
/// or after itself. Returns the button's response.
fn row(
    ui: &mut Ui,
    here: &[usize],
    folder: bool,
    edits: &mut Edits,
    text: RichText,
    hover: &str,
) -> egui::Response {
    let button = egui::Button::new(text)
        .frame(false)
        .sense(Sense::click_and_drag());
    let mut response = ui.add(button).on_hover_cursor(egui::CursorIcon::Grab);
    if !hover.is_empty() {
        response = response.on_hover_text(hover);
    }
    response.dnd_set_drag_payload(here.to_vec());
    let rect = response.rect;
    let Some(from) = response.dnd_hover_payload::<Path>() else {
        return response;
    };
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return response;
    };
    let fraction = (pointer.y - rect.top()) / rect.height().max(1.0);
    let (parent, index) = here.split_at(here.len() - 1);
    let (into, at) = if folder && fraction > 0.25 {
        (here.to_vec(), usize::MAX)
    } else {
        let after = !folder && fraction > 0.5;
        (parent.to_vec(), index[0] + usize::from(after))
    };
    // Not onto itself, or into what it holds.
    if into.starts_with(&from) {
        return response;
    }
    let stroke = Stroke::new(2.0, ui.visuals().selection.bg_fill);
    if at == usize::MAX {
        ui.painter()
            .rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    } else {
        let y = if at > index[0] {
            rect.bottom()
        } else {
            rect.top()
        };
        ui.painter().hline(rect.x_range(), y, stroke);
    }
    if let Some(from) = response.dnd_release_payload::<Path>() {
        edits.moved = Some((from.to_vec(), into, at));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, entries: Vec<Entry>) -> Entry {
        Entry::Folder(Folder {
            name: name.into(),
            open: true,
            entries,
        })
    }

    fn role(name: &str) -> Entry {
        Entry::Role(name.into())
    }

    fn names(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| match entry {
                Entry::Folder(folder) => {
                    format!("{}[{}]", folder.name, names(&folder.entries).join(","))
                }
                Entry::Role(name) => name.clone(),
                Entry::Agent(saved) => saved.name.clone(),
            })
            .collect()
    }

    fn sample() -> Tree {
        Tree {
            entries: vec![
                folder("a", vec![role("w"), role("x")]),
                folder("b", vec![role("y")]),
                role("z"),
            ],
        }
    }

    #[test]
    fn moves_keep_their_place() {
        // Down in its own folder: before "z" is the end.
        let mut tree = sample();
        assert!(tree.move_to(&[0, 0], &[0], 2));
        assert_eq!(names(&tree.entries), ["a[x,w]", "b[y]", "z"]);
        // Into a later folder, which an earlier removal must not shift.
        let mut tree = Tree {
            entries: vec![role("w"), folder("b", vec![role("y")])],
        };
        assert!(tree.move_to(&[0], &[1], 0));
        assert_eq!(names(&tree.entries), ["b[w,y]"]);
        // To the root, before the first entry.
        let mut tree = sample();
        assert!(tree.move_to(&[1, 0], &[], 0));
        assert_eq!(names(&tree.entries), ["y", "a[w,x]", "b[]", "z"]);
    }

    #[test]
    fn a_folder_cannot_go_into_itself() {
        let mut tree = sample();
        assert!(!tree.move_to(&[0], &[0], 0));
        assert!(!tree.move_to(&[0], &[0, 0], 0));
        assert!(!tree.move_to(&[0], &[7], 0));
        assert_eq!(names(&tree.entries), ["a[w,x]", "b[y]", "z"]);
    }

    #[test]
    fn removing_a_folder_keeps_what_is_in_it() {
        let mut tree = sample();
        tree.remove(&[0]);
        assert_eq!(names(&tree.entries), ["w", "x", "b[y]", "z"]);
        tree.remove(&[1]);
        assert_eq!(names(&tree.entries), ["w", "b[y]", "z"]);
    }

    #[test]
    fn saved_agents_land_in_my_agents_and_the_tree_reads_back() {
        let mut tree = Tree::default();
        let agent = Saved {
            name: "UI Designer".into(),
            instructions: "layouts".into(),
            ..Default::default()
        };
        tree.add_agent(agent.clone());
        tree.add_agent(agent);
        let text = ron::to_string(&tree).unwrap();
        let back: Tree = ron::from_str(&text).unwrap();
        assert_eq!(back, tree);
        let Some(Entry::Folder(mine)) = back.entries.last() else {
            panic!("no folder");
        };
        assert_eq!((mine.name.as_str(), mine.entries.len()), (MY_AGENTS, 2));
    }

    #[test]
    fn the_default_tree_has_every_role_once() {
        let tree = Tree::default();
        let all = names(&tree.entries).join(",");
        let all: Vec<&str> = all.split(['[', ']', ',']).collect();
        for role in ROLES {
            assert_eq!(all.iter().filter(|&&name| name == role.name).count(), 1);
        }
    }
}
