enum RuntimeIpcEventKind {
  runtimeReady('runtime_ready'),
  log('log'),
  sceneSnapshot('scene_snapshot'),
  frameStats('frame_stats'),
  unknown('');

  const RuntimeIpcEventKind(this.wireName);

  final String wireName;

  static RuntimeIpcEventKind fromWireName(Object? value) {
    if (value is! String) {
      return RuntimeIpcEventKind.unknown;
    }

    for (final kind in RuntimeIpcEventKind.values) {
      if (kind.wireName == value) {
        return kind;
      }
    }

    return RuntimeIpcEventKind.unknown;
  }
}

class RuntimeIpcEvent {
  const RuntimeIpcEvent({required this.kind, required this.payload});

  factory RuntimeIpcEvent.fromJson(Map<String, Object?> json) {
    return RuntimeIpcEvent(
      kind: RuntimeIpcEventKind.fromWireName(json['event']),
      payload: _asMap(json['payload']),
    );
  }

  final RuntimeIpcEventKind kind;
  final Map<String, Object?> payload;
}

class RuntimeIpcCommandResponse {
  const RuntimeIpcCommandResponse({
    required this.id,
    required this.ok,
    required this.result,
    required this.error,
  });

  factory RuntimeIpcCommandResponse.fromJson(Map<String, Object?> json) {
    return RuntimeIpcCommandResponse(
      id: (json['id'] as num?)?.toInt() ?? 0,
      ok: json['ok'] == true,
      result: _asMap(json['result']),
      error: _parseError(json['error']),
    );
  }

  final int id;
  final bool ok;
  final Map<String, Object?> result;
  final RuntimeIpcError? error;
}

class RuntimeIpcError {
  const RuntimeIpcError({required this.code, required this.message});

  factory RuntimeIpcError.fromJson(Map<String, Object?> json) {
    return RuntimeIpcError(
      code: json['code'] as String? ?? 'unknown',
      message: json['message'] as String? ?? 'Unknown runtime IPC error.',
    );
  }

  final String code;
  final String message;
}

class RuntimeIpcCommandException implements Exception {
  const RuntimeIpcCommandException(this.error);

  final RuntimeIpcError error;

  @override
  String toString() => error.message;
}

class RuntimeSceneSnapshot {
  const RuntimeSceneSnapshot({
    required this.root,
    required this.selectedEntityId,
  });

  factory RuntimeSceneSnapshot.fromPayload(Map<String, Object?> payload) {
    return RuntimeSceneSnapshot(
      root: RuntimeSceneEntity.fromJson(_asMap(payload['root'])),
      selectedEntityId: payload['selected_entity_id'] as String?,
    );
  }

  final RuntimeSceneEntity root;
  final String? selectedEntityId;
}

class RuntimeSceneEntity {
  const RuntimeSceneEntity({
    required this.id,
    required this.name,
    required this.kind,
    required this.children,
  });

  factory RuntimeSceneEntity.fromJson(Map<String, Object?> json) {
    final children = switch (json['children']) {
      final List<Object?> items =>
        items
            .map(_asMap)
            .where((child) => child.isNotEmpty)
            .map(RuntimeSceneEntity.fromJson)
            .toList(growable: false),
      _ => const <RuntimeSceneEntity>[],
    };

    return RuntimeSceneEntity(
      id: json['id'] as String? ?? '',
      name: json['name'] as String? ?? 'Entity',
      kind: json['kind'] as String? ?? 'entity',
      children: children,
    );
  }

  final String id;
  final String name;
  final String kind;
  final List<RuntimeSceneEntity> children;
}

class RuntimeFrameStats {
  const RuntimeFrameStats({required this.fps, required this.frameTimeMs});

  factory RuntimeFrameStats.fromPayload(Map<String, Object?> payload) {
    return RuntimeFrameStats(
      fps: (payload['fps'] as num?)?.toDouble() ?? 0,
      frameTimeMs: (payload['frame_time_ms'] as num?)?.toDouble() ?? 0,
    );
  }

  final double fps;
  final double frameTimeMs;
}

class RuntimeLogEntry {
  const RuntimeLogEntry({required this.level, required this.message});

  factory RuntimeLogEntry.fromPayload(Map<String, Object?> payload) {
    return RuntimeLogEntry(
      level: payload['level'] as String? ?? 'info',
      message: payload['message'] as String? ?? '',
    );
  }

  final String level;
  final String message;
}

RuntimeIpcError? _parseError(Object? value) {
  final error = _asMap(value);
  if (error.isEmpty) {
    return null;
  }

  return RuntimeIpcError.fromJson(error);
}

Map<String, Object?> _asMap(Object? value) {
  if (value is Map) {
    return value.map((key, value) => MapEntry(key.toString(), value));
  }

  return const {};
}
