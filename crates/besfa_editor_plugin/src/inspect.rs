//! Reports the world to the editor: the scene's entities, the selected
//! entity's components, every system, and the components the editor can add.
//!
//! A report is one stdout line: `@besfa ` and a JSON object with a `type`,
//! which tells reports from the game's logs; see [`besfa_protocol::Report`]. Entities and the selected
//! entity are reported when they change; systems and addable components
//! once, before the game runs. The game also reports a selection it made
//! itself and the outcome of a save.

use std::io::Write;

use besfa_protocol::{ComponentData, ComponentType, EntityInfo, Report, SystemInfo};
use bevy::{
    ecs::{
        component::{ComponentId, ComponentInfo},
        entity_disabling::Disabled,
        query::{Allow, Or},
        reflect::ReflectFromWorld,
        schedule::Schedules,
    },
    prelude::*,
    world_serialization::{DynamicWorldRoot, WorldAssetRoot},
};
use serde_json::Value;

use crate::{
    edit::Deleted,
    preview::ModelPreview,
    scene::{SceneEntity, json, left_out},
    scene_view::{EditorOnly, Parked},
};

/// The entity whose components are reported.
#[derive(Resource, Default)]
pub(crate) struct Selected(pub(crate) Option<Entity>);

/// How many `set`, `insert` and `remove` commands the game has taken. The
/// `entity` report carries the count, so the editor can tell a report
/// written before its latest change from one written after.
#[derive(Resource, Default)]
pub(crate) struct Changes(pub(crate) u64);

/// Selects `entity` from the game's side, telling the editor.
pub(crate) fn select(world: &mut World, entity: Option<Entity>) {
    world.resource_mut::<Selected>().0 = entity;
    report_selection(entity);
}

/// Tells the editor the game changed the selection.
pub(crate) fn report_selection(entity: Option<Entity>) {
    send_report(&Report::Selected {
        id: entity.map(Entity::to_bits),
    });
}

/// Tells the editor whether the scene file was written.
pub(crate) fn report_saved(error: Option<String>) {
    send_report(&Report::Saved { error });
}

/// Writes one report line.
pub(crate) fn send_report(report: &Report) {
    write(&report.line());
}

pub(crate) struct InspectPlugin;

impl Plugin for InspectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selected>()
            .init_resource::<Changes>()
            .init_resource::<EntitiesChanged>()
            // Last: after Update and transform propagation, so values are current.
            .add_systems(Last, (note_entity_changes, report).chain())
            .add_observer(note_removal);
    }

    /// Every plugin has added its systems by now and no schedule is running,
    /// so none is missing from `Schedules`.
    // ponytail: systems added while the game runs are not reported; report
    // from `Last` on a system count change if a game needs that.
    fn cleanup(&self, app: &mut App) {
        hide_internals(app.world_mut());
        send_report(&systems(app.world()));
        send_report(&addable_components(app.world()));
    }
}

/// Keeps out of the Hierarchy what Bevy spawns before the game runs, like
/// the gizmo renderers' named entities. Only plugins have run so far.
// ponytail: a game that spawns through `app.world_mut()` before `run` is
// hidden too; mark Bevy's entities by name if a game does that.
fn hide_internals(world: &mut World) {
    let internal: Vec<Entity> = world
        .query_filtered::<Entity, (
            Or<(With<Name>, With<Transform>, With<ChildOf>)>,
            Allow<Disabled>,
        )>()
        .iter(world)
        .collect();
    for entity in internal {
        world.entity_mut(entity).insert(EditorOnly);
    }
}

fn write(line: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
}

/// Writes `report` unless it is the one written last.
fn write_if_changed(last: &mut String, report: Report) {
    let line = report.line();
    if *last != line {
        write(&line);
        *last = line;
    }
}

/// Whether what the entities report shows may have changed since it was
/// last written. It starts out unwritten.
#[derive(Resource)]
struct EntitiesChanged(bool);

impl Default for EntitiesChanged {
    fn default() -> Self {
        Self(true)
    }
}

/// Notes a change to what the entities report shows: a name, a parent, a
/// saved or placed mark, or an entity joining the list.
#[allow(clippy::type_complexity, reason = "a system's parameters")]
fn note_entity_changes(
    changed: Query<
        (),
        (
            Or<(
                Changed<Name>,
                Changed<ChildOf>,
                Changed<SceneEntity>,
                Changed<DynamicWorldRoot>,
                Changed<WorldAssetRoot>,
                Added<Transform>,
                Added<EditorOnly>,
                Added<Deleted>,
            )>,
            Allow<Disabled>,
        ),
    >,
    mut entities_changed: ResMut<EntitiesChanged>,
) {
    if !changed.is_empty() {
        entities_changed.0 = true;
    }
}

