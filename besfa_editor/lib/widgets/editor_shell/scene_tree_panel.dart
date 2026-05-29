import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/shared/ui/panel_title.dart';
import 'package:flutter/material.dart';

class SceneTreePanel extends StatelessWidget {
  const SceneTreePanel({
    required this.snapshot,
    required this.onSelectEntity,
    super.key,
  });

  final RuntimeSceneSnapshot? snapshot;
  final ValueChanged<String> onSelectEntity;

  @override
  Widget build(BuildContext context) {
    final snapshot = this.snapshot;

    return ListView(
      padding: const EdgeInsets.all(12),
      children: [
        const PanelTitle('Scene'),
        if (snapshot == null)
          Text(
            'No scene',
            style: Theme.of(context).textTheme.bodySmall?.copyWith(
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
          )
        else
          _TreeItem(
            entity: snapshot.root,
            selectedEntityId: snapshot.selectedEntityId,
            onSelectEntity: onSelectEntity,
          ),
      ],
    );
  }
}

class _TreeItem extends StatelessWidget {
  const _TreeItem({
    required this.entity,
    required this.selectedEntityId,
    required this.onSelectEntity,
    this.indent = 0,
  });

  final RuntimeSceneEntity entity;
  final String? selectedEntityId;
  final ValueChanged<String> onSelectEntity;
  final double indent;

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        Padding(
          padding: EdgeInsets.only(left: indent),
          child: ListTile(
            dense: true,
            selected: selectedEntityId == entity.id,
            leading: Icon(_iconForKind(entity.kind), size: 18),
            title: Text(entity.name),
            contentPadding: EdgeInsets.zero,
            onTap: () => onSelectEntity(entity.id),
          ),
        ),
        for (final child in entity.children)
          _TreeItem(
            entity: child,
            selectedEntityId: selectedEntityId,
            onSelectEntity: onSelectEntity,
            indent: indent + 16,
          ),
      ],
    );
  }

  IconData _iconForKind(String kind) {
    return switch (kind) {
      'world' => Icons.public,
      'camera' => Icons.videocam,
      'light' => Icons.light_mode,
      'mesh' => Icons.category,
      _ => Icons.account_tree,
    };
  }
}
