import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';

const int runtimeIpcProtocolVersion = 1;
const String runtimeIpcOpenProjectMethod = 'open_project';
const String runtimeIpcReloadSceneMethod = 'reload_scene';
const String runtimeIpcSelectEntityMethod = 'select_entity';
const String runtimeIpcCreateEntityMethod = 'create_entity';
const String runtimeIpcSetTransformMethod = 'set_transform';

/// Port and token reserved by the editor before launching the runtime.
class RuntimeIpcHandshake {
  const RuntimeIpcHandshake({required this.port, required this.token});

  /// Localhost port the runtime should bind.
  final int port;

  /// Random token the runtime must validate during hello.
  final int token;
}

/// TCP client for the Besfa editor-to-runtime IPC protocol.
class RuntimeIpcClient {
  final Random _random = Random.secure();
  final StreamController<RuntimeIpcEvent> _events =
      StreamController<RuntimeIpcEvent>.broadcast();
  final Map<int, Completer<RuntimeIpcCommandResponse>> _pendingResponses = {};
  int _nextCommandId = 1;
  Socket? _socket;
  StreamSubscription<String>? _subscription;

  /// Stream of runtime events pushed after the handshake completes.
  Stream<RuntimeIpcEvent> get events => _events.stream;

  /// Reserves a free localhost port and creates a handshake token.
  Future<RuntimeIpcHandshake> reserveHandshake() async {
    final server = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
    final port = server.port;
    await server.close();

    return RuntimeIpcHandshake(port: port, token: _nextToken());
  }

  /// Connects to the launched runtime and waits for `runtime_ready`.
  Future<void> connectAndWaitReady(
    RuntimeIpcHandshake handshake, {
    Duration timeout = const Duration(seconds: 5),
  }) async {
    await disconnect();

    final deadline = DateTime.now().add(timeout);
    while (true) {
      try {
        final socket = await Socket.connect(
          InternetAddress.loopbackIPv4,
          handshake.port,
          timeout: const Duration(milliseconds: 250),
        );
        _socket = socket;
        _sendHello(socket, handshake);
        await _listenAndWaitForReady(socket, deadline);
        return;
      } on Object {
        await disconnect();
        if (DateTime.now().isAfter(deadline)) {
          throw TimeoutException('Runtime IPC did not become ready.', timeout);
        }
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
    }
  }

  /// Closes the socket and fails any pending command responses.
  Future<void> disconnect() async {
    await _subscription?.cancel();
    _subscription = null;
    _completePendingResponses(
      StateError('Runtime IPC disconnected before response.'),
    );

    final socket = _socket;
    _socket = null;
    if (socket != null) {
      await socket.close();
    }
  }

  int _nextToken() {
    return 1 + _random.nextInt(0x7ffffffe);
  }

  /// Sends a raw runtime command and waits for its response.
  Future<RuntimeIpcCommandResponse> sendCommand(
    String method, {
    Map<String, Object?> params = const {},
    Duration timeout = const Duration(seconds: 5),
  }) async {
    final socket = _socket;
    if (socket == null) {
      throw StateError('Runtime IPC is not connected.');
    }

    final id = _nextCommandId++;
    final completer = Completer<RuntimeIpcCommandResponse>();
    _pendingResponses[id] = completer;

    socket.write(
      '${jsonEncode({'type': 'command', 'id': id, 'method': method, 'params': params})}\n',
    );

    try {
      final response = await completer.future.timeout(timeout);
      if (!response.ok) {
        throw RuntimeIpcCommandException(
          response.error ??
              const RuntimeIpcError(
                code: 'unknown',
                message: 'Runtime command failed.',
              ),
        );
      }
      return response;
    } finally {
      _pendingResponses.remove(id);
    }
  }

  /// Sends `open_project` to the runtime.
  Future<void> openProject(String path) async {
    await sendCommand(runtimeIpcOpenProjectMethod, params: {'path': path});
  }

  /// Sends `reload_scene` to the runtime.
  Future<void> reloadScene() async {
    await sendCommand(runtimeIpcReloadSceneMethod);
  }

  /// Sends `select_entity` to the runtime.
  Future<void> selectEntity(String entityId) async {
    await sendCommand(
      runtimeIpcSelectEntityMethod,
      params: {'entity_id': entityId},
    );
  }

  /// Sends `create_entity` to the runtime and returns the new entity id.
  Future<String?> createEntity({
    required String kind,
    String? name,
    String? parentEntityId,
  }) async {
    final params = <String, Object?>{'kind': kind};
    if (name != null) {
      params['name'] = name;
    }
    if (parentEntityId != null) {
      params['parent_entity_id'] = parentEntityId;
    }

    final response = await sendCommand(
      runtimeIpcCreateEntityMethod,
      params: params,
    );
    return response.result['entity_id'] as String?;
  }

  /// Sends `set_transform` to update a runtime entity translation.
  Future<void> setTransform({
    required String entityId,
    required RuntimeVector3 translation,
  }) async {
    await sendCommand(
      runtimeIpcSetTransformMethod,
      params: {'entity_id': entityId, 'translation': translation.toPayload()},
    );
  }

  void _sendHello(Socket socket, RuntimeIpcHandshake handshake) {
    socket.write(
      '${jsonEncode({'type': 'hello', 'protocol_version': runtimeIpcProtocolVersion, 'token': handshake.token})}\n',
    );
  }

  Future<void> _listenAndWaitForReady(Socket socket, DateTime deadline) async {
    final completer = Completer<void>();
    _subscription = socket
        .cast<List<int>>()
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .listen(
          (line) {
            final event = _handleLine(line);
            if (event?.kind == RuntimeIpcEventKind.runtimeReady &&
                !completer.isCompleted) {
              completer.complete();
            }
          },
          onError: (Object error, StackTrace stackTrace) {
            if (!completer.isCompleted) {
              completer.completeError(error, stackTrace);
            }
          },
          onDone: () {
            _completePendingResponses(
              StateError('Runtime IPC closed before response.'),
            );
            if (!completer.isCompleted) {
              completer.completeError(
                StateError('Runtime IPC closed before ready.'),
              );
            }
          },
        );

    final remaining = deadline.difference(DateTime.now());
    if (remaining <= Duration.zero) {
      throw TimeoutException('Runtime IPC did not become ready.');
    }

    await completer.future.timeout(remaining);
  }

  RuntimeIpcEvent? _handleLine(String line) {
    final Object? decoded;
    try {
      decoded = jsonDecode(line);
    } on FormatException {
      return null;
    }

    final decodedMap = _asJsonMap(decoded);
    if (decodedMap.isEmpty) {
      return null;
    }

    switch (decodedMap['type']) {
      case 'event':
        final event = RuntimeIpcEvent.fromJson(decodedMap);
        _events.add(event);
        return event;
      case 'response':
        final response = RuntimeIpcCommandResponse.fromJson(decodedMap);
        _pendingResponses.remove(response.id)?.complete(response);
      default:
        return null;
    }

    return null;
  }

  void _completePendingResponses(Object error) {
    for (final completer in _pendingResponses.values) {
      if (!completer.isCompleted) {
        completer.completeError(error);
      }
    }
    _pendingResponses.clear();
  }
}

Map<String, Object?> _asJsonMap(Object? value) {
  if (value is Map) {
    return value.map((key, value) => MapEntry(key.toString(), value));
  }

  return const {};
}