/// Notes an entity losing what the entities report shows, or leaving the
/// list: a despawn removes every component, so it is noted too.
#[allow(clippy::type_complexity, reason = "the components the report shows")]
fn note_removal(
    _: On<
        Remove<(
            Name,
            ChildOf,
            SceneEntity,
            DynamicWorldRoot,
            WorldAssetRoot,
            Transform,
            EditorOnly,
            Deleted,
        )>,
    >,
    mut entities_changed: ResMut<EntitiesChanged>,
) {
    entities_changed.0 = true;
}

// ponytail: any change sends the whole list again, every frame for a game
// that spawns named entities every frame; report differences if that shows up.
fn report(
    world: &mut World,
    mut last_entities: Local<String>,
    mut last_entity: Local<String>,
    mut last_part: Local<String>,
) {
    // Not before the scene file has spawned, or what it holds would show
    // up as made by the game.
    if world.contains_resource::<crate::scene::Loading>() {
        return;
    }
    // Thousands of entities, in a scene of placed models, are too many to
    // list every frame.
    if std::mem::take(&mut world.resource_mut::<EntitiesChanged>().0) {
        write_if_changed(&mut last_entities, entities(world));
    }

    // The model preview's part, reported the way the selection is.
    let part = world
        .get_resource::<ModelPreview>()
        .and_then(|preview| preview.selected);
    match part.map(|part| (part, components(world, part))) {
        Some((part, Some(components))) => write_if_changed(
            &mut last_part,
            Report::Entity {
                id: part.to_bits(),
                changes: Some(world.resource::<Changes>().0),
                components,
            },
        ),
        // Gone with the model; the editor picks the part again by name.
        Some((_, None)) => world.resource_mut::<ModelPreview>().selected = None,
        None => last_part.clear(),
    }

    let Some(entity) = world.resource::<Selected>().0 else {
        last_entity.clear();
        return;
    };
    match components(world, entity) {
        // A new count makes a new line, so every change is answered.
        Some(components) => write_if_changed(
            &mut last_entity,
            Report::Entity {
                id: entity.to_bits(),
                changes: Some(world.resource::<Changes>().0),
                components,
            },
        ),
        // Despawned; the entities report drops it as well.
        None => select(world, None),
    }
}

/// The scene's entities with their names and parents: named or placed
/// ones and their children, disabled ones included. That leaves out the
/// hundreds of entities Bevy keeps for observers and events.
// ponytail: tells scene from internals by components; an explicit marker
// would be needed once internal entities carry transforms.
fn entities(world: &mut World) -> Report {
    let mut entities: Vec<_> = world
        .query_filtered::<(
            Entity,
            Option<&Name>,
            Option<&ChildOf>,
            Has<SceneEntity>,
            (Option<&DynamicWorldRoot>, Option<&WorldAssetRoot>),
        ), (
            Or<(With<Name>, With<Transform>, With<ChildOf>)>,
            Without<EditorOnly>,
            Without<Deleted>,
            Allow<Disabled>,
        )>()
        .iter(world)
        .collect();
    // Spawn order; `Entity`'s own order inverts the index.
    entities
        .sort_unstable_by_key(|(entity, ..)| (entity.index_u32(), entity.generation().to_bits()));
    let entities = entities
        .iter()
        .map(
            |(entity, name, parent, scene, (placed_scene, model))| EntityInfo {
                id: entity.to_bits(),
                name: name.map(|name| name.as_str().into()),
                parent: parent.map(|parent| parent.parent().to_bits()),
                // From the scene file or the editor; saving keeps it.
                scene: *scene,
                // A placed model, scene or prefab: the file it shows.
                asset: placed_scene
                    .and_then(|root| root.0.path())
                    .or_else(|| model.and_then(|root| root.0.path()))
                    .map(|path| path.without_label().to_string()),
            },
        )
        .collect();
    Report::Entities { entities }
}

