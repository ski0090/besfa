import 'package:besfa_editor/features/runtime_preview/application/runtime_preview_controller.dart';
import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:besfa_flutter_plugin/besfa_flutter_plugin.dart';
import 'package:flutter_test/flutter_test.dart';

class FakeBesfaFlutterPlugin extends BesfaFlutterPlugin {
  BesfaRuntimeState state = BesfaRuntimeState.stopped;
  BesfaRuntimeErrorCode error = BesfaRuntimeErrorCode.none;
  BesfaRuntimeCommandResult startResult = BesfaRuntimeCommandResult.ok;
  BesfaRuntimeCommandResult stopResult = BesfaRuntimeCommandResult.ok;

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

  test('reports start failures from the native runtime bridge', () {
    final plugin = FakeBesfaFlutterPlugin()
      ..startResult = BesfaRuntimeCommandResult.failed
      ..error = BesfaRuntimeErrorCode.executableNotFound;
    final controller = RuntimePreviewController(plugin: plugin);
    addTearDown(controller.dispose);

    controller.runPreview();

    expect(controller.status, RuntimePreviewStatus.failed);
    expect(controller.message, 'Runtime executable was not found.');
  });
}
