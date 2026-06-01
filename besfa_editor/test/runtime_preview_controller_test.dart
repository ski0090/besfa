import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/features/runtime_preview/application/runtime_preview_controller.dart';
import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:besfa_flutter_plugin/besfa_flutter_plugin.dart';
import 'package:flutter_test/flutter_test.dart';

import 'helpers/runtime_preview_fakes.dart';

void main() {
  test(
    'restarts when a running scene runtime exits outside the editor',
    () async {
      final plugin = FakeBesfaFlutterPlugin()
        ..state = BesfaRuntimeState.running;
      final ipcClient = FakeRuntimeIpcClient();
      final controller = RuntimePreviewController(
        plugin: plugin,
        ipcClient: ipcClient,
      );
      addTearDown(controller.dispose);
      addTearDown(ipcClient.close);

      expect(controller.status, RuntimePreviewStatus.running);

      plugin.state = BesfaRuntimeState.exited;
      controller.refreshRuntimeStatus();

      expect(controller.status, RuntimePreviewStatus.starting);
      expect(controller.message, 'Scene runtime closed; restarting.');

      await Future<void>.delayed(Duration.zero);

      expect(controller.status, RuntimePreviewStatus.running);
      expect(plugin.state, BesfaRuntimeState.running);
    },
  );

  test('stops runtime when controller is disposed', () {
    final plugin = FakeBesfaFlutterPlugin()..state = BesfaRuntimeState.running;
    final controller = RuntimePreviewController(plugin: plugin);

    controller.dispose();

    expect(plugin.stopCalls, 1);
    expect(plugin.state, BesfaRuntimeState.stopped);
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

  test('creates cubes through runtime IPC', () async {
    final plugin = FakeBesfaFlutterPlugin();
    final ipcClient = FakeRuntimeIpcClient();
    final controller = RuntimePreviewController(
      plugin: plugin,
      ipcClient: ipcClient,
    );
    addTearDown(controller.dispose);
    addTearDown(ipcClient.close);

    await controller.ensureRuntimeReady();
    await controller.createCube();

    expect(controller.status, RuntimePreviewStatus.running);
    expect(ipcClient.createEntityCalls, 1);
  });

  test('attaches runtime preview surface events', () async {
    final plugin = FakeBesfaFlutterPlugin();
    final ipcClient = FakeRuntimeIpcClient();
    final controller = RuntimePreviewController(
      plugin: plugin,
      ipcClient: ipcClient,
    );
    addTearDown(controller.dispose);
    addTearDown(ipcClient.close);

    ipcClient.emit(
      const RuntimeIpcEvent(
        kind: RuntimeIpcEventKind.previewSurfaceReady,
        payload: {
          'shared_handle_name': 'Local\\BesfaPreviewSurface-42',
          'width': 640,
          'height': 360,
          'format': 'bgra8_unorm',
        },
      ),
    );
    await Future<void>.delayed(Duration.zero);

    expect(controller.previewTextureId, 13);
  });
}