/// The components on `entity`, or `None` once it is despawned. A value is
/// only there for components registered with `Reflect`.
fn components(world: &World, entity: Entity) -> Option<Vec<ComponentData>> {
    let infos: Vec<(ComponentId, &ComponentInfo)> = world.inspect_entity(entity).ok()?.collect();
    // The present component that requires a component. `Mesh3d` requires
    // `Visibility`, which requires `ViewVisibility`: the requirer with the
    // fewest requirements of its own is the direct one.
    let required_by = |id: ComponentId| {
        infos
            .iter()
            .filter(|(_, info)| {
                info.required_components()
                    .iter_ids()
                    .any(|required| required == id)
            })
            .min_by_key(|(_, info)| info.required_components().iter_ids().count())
            .map(|(_, info)| info.name().shortname().to_string())
    };
    let registry = world.resource::<AppTypeRegistry>().read();
    let entity_ref = world.entity(entity);
    let parked = world.get::<Parked>(entity).map(|parked| parked.active);
    let components = infos
        .iter()
        .map(|&(id, info)| {
            let mut value = info
                .type_id()
                .and_then(|type_id| registry.get_type_data::<ReflectComponent>(type_id))
                .and_then(|reflect| reflect.reflect(entity_ref))
                .map(|value| {
                    // Handles to loaded assets show their paths. Other
                    // handles and opaque types do not serialize; show them
                    // the way Debug prints them.
                    json(value.as_partial_reflect(), &registry)
                        .unwrap_or_else(|_| Value::String(format!("{value:?}")))
                });
            // A game camera is off in edit mode; show the switch it plays with.
            if let (Some(active), Some(Value::Object(fields))) = (parked, &mut value)
                && info.type_id() == Some(std::any::TypeId::of::<Camera>())
            {
                fields.insert("is_active".into(), active.into());
            }
            // The registry's path is what `set` and the scene file use;
            // the type name can differ from it.
            let registration = info.type_id().and_then(|id| registry.get(id));
            let path = registration.map_or_else(
                || info.name().to_string(),
                |registration| registration.type_info().type_path().to_string(),
            );
            let name = registration.map_or_else(
                || info.name().shortname().to_string(),
                |registration| {
                    registration
                        .type_info()
                        .type_path_table()
                        .short_path()
                        .to_string()
                },
            );
            ComponentData {
                name,
                path,
                mutable: info.mutable(),
                required_by: required_by(id),
                // Computed from other components; the scene file leaves it out.
                saved: info.type_id().is_some_and(|id| !left_out(id)),
                value,
            }
        })
        .collect();
    Some(components)
}

/// Every system of every schedule, with the game's crate name so the editor
/// can put the game's own systems first.
fn systems(world: &World) -> Report {
    let systems = world
        .resource::<Schedules>()
        .iter()
        .flat_map(|(label, schedule)| {
            let label = format!("{label:?}");
            system_names(schedule)
                .into_iter()
                .map(move |name| SystemInfo {
                    schedule: label.clone(),
                    name,
                })
        })
        .collect();
    Report::Systems {
        game_crate: game_crate(),
        systems,
    }
}

/// Initializing a schedule moves its systems out of the graph, so look in
/// both places.
fn system_names(schedule: &Schedule) -> Vec<String> {
    match schedule.systems() {
        Ok(systems) => systems
            .map(|(_, system)| system.name().to_string())
            .collect(),
        Err(_) => schedule
            .graph()
            .systems
            .iter()
            .map(|(_, system, _)| system.name().to_string())
            .collect(),
    }
}

/// Components the editor can add: reflected, with a default value, and not
/// computed from other components.
fn addable_components(world: &World) -> Report {
    let registry = world.resource::<AppTypeRegistry>().read();
    let mut components: Vec<(&str, &str)> = registry
        .iter()
        .filter(|registration| {
            registration.data::<ReflectComponent>().is_some()
                && (registration.data::<ReflectDefault>().is_some()
                    || registration.data::<ReflectFromWorld>().is_some())
                && !left_out(registration.type_id())
        })
        .map(|registration| {
            let table = registration.type_info().type_path_table();
            (table.short_path(), table.path())
        })
        .collect();
    components.sort_unstable_by_key(|(_, path)| *path);
    let components = components
        .into_iter()
        .map(|(name, path)| ComponentType {
            name: name.into(),
            path: path.into(),
        })
        .collect();
    Report::Components { components }
}

