import 'dart:convert';

import 'package:flutter/material.dart';

import 'package:editor_ui/entities/scene/model/scene.dart';
import 'package:editor_ui/shared/ui/panel.dart';

/// The selected entity's components, and every system of the game, grouped
/// by crate with the game's own crate first.
class EntityInspectorPanel extends StatelessWidget {
  const EntityInspectorPanel({super.key, required this.scene});

  final Scene scene;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: scene,
      builder: (context, _) {
        final selected = scene.selected;
        final entity = scene.entities
            .where((entity) => entity.id == selected)
            .firstOrNull;
        return Panel(
          title: 'Inspector',
          child: ListView(
            children: [
              Padding(
                padding: const EdgeInsets.all(12),
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
              if (selected != null) ...[
                const _SectionTitle('Components'),
                ..._crateGroups(
                  scene.components,
                  (component) => component.crate,
                  scene.crate,
                  (component) => _ComponentTile(component),
                ),
              ],
              const _SectionTitle('Systems'),
              ..._crateGroups(
                scene.systems,
                (system) => system.crate,
                scene.crate,
                (system) => _SystemRow(system),
              ),
            ],
          ),
        );
      },
    );
  }
}

/// One collapsible group per crate, the game's crate first and expanded.
List<Widget> _crateGroups<T>(
  List<T> items,
  String Function(T) crateOf,
  String? gameCrate,
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
        initiallyExpanded: crate == gameCrate,
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
  const _ComponentTile(this.component);

  final EntityComponent component;

  @override
  Widget build(BuildContext context) {
    final value = component.value;
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 4, 12, 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            spacing: 8,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Text(
                component.name,
                style: const TextStyle(
                  fontSize: 12,
                  fontWeight: FontWeight.w600,
                  color: panelText,
                ),
              ),
              if (component.requiredBy case final requiredBy?)
                _Tag('required by $requiredBy'),
              if (!component.mutable) const _Tag('immutable'),
              if (value == null) const _Tag('no Reflect'),
            ],
          ),
          if (value != null)
            SelectableText(
              _pretty(value),
              style: const TextStyle(
                fontFamily: 'Consolas',
                fontSize: 11,
                color: panelText,
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
