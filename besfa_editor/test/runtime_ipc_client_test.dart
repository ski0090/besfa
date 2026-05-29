import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:besfa_editor/features/runtime_ipc/application/runtime_ipc_client.dart';
import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('connects and waits for runtime_ready', () async {
    final server = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
    final client = RuntimeIpcClient();
    addTearDown(client.disconnect);
    addTearDown(server.close);

    final serverDone = Completer<void>();
    server.listen((socket) {
      socket
          .cast<List<int>>()
          .transform(utf8.decoder)
          .transform(const LineSplitter())
          .listen((line) {
            final decoded = jsonDecode(line);
            if (decoded is Map<String, Object?> &&
                decoded['type'] == 'hello' &&
                decoded['token'] == 42) {
              socket.write(
                '${jsonEncode({
                  'type': 'event',
                  'event': 'runtime_ready',
                  'payload': {'protocol_version': runtimeIpcProtocolVersion},
                })}\n',
              );
              serverDone.complete();
            }
          });
    });

    await client.connectAndWaitReady(
      RuntimeIpcHandshake(port: server.port, token: 42),
    );
    await serverDone.future;
  });

  test('receives runtime events and routes command responses', () async {
    final server = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
    final client = RuntimeIpcClient();
    addTearDown(client.disconnect);
    addTearDown(server.close);

    final command = Completer<Map<String, Object?>>();
    server.listen((socket) {
      socket
          .cast<List<int>>()
          .transform(utf8.decoder)
          .transform(const LineSplitter())
          .listen((line) {
            final decoded = _asMap(jsonDecode(line));
            if (decoded['type'] == 'hello') {
              socket.write(
                '${jsonEncode({
                  'type': 'event',
                  'event': 'runtime_ready',
                  'payload': {'protocol_version': runtimeIpcProtocolVersion},
                })}\n',
              );
              socket.write(
                '${jsonEncode({
                  'type': 'event',
                  'event': 'scene_snapshot',
                  'payload': {
                    'root': {'id': 'world', 'name': 'World', 'kind': 'world', 'children': <Object?>[]},
                  },
                })}\n',
              );
            } else if (decoded['type'] == 'command') {
              command.complete(decoded);
              socket.write(
                '${jsonEncode({'type': 'response', 'id': decoded['id'], 'ok': true, 'result': <String, Object?>{}})}\n',
              );
            }
          });
    });

    final snapshotEvent = client.events.firstWhere(
      (event) => event.kind == RuntimeIpcEventKind.sceneSnapshot,
    );

    await client.connectAndWaitReady(
      RuntimeIpcHandshake(port: server.port, token: 42),
    );
    await client.reloadScene();

    expect((await command.future)['method'], runtimeIpcReloadSceneMethod);
    expect((await snapshotEvent).payload['root'], isA<Map<String, Object?>>());
  });
}

Map<String, Object?> _asMap(Object? value) {
  if (value is Map) {
    return value.map((key, value) => MapEntry(key.toString(), value));
  }

  return const {};
}
