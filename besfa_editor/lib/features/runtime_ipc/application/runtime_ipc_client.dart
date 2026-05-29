import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

const int runtimeIpcProtocolVersion = 1;

class RuntimeIpcHandshake {
  const RuntimeIpcHandshake({required this.port, required this.token});

  final int port;
  final int token;
}

class RuntimeIpcClient {
  final Random _random = Random.secure();
  Socket? _socket;
  StreamSubscription<String>? _subscription;

  Future<RuntimeIpcHandshake> reserveHandshake() async {
    final server = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
    final port = server.port;
    await server.close();

    return RuntimeIpcHandshake(port: port, token: _nextToken());
  }

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
        await _waitForReady(socket, deadline);
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

  Future<void> disconnect() async {
    await _subscription?.cancel();
    _subscription = null;

    final socket = _socket;
    _socket = null;
    if (socket != null) {
      await socket.close();
    }
  }

  int _nextToken() {
    return 1 + _random.nextInt(0x7ffffffe);
  }

  void _sendHello(Socket socket, RuntimeIpcHandshake handshake) {
    socket.write(
      '${jsonEncode({'type': 'hello', 'protocol_version': runtimeIpcProtocolVersion, 'token': handshake.token})}\n',
    );
  }

  Future<void> _waitForReady(Socket socket, DateTime deadline) async {
    final completer = Completer<void>();
    _subscription = socket
        .cast<List<int>>()
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .listen(
          (line) {
            if (_isRuntimeReady(line) && !completer.isCompleted) {
              completer.complete();
            }
          },
          onError: (Object error, StackTrace stackTrace) {
            if (!completer.isCompleted) {
              completer.completeError(error, stackTrace);
            }
          },
          onDone: () {
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

  bool _isRuntimeReady(String line) {
    final decoded = jsonDecode(line);
    return decoded is Map<String, Object?> &&
        decoded['type'] == 'event' &&
        decoded['event'] == 'runtime_ready';
  }
}
