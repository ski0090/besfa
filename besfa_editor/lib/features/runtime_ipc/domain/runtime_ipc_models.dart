/// Runtime event names understood by the editor.
enum RuntimeIpcEventKind {
  /// Runtime accepted the editor handshake.
  runtimeReady('runtime_ready'),

  /// Runtime log event.
  log('log'),

  /// Runtime scene hierarchy snapshot.
  sceneSnapshot('scene_snapshot'),

  /// Runtime frame timing telemetry.
  frameStats('frame_stats'),

  /// Runtime preview surface descriptor.
  previewSurfaceReady('preview_surface_ready'),

  /// Unknown or unsupported event name.
  unknown('');

  const RuntimeIpcEventKind(this.wireName);

  final String wireName;

  /// Converts a wire event name into an editor event kind.
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

/// Runtime event received over IPC.
class RuntimeIpcEvent {
  const RuntimeIpcEvent({required this.kind, required this.payload});

  /// Parses a runtime event JSON object.
  factory RuntimeIpcEvent.fromJson(Map<String, Object?> json) {
    return RuntimeIpcEvent(
      kind: RuntimeIpcEventKind.fromWireName(json['event']),
      payload: _asMap(json['payload']),
    );
  }

  /// Event name.
  final RuntimeIpcEventKind kind;

  /// Event-specific payload.
  final Map<String, Object?> payload;
}

/// Response to an editor command sent over runtime IPC.
class RuntimeIpcCommandResponse {
  const RuntimeIpcCommandResponse({
    required this.id,
    required this.ok,
    required this.result,
    required this.error,
  });

  /// Parses a command response JSON object.
  factory RuntimeIpcCommandResponse.fromJson(Map<String, Object?> json) {
    return RuntimeIpcCommandResponse(
      id: (json['id'] as num?)?.toInt() ?? 0,
      ok: json['ok'] == true,
      result: _asMap(json['result']),
      error: _parseError(json['error']),
    );
  }

  /// Request id from the command.
  final int id;

  /// Whether the runtime accepted the command.
  final bool ok;

  /// Optional success payload.
  final Map<String, Object?> result;

  /// Optional error payload.
  final RuntimeIpcError? error;
}

/// Error returned by a failed runtime command response.
class RuntimeIpcError {
  const RuntimeIpcError({required this.code, required this.message});

  /// Parses an IPC error JSON object.
  factory RuntimeIpcError.fromJson(Map<String, Object?> json) {
    return RuntimeIpcError(
      code: json['code'] as String? ?? 'unknown',
      message: json['message'] as String? ?? 'Unknown runtime IPC error.',
    );
  }

  /// Stable machine-readable error code.
  final String code;

  /// Human-readable error message.
  final String message;
}

/// Exception raised when a runtime command response is not successful.
class RuntimeIpcCommandException implements Exception {
  const RuntimeIpcCommandException(this.error);

  /// Error returned by the runtime.
  final RuntimeIpcError error;

  @override
  String toString() => error.message;
}

/// Runtime scene snapshot used by editor hierarchy surfaces.
class RuntimeSceneSnapshot {
  const RuntimeSceneSnapshot({
    required this.root,
    required this.selectedEntityId,
  });

  /// Parses a snapshot event payload.
  factory RuntimeSceneSnapshot.fromPayload(Map<String, Object?> payload) {
    return RuntimeSceneSnapshot(
      root: RuntimeSceneEntity.fromJson(_asMap(payload['root'])),
      selectedEntityId: payload['selected_entity_id'] as String?,
    );
  }

  /// Root node of the runtime hierarchy.
  final RuntimeSceneEntity root;

  /// Currently selected entity id, if any.
  final String? selectedEntityId;

  /// Currently selected entity node, if it exists in this snapshot.
  RuntimeSceneEntity? get selectedEntity {
    final selectedEntityId = this.selectedEntityId;
    if (selectedEntityId == null) {
      return null;
    }

    return root.findById(selectedEntityId);
  }
}

/// Entity node inside a runtime scene snapshot.
class RuntimeSceneEntity {
  const RuntimeSceneEntity({
    required this.id,
    required this.name,
    required this.kind,
    required this.children,
  });

  /// Parses an entity node JSON object.
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

  /// Stable runtime entity id.
  final String id;

  /// Display name.
  final String name;

  /// Lightweight type hint for editor UI.
  final String kind;

  /// Child entities.
  final List<RuntimeSceneEntity> children;

  /// Finds this entity or a descendant by stable runtime id.
  RuntimeSceneEntity? findById(String entityId) {
    if (id == entityId) {
      return this;
    }

    for (final child in children) {
      final found = child.findById(entityId);
      if (found != null) {
        return found;
      }
    }

    return null;
  }
}

/// Runtime frame timing telemetry.
class RuntimeFrameStats {
  const RuntimeFrameStats({required this.fps, required this.frameTimeMs});

  /// Parses a frame stats event payload.
  factory RuntimeFrameStats.fromPayload(Map<String, Object?> payload) {
    return RuntimeFrameStats(
      fps: (payload['fps'] as num?)?.toDouble() ?? 0,
      frameTimeMs: (payload['frame_time_ms'] as num?)?.toDouble() ?? 0,
    );
  }

  /// Estimated frames per second.
  final double fps;

  /// Average frame time in milliseconds.
  final double frameTimeMs;
}

/// Runtime log message shown by editor status surfaces.
class RuntimeLogEntry {
  const RuntimeLogEntry({required this.level, required this.message});

  /// Parses a log event payload.
  factory RuntimeLogEntry.fromPayload(Map<String, Object?> payload) {
    return RuntimeLogEntry(
      level: payload['level'] as String? ?? 'info',
      message: payload['message'] as String? ?? '',
    );
  }

  /// Log level string.
  final String level;

  /// Human-readable log message.
  final String message;
}

/// Runtime-owned preview surface shared with the editor.
class RuntimePreviewSurface {
  const RuntimePreviewSurface({
    required this.sharedHandleName,
    required this.width,
    required this.height,
    required this.format,
  });

  /// Parses a preview surface event payload.
  factory RuntimePreviewSurface.fromPayload(Map<String, Object?> payload) {
    return RuntimePreviewSurface(
      sharedHandleName: payload['shared_handle_name'] as String? ?? '',
      width: (payload['width'] as num?)?.toInt() ?? 0,
      height: (payload['height'] as num?)?.toInt() ?? 0,
      format: payload['format'] as String? ?? '',
    );
  }

  /// Native shared handle name opened by the editor plugin.
  final String sharedHandleName;

  /// Surface width in physical pixels.
  final int width;

  /// Surface height in physical pixels.
  final int height;

  /// Texture format name.
  final String format;
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
