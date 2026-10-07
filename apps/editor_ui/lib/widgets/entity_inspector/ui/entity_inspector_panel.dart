import 'dart:convert';

import 'package:flutter/material.dart';

import 'package:editor_ui/entities/scene/model/scene.dart';
import 'package:editor_ui/shared/ui/panel.dart';
import 'package:editor_ui/widgets/entity_inspector/ui/value_editor.dart';

const _transformPath = 'bevy_transform::components::transform::Transform';

/// The selected entity's components, and every system of the game, grouped
/// by crate with the game's own crate first. With the callbacks, component
/// values become fields and components can be added and removed.
class EntityInspectorPanel extends StatelessWidget {
  const EntityInspectorPanel({
    super.key,
    required this.scene,
    this.onSet,
    this.onInsert,
    this.onRemove,
    this.onApplyPrefab,
  });

  final Scene scene;

  /// A component's type path and its whole new value.
  final void Function(String component, Object? value)? onSet;
  final ValueChanged<String>? onInsert;
  final ValueChanged<String>? onRemove;

  /// Writes the placed prefab of this id back to its file.
  final ValueChanged<int>? onApplyPrefab;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: scene,
      builder: (context, _) {
        final selected = scene.selected;
        final entity = scene.selectedEntity;
        return Panel(
          title: 'Inspector',
          child: ListView(
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(12, 12, 12, 4),
                child: Text(
                  selected == null
                      ? 'Select an entity in the Hierarchy'
                      : entity == null
                      ? 'Entity $selected is gone'
                      : entity.label,
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: entity == null ? panelMutedText : panelText,
                  ),
                ),
              ),
              if (entity != null) ..._placement(entity),
              if (selected != null) ...[
                const _SectionTitle('Components'),
                ..._crateGroups(
                  scene.components,
                  (component) => component.crate,
                  scene.crate,
                  // Groups that hold something the scene file keeps.
                  (components) =>
                      components.any((component) => component.saved),
                  (component) => _ComponentTile(
                    component,
                    // A new entity starts with fresh fields.
                    key: ValueKey('$selected/${component.path}'),
                    onSet: onSet,
                    onRemove: onRemove,
                  ),
                ),
                if (onInsert != null && entity != null)
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    child: Align(
                      alignment: Alignment.centerLeft,
                      child: TextButton.icon(
                        icon: const Icon(Icons.add, size: 16),
                        label: const Text('Add component'),
                        onPressed: () => _addComponent(context),
                      ),
                    ),
                  ),
              ],
              const _SectionTitle('Systems'),
              ..._crateGroups(
                scene.systems,
                (system) => system.crate,
                scene.crate,
                (_) => false,
                (system) => _SystemRow(system),
              ),
            ],
          ),
        );
      },
    );
  }

  /// Where the entity comes from when Save scene does not keep it, and a
  /// way to keep changes made inside a placed prefab.
  List<Widget> _placement(SceneEntity entity) {
    final placed = scene.placement(entity);
    final tag = switch (placed?.asset) {
      _ when entity.scene => null,
      final asset? => 'Part of $asset; Save scene skips it.',
      null => 'Spawned by the game while it runs; Save scene skips it.',
    };
    return [
      if (tag != null)
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12),
          child: _Tag(tag),
        ),
      if (placed != null &&
          placed.asset!.endsWith('.scn.ron') &&
          onApplyPrefab != null)
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12),
          child: Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              icon: const Icon(Icons.upload_file, size: 16),
              label: const Text('Apply to prefab'),
              onPressed: () => onApplyPrefab!(placed.id),
            ),
          ),
        ),
    ];
  }

  Future<void> _addComponent(BuildContext context) async {
    final present = {for (final component in scene.components) component.path};
    final path = await showDialog<String>(
      context: context,
      builder: (context) => _AddComponentDialog([
        for (final type in scene.addable)
          if (!present.contains(type.path)) type,
      ]),
    );
    if (path != null) {
      onInsert!(path);
    }
  }
}

/// One collapsible group per crate, the game's crate first. The game's
/// crate and groups [expand] picks start open.
List<Widget> _crateGroups<T>(
  List<T> items,
  String Function(T) crateOf,
  String? gameCrate,
  bool Function(List<T>) expand,
  Widget Function(T) build,
) {
  final groups = <String, List<T>>{};
  for (final item in items) {
    (groups[crateOf(item)] ??= []).add(item);
  }
  final crates = groups.keys.toList()
    ..sort(
      (a, b) => a == gameCrate
          ? -1
          : b == gameCrate
          ? 1
          : a.compareTo(b),
    );
  return [
    for (final crate in crates)
      ExpansionTile(
        key: ValueKey(crate),
        initiallyExpanded: crate == gameCrate || expand(groups[crate]!),
        dense: true,
        tilePadding: const EdgeInsets.symmetric(horizontal: 12),
        childrenPadding: const EdgeInsets.only(bottom: 4),
        shape: const Border(),
        collapsedShape: const Border(),
        title: Text(
          '$crate (${groups[crate]!.length})',
          style: const TextStyle(fontSize: 12, color: panelText),
        ),
        children: [for (final item in groups[crate]!) build(item)],
      ),
  ];
}

