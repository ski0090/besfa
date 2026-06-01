import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/shared/ui/panel_title.dart';
import 'package:flutter/material.dart';

/// Inspector panel for the currently selected runtime scene entity.
class InspectorPanel extends StatelessWidget {
  const InspectorPanel({required this.selectedEntity, super.key});

  /// Entity selected in the runtime scene snapshot, if any.
  final RuntimeSceneEntity? selectedEntity;

  @override
  Widget build(BuildContext context) {
    final selectedEntity = this.selectedEntity;

    return ListView(
      padding: const EdgeInsets.all(12),
      children: [
        const PanelTitle('Inspector'),
        if (selectedEntity == null)
          Text(
            'No selection',
            style: Theme.of(context).textTheme.bodySmall?.copyWith(
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
          )
        else ...[
          _PropertyRow(label: 'Name', value: selectedEntity.name),
          _PropertyRow(label: 'Kind', value: selectedEntity.kind),
          _PropertyRow(label: 'Entity ID', value: selectedEntity.id),
        ],
      ],
    );
  }
}

class _PropertyRow extends StatelessWidget {
  const _PropertyRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 92,
            child: Text(label, style: Theme.of(context).textTheme.labelMedium),
          ),
          Expanded(child: SelectableText(value)),
        ],
      ),
    );
  }
}
