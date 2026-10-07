import 'dart:convert';

import 'package:flutter/material.dart';

import 'package:editor_ui/entities/scene/model/scene.dart';
import 'package:editor_ui/shared/ui/panel.dart';
import 'package:editor_ui/widgets/entity_inspector/ui/value_editor.dart';

const _transformPath = 'bevy_transform::components::transform::Transform';
const _namePath = 'bevy_ecs::name::Name';

/// Fields Bevy recomputes while the game runs, out of the simple view.
// ponytail: the template's camera; add types as they show more.
const _runtimeFields = {
  'bevy_camera::camera::Camera': {'computed'},
};

/// The selected entity and the game's systems. Simply, what there is to
/// edit: the name, the components the scene file keeps with Transform
/// first, the game's own components, and the game's own systems. In
/// detail, every component and system grouped by crate, with what Bevy
/// computes. With the callbacks, component values become fields and
/// components can be added and removed.
class EntityInspectorPanel extends StatefulWidget {
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
  State<EntityInspectorPanel> createState() => _EntityInspectorPanelState();
}

class _EntityInspectorPanelState extends State<EntityInspectorPanel> {
  bool _details = false;

  Scene get scene => widget.scene;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: scene,
      builder: (context, _) {
        final selected = scene.selected;
        final entity = scene.selectedEntity;
        final onSet = widget.onSet;
        // Simply, the name is edited in the header.
        final name = _details || onSet == null
            ? null
            : scene.components
                  .where((c) => c.path == _namePath && c.editable)
                  .firstOrNull;
        Widget tile(EntityComponent component) => _ComponentTile(
          component,
          // A new entity starts with fresh fields.
          key: ValueKey('$selected/${component.path}'),
          details: _details,
          onSet: onSet,
          onRemove: widget.onRemove,
        );
        return Panel(
          title: 'Inspector',
          actions: [
            PanelAction(
              icon: _details ? Icons.unfold_less : Icons.unfold_more,
              tooltip: _details ? 'Hide details' : 'Show details',
              onPressed: () => setState(() => _details = !_details),
            ),
          ],
          child: ListView(
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(12, 12, 12, 4),
                child: name != null
                    ? ValueEditor(
                        key: ValueKey('$selected/name'),
                        value: name.value,
                        onChanged: (value) => onSet!(_namePath, value),
                      )
                    : Text(
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
                if (_details)
                  ..._crateGroups(
                    scene.components,
                    (component) => component.crate,
                    scene.crate,
                    // Groups that hold something the scene file keeps.
                    (components) =>
                        components.any((component) => component.saved),
                    tile,
                  )
                else
                  for (final component in _editable(name)) tile(component),
                if (widget.onInsert != null && entity != null)
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
              if (_details) ...[
                const _SectionTitle('Systems'),
                ..._crateGroups(
                  scene.systems,
                  (system) => system.crate,
                  scene.crate,
                  (_) => false,
                  (system) => _SystemRow(system),
                ),
              ] else if (_gameSystems case final systems
                  when systems.isNotEmpty) ...[
                const _SectionTitle('Systems'),
                for (final system in systems) _SystemRow(system),
              ],
            ],
          ),
        );
      },
    );
  }

  /// What there is to edit, Transform first: the components the scene
  /// file keeps, and the game's own, where an unreflected one is tagged
  /// so. The [name] edited in the header is left out.
  List<EntityComponent> _editable(EntityComponent? name) {
    final shown = [
      for (final component in scene.components)
        if (component != name &&
            (component.saved && component.value != null ||
                component.crate == scene.crate))
          component,
    ];
    return [
      ...shown.where((component) => component.path == _transformPath),
      ...shown.where((component) => component.path != _transformPath),
    ];
  }

  /// The game's own systems; Bevy's are in the details.
  List<GameSystem> get _gameSystems => [
    for (final system in scene.systems)
      if (system.crate == scene.crate) system,
  ];

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
          // The scene placed in itself is saved with Save scene.
          placed.asset != 'scenes/main.scn.ron' &&
          widget.onApplyPrefab != null)
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12),
          child: Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              icon: const Icon(Icons.upload_file, size: 16),
              label: const Text('Apply to prefab'),
              onPressed: () => widget.onApplyPrefab!(placed.id),
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
      widget.onInsert!(path);
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
    required this.details,
    required this.onSet,
    required this.onRemove,
  });

  final EntityComponent component;

  /// With tags for what Bevy computes or requires, and every field.
  final bool details;
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
                    if (details) ...[
                      if (component.requiredBy case final requiredBy?)
                        _Tag('required by $requiredBy'),
                      if (!component.saved) const _Tag('computed'),
                      if (!component.mutable) const _Tag('immutable'),
                    ],
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
                    hidden: details
                        ? const {}
                        : _runtimeFields[component.path] ?? const {},
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
