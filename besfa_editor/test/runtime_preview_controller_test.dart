import 'dart:io';

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
          'selected_entity_id': 'world',
          'root': {
            'id': 'world',
            'name': 'World',
            'kind': 'world',
            'transform': {
              'translation': {'x': 0, 'y': 1, 'z': 2},
            },
            'children': <Object?>[],
          },
        },
      ),
    );
    await Future<void>.delayed(Duration.zero);

    expect(controller.status, RuntimePreviewStatus.running);
    expect(controller.sceneSnapshot?.root.name, 'World');
    expect(controller.sceneSnapshot?.selectedEntity?.name, 'World');
    expect(
      controller.sceneSnapshot?.selectedEntity?.transform?.translation.y,
      1,
    );
  });

  test('updates selected entity translation through runtime IPC', () async {
    final plugin = FakeBesfaFlutterPlugin();
    final ipcClient = FakeRuntimeIpcClient();
    final controller = RuntimePreviewController(
      plugin: plugin,
      ipcClient: ipcClient,
    );
    addTearDown(controller.dispose);
    addTearDown(ipcClient.close);

    await controller.ensureRuntimeReady();
    ipcClient.emit(
      const RuntimeIpcEvent(
        kind: RuntimeIpcEventKind.sceneSnapshot,
        payload: {
          'selected_entity_id': 'cube_1',
          'root': {
            'id': 'world',
            'name': 'World',
            'kind': 'world',
            'children': [
              {
                'id': 'cube_1',
                'name': 'Cube 1',
                'kind': 'mesh',
                'transform': {
                  'translation': {'x': 0, 'y': 0, 'z': 0},
                },
                'children': <Object?>[],
              },
            ],
          },
        },
      ),
    );
    await Future<void>.delayed(Duration.zero);

    await controller.setSelectedEntityTranslation(
      const RuntimeVector3(x: 1, y: 2, z: 3),
    );

    expect(ipcClient.lastTranslation?.x, 1);
    expect(ipcClient.lastTranslation?.y, 2);
    expect(ipcClient.lastTranslation?.z, 3);
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

  test('picks entities from normalized viewport coordinates', () async {
    final plugin = FakeBesfaFlutterPlugin();
    final ipcClient = FakeRuntimeIpcClient();
    final controller = RuntimePreviewController(
      plugin: plugin,
      ipcClient: ipcClient,
    );
    addTearDown(controller.dispose);
    addTearDown(ipcClient.close);

    await controller.ensureRuntimeReady();
    await controller.pickViewportEntity(viewportX: 0.4, viewportY: 0.6);

    expect(ipcClient.lastPick?.viewportX, 0.4);
    expect(ipcClient.lastPick?.viewportY, 0.6);
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

  test('keeps runtime logs for the bottom console', () async {
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
        kind: RuntimeIpcEventKind.log,
        payload: {'level': 'info', 'message': 'Created Cube 1'},
      ),
    );
    await Future<void>.delayed(Duration.zero);

    expect(controller.logs.single.message, 'Created Cube 1');
    expect(controller.message, 'Created Cube 1');
  });

  test('tails native runtime stdout and stderr logs', () async {
    final tempDir = await Directory.systemTemp.createTemp('besfa_logs_');
    addTearDown(() => tempDir.delete(recursive: true));
    final logFile = File('${tempDir.path}${Platform.pathSeparator}runtime.log');
    await logFile.writeAsString('');

    final plugin = FakeBesfaFlutterPlugin()..runtimeLogPathValue = logFile.path;
    final ipcClient = FakeRuntimeIpcClient();
    final controller = RuntimePreviewController(
      plugin: plugin,
      ipcClient: ipcClient,
    );
    addTearDown(controller.dispose);
    addTearDown(ipcClient.close);

    await controller.ensureRuntimeReady();
    await logFile.writeAsString('native hello\n', mode: FileMode.append);
    await Future<void>.delayed(const Duration(milliseconds: 650));

    expect(
      controller.logs,
      contains(
        isA<RuntimeLogEntry>()
            .having((entry) => entry.level, 'level', 'native')
            .having((entry) => entry.message, 'message', 'native hello'),
      ),
    );
    expect(controller.message, isNull);
  });

  test('keeps native runtime logs when IPC startup fails', () async {
    final tempDir = await Directory.systemTemp.createTemp('besfa_logs_');
    addTearDown(() => tempDir.delete(recursive: true));
    final logFile = File('${tempDir.path}${Platform.pathSeparator}runtime.log');
    await logFile.writeAsString('startup failed\n');

    final plugin = FakeBesfaFlutterPlugin()..runtimeLogPathValue = logFile.path;
    final ipcClient = FakeRuntimeIpcClient()
      ..connectError = StateError('not ready');
    final controller = RuntimePreviewController(
      plugin: plugin,
      ipcClient: ipcClient,
    );
    addTearDown(controller.dispose);
    addTearDown(ipcClient.close);

    await controller.ensureRuntimeReady();
    await Future<void>.delayed(const Duration(milliseconds: 50));

    expect(controller.status, RuntimePreviewStatus.failed);
    expect(
      controller.logs,
      contains(
        isA<RuntimeLogEntry>()
            .having((entry) => entry.level, 'level', 'native')
            .having((entry) => entry.message, 'message', 'startup failed'),
      ),
    );
  });
}
