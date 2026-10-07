import 'package:flutter/material.dart';

import 'package:editor_ui/entities/scene/model/scene.dart';
import 'package:editor_ui/shared/ui/panel.dart';

/// What the add menu spawns, by the `kind` the game takes.
const spawnKinds = {
  'empty': 'Empty',
  'cube': 'Cube',
  'sphere': 'Sphere',
  'plane': 'Plane',
  'light': 'Light',
  'camera': 'Camera',
};

/// The game's entities as a tree; tapping one selects it, tapping the
/// selected one clears the selection. With the callbacks, the header adds,
/// duplicates and deletes entities.
class SceneHierarchyPanel extends StatelessWidget {
  const SceneHierarchyPanel({
    super.key,
    required this.scene,
    required this.onSelect,
    this.onSpawn,
    this.onDuplicate,
    this.onDelete,
    this.onSavePrefab,
  });

  final Scene scene;
  final ValueChanged<int?> onSelect;

  /// A kind from [spawnKinds].
  final ValueChanged<String>? onSpawn;
  final VoidCallback? onDuplicate;
  final VoidCallback? onDelete;
  final VoidCallback? onSavePrefab;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: scene,
      builder: (context, _) {
        final rows = _treeRows(scene.entities);
        final hasSelection = scene.selectedEntity != null;
        return Panel(
          title: 'Hierarchy',
          actions: [
            if (onSpawn case final onSpawn?)
              PopupMenuButton<String>(
                tooltip: 'Add entity',
                icon: const Icon(Icons.add, size: 16),
                padding: EdgeInsets.zero,
                constraints: const BoxConstraints(minWidth: 120),
                onSelected: onSpawn,
                itemBuilder: (context) => [
                  for (final MapEntry(:key, :value) in spawnKinds.entries)
                    PopupMenuItem(value: key, height: 32, child: Text(value)),
                ],
              ),
            if (onDuplicate != null)
              PanelAction(
                icon: Icons.copy,
                tooltip: 'Duplicate (Ctrl+D)',
                onPressed: hasSelection ? onDuplicate : null,
              ),
            if (onDelete != null)
              PanelAction(
                icon: Icons.delete_outline,
                tooltip: 'Delete (Delete)',
                onPressed: hasSelection ? onDelete : null,
              ),
            if (onSavePrefab != null)
              PanelAction(
                icon: Icons.inventory_2_outlined,
                tooltip: 'Save as prefab',
                onPressed: hasSelection ? onSavePrefab : null,
              ),
          ],
          child: ListView.builder(
            itemCount: rows.length,
            itemBuilder: (context, index) {
              final (entity, depth) = rows[index];
              final selected = entity.id == scene.selected;
              return InkWell(
                onTap: () => onSelect(selected ? null : entity.id),
                child: Container(
                  height: 24,
                  padding: EdgeInsets.only(left: 12 + depth * 16, right: 12),
                  alignment: Alignment.centerLeft,
                  color: selected ? const Color(0x338CB4FF) : null,
                  child: Text(
                    entity.label,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 12,
                      // Entities the game spawns while running are not saved.
                      fontStyle: entity.scene ? null : FontStyle.italic,
                      color: entity.name == null || !entity.scene
                          ? panelMutedText
                          : panelText,
                    ),
                  ),
                ),
              );
            },
          ),
        );
      },
    );
  }
}

/// Entities in tree order with their depth. An entity whose parent is not
/// reported is a root.
List<(SceneEntity, int)> _treeRows(List<SceneEntity> entities) {
  final ids = {for (final entity in entities) entity.id};
  final children = <int?, List<SceneEntity>>{};
  for (final entity in entities) {
    final parent = ids.contains(entity.parent) ? entity.parent : null;
    (children[parent] ??= []).add(entity);
  }
  final rows = <(SceneEntity, int)>[];
  // A parent cycle would be a game bug; this keeps it from hanging the editor.
  final seen = <int>{};
  void visit(int? parent, int depth) {
    for (final entity in children[parent] ?? const <SceneEntity>[]) {
      if (seen.add(entity.id)) {
        rows.add((entity, depth));
        visit(entity.id, depth + 1);
      }
    }
  }

  visit(null, 0);
  return rows;
}
