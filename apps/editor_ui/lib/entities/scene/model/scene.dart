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
  const SceneEntity({required this.id, this.name, this.parent});

  factory SceneEntity.fromJson(Map<String, Object?> json) => SceneEntity(
    id: json['id'] as int,
    name: json['name'] as String?,
    parent: json['parent'] as int?,
  );

  /// Bevy's entity bits; what `select <id>` takes.
  final int id;
  final String? name;
  final int? parent;

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
    this.value,
  });

  factory EntityComponent.fromJson(Map<String, Object?> json) =>
      EntityComponent(
        name: json['name'] as String,
        path: json['path'] as String,
        mutable: json['mutable'] as bool,
        requiredBy: json['required_by'] as String?,
        value: json['value'],
      );

  final String name;
  final String path;
  final bool mutable;

  /// The component on the same entity that required this one, if any.
  final String? requiredBy;

  /// The reflected value as decoded JSON; null for types without `Reflect`.
  final Object? value;

  String get crate => crateOf(path);
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

/// What the running game reports about its world, read from its output.
class Scene extends ChangeNotifier {
  List<SceneEntity> entities = const [];

  /// The entity whose components are shown; the game reports them.
  int? selected;
  List<EntityComponent> components = const [];
  List<GameSystem> systems = const [];

  /// The game's own crate, listed before Bevy's.
  String? crate;

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
    }
  }

  void select(int? id) {
    selected = id;
    components = const [];
    notifyListeners();
  }

  /// A new game process: what the old one reported no longer holds. The
  /// selection stays, so the same entity is picked up after a Stop.
  void reset() {
    entities = const [];
    components = const [];
    systems = const [];
    crate = null;
    notifyListeners();
  }
}
