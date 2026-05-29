import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:besfa_editor/features/runtime_ipc/application/runtime_ipc_client.dart';
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
}