/// The game's crate name as its type paths start: `my-game.exe` is `my_game::`.
fn game_crate() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.file_stem()?.to_string_lossy().replace('-', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component)]
    struct Plain;

    fn find<'a>(components: &'a [ComponentData], name: &str) -> &'a ComponentData {
        components
            .iter()
            .find(|component| component.name == name)
            .unwrap_or_else(|| panic!("no component {name} in {components:?}"))
    }

    fn reported_entities(report: Report) -> Vec<EntityInfo> {
        match report {
            Report::Entities { entities } => entities,
            other => panic!("not an entities report: {other:?}"),
        }
    }

    #[test]
    fn reports_scene_entities_with_names_and_parents() {
        let mut world = World::new();
        // Neither named, placed nor a child: an internal entity to the editor.
        world.spawn(Plain);
        let parent = world.spawn(Name::new("Parent")).id();
        let child = world.spawn(ChildOf(parent)).id();
        let placed = world.spawn(Transform::default()).id();

        let entities = reported_entities(entities(&mut world));

        assert_eq!(
            entities.iter().map(|e| e.id).collect::<Vec<_>>(),
            [parent.to_bits(), child.to_bits(), placed.to_bits()],
            "spawn order, internals left out: {entities:?}"
        );
        let find = |entity: Entity| {
            entities
                .iter()
                .find(|reported| reported.id == entity.to_bits())
                .unwrap_or_else(|| panic!("{entity} missing from {entities:?}"))
        };
        assert_eq!(find(parent).name.as_deref(), Some("Parent"));
        assert_eq!(find(parent).parent, None);
        assert_eq!(find(child).name, None);
        assert_eq!(find(child).parent, Some(parent.to_bits()));
    }

    #[test]
    fn notes_only_changes_the_entities_report_shows() {
        fn changed(app: &mut App) -> bool {
            app.update();
            std::mem::take(&mut app.world_mut().resource_mut::<EntitiesChanged>().0)
        }
        let mut app = App::new();
        app.init_resource::<EntitiesChanged>()
            .add_systems(Update, note_entity_changes)
            .add_observer(note_removal);
        let cube = app
            .world_mut()
            .spawn((Name::new("Cube"), Transform::default()))
            .id();
        assert!(changed(&mut app), "spawned");
        assert!(!changed(&mut app));

        app.world_mut()
            .get_mut::<Transform>(cube)
            .unwrap()
            .translation
            .x = 1.0;
        assert!(!changed(&mut app), "a move is not listed");
        *app.world_mut().get_mut::<Name>(cube).unwrap() = Name::new("Box");
        assert!(changed(&mut app), "renamed");
        app.world_mut().entity_mut(cube).insert((Disabled, Deleted));
        assert!(changed(&mut app), "deleted");
        app.world_mut()
            .entity_mut(cube)
            .remove::<(Disabled, Deleted)>();
        assert!(changed(&mut app), "restored");
        app.world_mut().despawn(cube);
        assert!(changed(&mut app), "despawned");
    }

    #[test]
    fn hides_entities_bevy_spawns_before_the_game_runs() {
        let mut app = App::new();
        app.world_mut().spawn(Name::new("LineGizmoRenderer"));
        app.add_plugins(InspectPlugin);
        app.finish();
        app.cleanup();
        let cube = app.world_mut().spawn(Name::new("Cube")).id();

        let ids: Vec<u64> = reported_entities(entities(app.world_mut()))
            .iter()
            .map(|entity| entity.id)
            .collect();
        assert_eq!(ids, [cube.to_bits()]);
    }

    #[test]
    fn reports_components_with_reflected_values_and_requirers() {
        let mut app = App::new();
        app.register_type::<Transform>()
            .register_type::<Visibility>();
        let cube = app
            .world_mut()
            .spawn((
                Name::new("Cube"),
                Transform::from_xyz(1.0, 2.0, 3.0),
                Visibility::Hidden,
                Plain,
            ))
            .id();

        let components = components(app.world(), cube).expect("the cube exists");

        let transform = find(&components, "Transform");
        assert_eq!(
            transform.path,
            "bevy_transform::components::transform::Transform"
        );
        // glam serializes vectors as arrays.
        assert_eq!(transform.value.as_ref().unwrap()["translation"][1], 2.0);
        assert!(transform.mutable);
        assert_eq!(transform.required_by, None);
        // Required by Visibility, which Visibility::Hidden brought in.
        assert_eq!(
            find(&components, "ViewVisibility").required_by.as_deref(),
            Some("Visibility")
        );
        assert_eq!(
            find(&components, "Visibility").value,
            Some(Value::from("Hidden"))
        );
        // No Reflect: the name alone.
        let plain = find(&components, "Plain");
        assert_eq!(plain.value, None);
        assert!(plain.path.ends_with("::tests::Plain"));

        app.world_mut().despawn(cube);
        assert!(super::components(app.world(), cube).is_none());
    }

    #[test]
    fn reports_systems_per_schedule_before_and_after_they_run() {
        fn spin() {}
        let mut app = App::new();
        app.add_systems(Update, spin);

        let before = systems(app.world());
        app.update();
        let after = systems(app.world());

        for report in [before, after] {
            let Report::Systems { systems, .. } = report else {
                panic!("not a systems report: {report:?}");
            };
            assert!(
                systems
                    .iter()
                    .any(|system| system.schedule == "Update" && system.name.ends_with("::spin")),
                "{systems:?}"
            );
        }
    }
}
