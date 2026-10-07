import 'dart:convert';

import 'package:flutter/foundation.dart';

/// A game output line that is a report rather than a log starts with this;
/// a JSON object with a `type` follows. Written by `besfa_editor_plugin`.
const _reportPrefix = '@besfa ';

/// The crate a Rust type path starts with: `bevy_transform::...::Transform`
/// is in `bevy_transform`. Trait method names open with `<`, as in
/// `<bevy_a::B as C>::run`.
String crateOf(String path) {
  final start = path.startsWith('<') ? 1 : 0;
  final end = path.indexOf('::', start);
  return end < 0 ? path.substring(start) : path.substring(start, end);
}

class SceneEntity {
  const SceneEntity({
    required this.id,
    this.name,
    this.parent,
    this.scene = true,
  });

  factory SceneEntity.fromJson(Map<String, Object?> json) => SceneEntity(
    id: json['id'] as int,
    name: json['name'] as String?,
    parent: json['parent'] as int?,
    scene: json['scene'] as bool? ?? true,
  );

  /// Bevy's entity bits; what `select` takes.
  final int id;
  final String? name;
  final int? parent;

  /// From the scene file or the editor, so saving keeps it. Entities the
  /// game's code spawns while it runs are not saved.
  final bool scene;

  /// Unnamed entities are shown the way Bevy prints them: `4v0`, the index
  /// (stored inverted in the low bits) and the generation.
  String get label => name ?? 'Entity ${~id & 0xFFFFFFFF}v${id >>> 32}';
}

class EntityComponent {
  const EntityComponent({
    required this.name,
    required this.path,
    required this.mutable,
    this.requiredBy,
    this.saved = true,
    this.value,
  });

  factory EntityComponent.fromJson(Map<String, Object?> json) =>
      EntityComponent(
        name: json['name'] as String,
        path: json['path'] as String,
        mutable: json['mutable'] as bool,
        requiredBy: json['required_by'] as String?,
        saved: json['saved'] as bool? ?? true,
        value: json['value'],
      );

  final String name;

  /// The reflected type path, which commands name the component by.
  final String path;
  final bool mutable;

  /// The component on the same entity that required this one, if any.
  final String? requiredBy;

  /// False for components computed from others, which the scene file
  /// leaves out and the editor shows read-only.
  final bool saved;

  /// The reflected value as decoded JSON; null for types without `Reflect`.
  final Object? value;

  String get crate => crateOf(path);

  /// The editor can change it: reflected, mutable, and not computed.
  bool get editable => saved && mutable && value != null;
}

class GameSystem {
  const GameSystem({required this.schedule, required this.name});

  factory GameSystem.fromJson(Map<String, Object?> json) => GameSystem(
    schedule: json['schedule'] as String,
    name: json['name'] as String,
  );

  final String schedule;
  final String name;

  String get crate => crateOf(name);
}

/// A component type the editor can add to an entity.
class ComponentType {
  const ComponentType({required this.name, required this.path});

  factory ComponentType.fromJson(Map<String, Object?> json) =>
      ComponentType(name: json['name'] as String, path: json['path'] as String);

  final String name;
  final String path;

  String get crate => crateOf(path);
}

/// What the running game reports about its world, read from its output,
/// and whether the editor changed it since the last save.
class Scene extends ChangeNotifier {
  List<SceneEntity> entities = const [];

  /// The entity whose components are shown; the game reports them.
  int? selected;
  List<EntityComponent> components = const [];
  List<GameSystem> systems = const [];

  /// Components the game can add with a default value.
  List<ComponentType> addable = const [];

  /// The game's own crate, listed before Bevy's.
  String? crate;

  /// The editor changed the scene since it was loaded or last saved.
  bool dirty = false;

  SceneEntity? get selectedEntity =>
      entities.where((entity) => entity.id == selected).firstOrNull;

  /// Takes [line] if it is a report. Logs, and lines that only look like a
  /// report, are left for the log panel.
  bool handle(String line) {
    if (!line.startsWith(_reportPrefix)) {
      return false;
    }
    try {
      _apply(
        jsonDecode(line.substring(_reportPrefix.length))
            as Map<String, Object?>,
      );
    } catch (_) {
      // Game output is not trusted to be well formed.
      return false;
    }
    notifyListeners();
    return true;
  }

  void _apply(Map<String, Object?> report) {
    switch (report['type']) {
      case 'entities':
        entities = [
          for (final entity in report['entities'] as List)
            SceneEntity.fromJson(entity as Map<String, Object?>),
        ];
      case 'entity':
        // A report for an earlier selection may still be in the pipe.
        if (report['id'] == selected) {
          components = [
            for (final component in report['components'] as List)
              EntityComponent.fromJson(component as Map<String, Object?>),
          ];
        }
      case 'systems':
        crate = report['crate'] as String?;
        systems = [
          for (final system in report['systems'] as List)
            GameSystem.fromJson(system as Map<String, Object?>),
        ];
      case 'components':
        addable = [
          for (final type in report['components'] as List)
            ComponentType.fromJson(type as Map<String, Object?>),
        ];
      // The game picked an entity itself: a new one, or none once the
      // selected one is gone.
      case 'selected':
        _select(report['id'] as int?);
      case 'saved':
        if (report['error'] == null) {
          dirty = false;
        }
    }
  }

  void select(int? id) {
    _select(id);
    notifyListeners();
  }

  void _select(int? id) {
    if (id != selected) {
      selected = id;
      components = const [];
    }
  }

  void markDirty() {
    if (!dirty) {
      dirty = true;
      notifyListeners();
    }
  }

  /// A new game process: what the old one reported no longer holds, and it
  /// starts from the scene file. The selection stays, so the same entity is
  /// picked up after a Stop.
  void reset() {
    entities = const [];
    components = const [];
    systems = const [];
    addable = const [];
    crate = null;
    dirty = false;
    notifyListeners();
  }
}
