import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:besfa_flutter_plugin/besfa_flutter_plugin.dart';
import 'package:flutter/foundation.dart';

class RuntimePreviewController extends ChangeNotifier {
  RuntimePreviewController({BesfaFlutterPlugin? plugin})
    : _plugin = plugin ?? BesfaFlutterPlugin() {
    platformVersion = _plugin.getPlatformVersion();
    abiVersion = _plugin.abiVersion;
  }

  final BesfaFlutterPlugin _plugin;

  late final Future<String?> platformVersion;
  late final int abiVersion;

  RuntimePreviewStatus status = RuntimePreviewStatus.stopped;
  bool isBusy = false;

  void runPreview() {
    if (isBusy) {
      return;
    }

    _setBusy(true);
    final result = _plugin.startRuntime();
    status = _statusForStartResult(result);
    _setBusy(false);
  }

  void stopPreview() {
    if (isBusy) {
      return;
    }

    _setBusy(true);
    final result = _plugin.stopRuntime();
    status = _statusForStopResult(result);
    _setBusy(false);
  }

  void reloadRuntime() {
    if (isBusy) {
      return;
    }

    _setBusy(true);
    _plugin.stopRuntime();
    final result = _plugin.startRuntime();
    status = _statusForStartResult(result);
    _setBusy(false);
  }

  void _setBusy(bool value) {
    isBusy = value;
    notifyListeners();
  }

  RuntimePreviewStatus _statusForStartResult(BesfaRuntimeCommandResult result) {
    return switch (result) {
      BesfaRuntimeCommandResult.ok ||
      BesfaRuntimeCommandResult.alreadyRunning => RuntimePreviewStatus.running,
      BesfaRuntimeCommandResult.notRunning ||
      BesfaRuntimeCommandResult.failed => RuntimePreviewStatus.failed,
    };
  }

  RuntimePreviewStatus _statusForStopResult(BesfaRuntimeCommandResult result) {
    return switch (result) {
      BesfaRuntimeCommandResult.ok ||
      BesfaRuntimeCommandResult.notRunning => RuntimePreviewStatus.stopped,
      BesfaRuntimeCommandResult.alreadyRunning ||
      BesfaRuntimeCommandResult.failed => RuntimePreviewStatus.failed,
    };
  }
}
