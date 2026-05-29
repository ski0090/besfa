import 'dart:async';

import 'package:besfa_editor/features/runtime_ipc/application/runtime_ipc_client.dart';
import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/features/runtime_preview/application/runtime_preview_controller.dart';
import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:besfa_flutter_plugin/besfa_flutter_plugin.dart';
import 'package:flutter_test/flutter_test.dart';

class FakeBesfaFlutterPlugin extends BesfaFlutterPlugin {
  BesfaRuntimeState state = BesfaRuntimeState.stopped;
  BesfaRuntimeErrorCode error = BesfaRuntimeErrorCode.none;
  BesfaRuntimeCommandResult startResult = BesfaRuntimeCommandResult.ok;
  BesfaRuntimeCommandResult stopResult = BesfaRuntimeCommandResult.ok;
  int? createdTextureId;

  @override
  Future<String?> getPlatformVersion() => Future.value('fake platform');

  @override
  int get abiVersion => 1;

  @override
  BesfaRuntimeCommandResult startRuntime() {
    if (startResult == BesfaRuntimeCommandResult.ok) {
      state = BesfaRuntimeState.running;
      error = BesfaRuntimeErrorCode.none;
    }
    return startResult;
  }

  @override
  BesfaRuntimeCommandResult startRuntimeWithIpc({
    required int port,
    required int token,
  }) {
    return startRuntime();
  }

  @override
  BesfaRuntimeCommandResult stopRuntime() {
    if (stopResult == BesfaRuntimeCommandResult.ok) {
      state = BesfaRuntimeState.stopped;
      error = BesfaRuntimeErrorCode.none;
    }
    return stopResult;
  }

  @override
  BesfaRuntimeState get runtimeState => state;

  @override
  BesfaRuntimeErrorCode get runtimeLastError => error;

  @override
  Future<int?> createPreviewTexture({int width = 640, int height = 360}) async {
    createdTextureId = 11;
    return createdTextureId;
  }

  @override
  Future<bool> disposePreviewTexture(int textureId) async {
    if (createdTextureId == textureId) {
      createdTextureId = null;
      return true;
    }
    return false;
  }
}

class FakeRuntimeIpcClient extends RuntimeIpcClient {
  final StreamController<RuntimeIpcEvent> _events =
      StreamController<RuntimeIpcEvent>.broadcast();

  @override
  Stream<RuntimeIpcEvent> get events => _events.stream;

  void emit(RuntimeIpcEvent event) {
    _events.add(event);
  }

  Future<void> close() async {
    await _events.close();
  }

  @override
  Future<RuntimeIpcHandshake> reserveHandshake() async {
    return const RuntimeIpcHandshake(port: 49152, token: 42);
  }

  @override
  Future<void> connectAndWaitReady(
    RuntimeIpcHandshake handshake, {
    Duration timeout = const Duration(seconds: 5),
  }) async {}

  @override
  Future<void> disconnect() async {}

  @override
  Future<void> reloadScene() async {}

  @override
  Future<void> selectEntity(String entityId) async {}
}

void main() {
  test('detects when a running preview exits outside the editor', () {
    final plugin = FakeBesfaFlutterPlugin()..state = BesfaRuntimeState.running;
    final controller = RuntimePreviewController(plugin: plugin);
    addTearDown(controller.dispose);

    expect(controller.status, RuntimePreviewStatus.running);

    plugin.state = BesfaRuntimeState.exited;
    controller.refreshRuntimeStatus();

    expect(controller.status, RuntimePreviewStatus.stopped);
    expect(controller.message, 'Preview window closed.');
  });

  test('reports start failures from the native runtime bridge', () async {
    final plugin = FakeBesfaFlutterPlugin()
      ..startResult = BesfaRuntimeCommandResult.failed
      ..error = BesfaRuntimeErrorCode.executableNotFound;
    final controller = RuntimePreviewController(plugin: plugin);
    addTearDown(controller.dispose);

    await controller.runPreview();

    expect(controller.status, RuntimePreviewStatus.failed);
    expect(controller.message, 'Runtime executable was not found.');
  });

  test('updates scene snapshot from runtime IPC events', () async {
    final plugin = FakeBesfaFlutterPlugin();
    final ipcClient = FakeRuntimeIpcClient();
    final controller = RuntimePreviewController(
      plugin: plugin,
      ipcClient: ipcClient,
    );
    addTearDown(controller.dispose);
    addTearDown(ipcClient.close);

    await controller.runPreview();
    ipcClient.emit(
      RuntimeIpcEvent(
        kind: RuntimeIpcEventKind.sceneSnapshot,
        payload: {
          'root': {
            'id': 'world',
            'name': 'World',
            'kind': 'world',
            'children': <Object?>[],
          },
        },
      ),
    );
    await Future<void>.delayed(Duration.zero);

    expect(controller.status, RuntimePreviewStatus.running);
    expect(controller.sceneSnapshot?.root.name, 'World');
  });
}
