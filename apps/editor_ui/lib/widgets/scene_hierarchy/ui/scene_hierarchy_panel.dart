import 'package:flutter/material.dart';

import 'package:editor_ui/entities/scene/model/scene.dart';
import 'package:editor_ui/shared/ui/panel.dart';

/// The game's entities as a tree; tapping one selects it, tapping the
/// selected one clears the selection.
class SceneHierarchyPanel extends StatelessWidget {
  const SceneHierarchyPanel({
    super.key,
    required this.scene,
    required this.onSelect,
  });

  final Scene scene;
  final ValueChanged<int?> onSelect;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: scene,
      builder: (context, _) {
        final rows = _treeRows(scene.entities);
        return Panel(
          title: 'Hierarchy',
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
                      color: entity.name == null ? panelMutedText : panelText,
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
