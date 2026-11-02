# Scene entities

A user adds entities to the scene from the Hierarchy, changes their components in
the Inspector, duplicates and deletes them, and gets a deleted one back with Undo.

## Sub-features

- `entity-create` spawns an empty, cube, sphere, plane, light or camera, named and placed.
- `entity-edit` replaces a component's value, such as `Transform` or `Name`.
- `entity-component` adds and removes a component by type path.
- `entity-duplicate` copies an entity with its components.
- `entity-delete` hides an entity, and `restore` brings it back (what Undo sends).

## How to get to it (user POV)

- Hierarchy **➕** menu: **Empty**, **Cube**, **Sphere**, **Plane**, **Light**, **Camera**.
- Inspector: edit a field, **➕ Add component**, or a component's remove button.
- Hierarchy 🗐 or **Ctrl+D** duplicates. 🗑 or **Delete** deletes. **Ctrl+Z** undoes.

## Driving it with besfa MCP

Preconditions:

- Baseline game started and `entities` answered.

- **Create.** `send` `{"command":"spawn","kind":"cube","name":"VerifyBox","translation":[2,0.5,0]}`.
  The reply is a `spawned` report with the new id. `entities` with `name: "VerifyBox"` lists it with `scene: true`.
- **Edit.** `send` `{"command":"set","id":ID,"component":"bevy_transform::components::transform::Transform","value":{"translation":[2,3,0],"rotation":[0,0,0,1],"scale":[1,1,1]}}`.
  `entities` with `id` shows `translation` `[2.0, 3.0, 0.0]`.
- **Duplicate.** `send` `{"command":"duplicate","id":ID}`. A second `spawned` id. `entities` with `name: "VerifyBox"` lists two.
- **Delete and restore.** `send` `{"command":"delete","id":COPY}`. `entities` no longer lists the copy.
  `send` `{"command":"restore","id":COPY}`. It is listed again.
- **Proof.** `send` `save`, then read `assets/scenes/main.scn.ron`: it holds `"VerifyBox"`
  twice with the edited translation. Save the reports and that excerpt to the evidence folder.

## Gotchas

- `set` replaces the whole value: left-out fields take defaults. Always send all
  fields of the component.
- Component paths are type paths exactly as `entities` with an `id` prints them.
  Read them there instead of guessing.
- `delete` only hides, so the entity keeps its id for `restore`. A hidden entity is left out
  of the next save.