class _SectionTitle extends StatelessWidget {
  const _SectionTitle(this.title);

  final String title;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 4),
      child: Text(
        title,
        style: const TextStyle(
          fontSize: 11,
          fontWeight: FontWeight.w600,
          letterSpacing: .4,
          color: panelMutedText,
        ),
      ),
    );
  }
}

class _ComponentTile extends StatelessWidget {
  const _ComponentTile(
    this.component, {
    super.key,
    required this.onSet,
    required this.onRemove,
  });

  final EntityComponent component;
  final void Function(String component, Object? value)? onSet;
  final ValueChanged<String>? onRemove;

  @override
  Widget build(BuildContext context) {
    final value = component.value;
    final onSet = this.onSet;
    final onRemove = this.onRemove;
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 4, 8, 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Wrap(
                  spacing: 8,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: [
                    Text(
                      component.name,
                      style: TextStyle(
                        fontSize: 12,
                        fontWeight: FontWeight.w600,
                        color: component.saved ? panelText : panelMutedText,
                      ),
                    ),
                    if (component.requiredBy case final requiredBy?)
                      _Tag('required by $requiredBy'),
                    if (!component.saved) const _Tag('computed'),
                    if (!component.mutable) const _Tag('immutable'),
                    if (value == null) const _Tag('no Reflect'),
                  ],
                ),
              ),
              if (onRemove != null && component.saved)
                PanelAction(
                  icon: Icons.close,
                  tooltip: 'Remove component',
                  onPressed: () => onRemove(component.path),
                ),
            ],
          ),
          if (value != null)
            component.editable && onSet != null
                ? ValueEditor(
                    value: value,
                    eulerRotation: component.path == _transformPath,
                    onChanged: (value) => onSet(component.path, value),
                  )
                : SelectableText(
                    _pretty(value),
                    style: const TextStyle(
                      fontFamily: 'Consolas',
                      fontSize: 11,
                      color: panelMutedText,
                    ),
                  ),
        ],
      ),
    );
  }
}

/// Structured values as indented JSON; a string, number or bool as is.
String _pretty(Object value) => value is Map || value is List
    ? const JsonEncoder.withIndent('  ').convert(value)
    : value.toString();

class _Tag extends StatelessWidget {
  const _Tag(this.text);

  final String text;

  @override
  Widget build(BuildContext context) {
    return Text(
      text,
      style: const TextStyle(fontSize: 11, color: panelMutedText),
    );
  }
}

class _SystemRow extends StatelessWidget {
  const _SystemRow(this.system);

  final GameSystem system;

  @override
  Widget build(BuildContext context) {
    final prefix = '${system.crate}::';
    final name = system.name.startsWith(prefix)
        ? system.name.substring(prefix.length)
        : system.name;
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 2, 12, 2),
      child: Text.rich(
        TextSpan(
          children: [
            TextSpan(
              text: '${system.schedule}  ',
              style: const TextStyle(color: panelMutedText),
            ),
            TextSpan(text: name),
          ],
        ),
        style: const TextStyle(fontSize: 11, color: panelText),
      ),
    );
  }
}

/// Picks a component type by name; pops with its type path.
class _AddComponentDialog extends StatefulWidget {
  const _AddComponentDialog(this.types);

  final List<ComponentType> types;

  @override
  State<_AddComponentDialog> createState() => _AddComponentDialogState();
}

class _AddComponentDialogState extends State<_AddComponentDialog> {
  var _query = '';

  @override
  Widget build(BuildContext context) {
    final query = _query.toLowerCase();
    final types = [
      for (final type in widget.types)
        if (type.path.toLowerCase().contains(query)) type,
    ];
    return AlertDialog(
      title: const Text('Add component'),
      content: SizedBox(
        width: 420,
        height: 420,
        child: Column(
          children: [
            TextField(
              autofocus: true,
              decoration: const InputDecoration(
                hintText: 'Search components',
                prefixIcon: Icon(Icons.search, size: 18),
                isDense: true,
              ),
              onChanged: (query) => setState(() => _query = query),
            ),
            const SizedBox(height: 8),
            Expanded(
              child: ListView.builder(
                itemCount: types.length,
                itemBuilder: (context, index) {
                  final type = types[index];
                  return ListTile(
                    dense: true,
                    title: Text(type.name),
                    subtitle: Text(type.crate),
                    onTap: () => Navigator.of(context).pop(type.path),
                  );
                },
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
      ],
    );
  }
}
