import 'dart:async';

import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:besfa_flutter_plugin/besfa_flutter_plugin.dart';
import 'package:flutter/foundation.dart';

class RuntimePreviewController extends ChangeNotifier {
  RuntimePreviewController({BesfaFlutterPlugin? plugin})
    : _plugin = plugin ?? BesfaFlutterPlugin() {
    platformVersion = _plugin.getPlatformVersion();
    abiVersion = _plugin.abiVersion;
    _syncInitialStatus();
    _statusTimer = Timer.periodic(const Duration(seconds: 1), (_) {
      refreshRuntimeStatus();
    });
  }

  final BesfaFlutterPlugin _plugin;
  Timer? _statusTimer;

  late final Future<String?> platformVersion;
  late final int abiVersion;

  RuntimePreviewStatus status = RuntimePreviewStatus.stopped;
  bool isBusy = false;
  String? message;

  void runPreview() {
    if (isBusy) {
      return;
    }

    _apply(isBusy: true);
    final result = _plugin.startRuntime();
    _apply(
      status: _statusForStartResult(result),
      message: _messageForStartResult(result),
      isBusy: false,
    );
  }

  void stopPreview() {
    if (isBusy) {
      return;
    }

    _apply(isBusy: true);
    final result = _plugin.stopRuntime();
    _apply(
      status: _statusForStopResult(result),
      message: _messageForStopResult(result),
      isBusy: false,
    );
  }

  void reloadRuntime() {
    if (isBusy) {
      return;
    }

    _apply(isBusy: true);
    _plugin.stopRuntime();
    final result = _plugin.startRuntime();
    _apply(
      status: _statusForStartResult(result),
      message: _messageForStartResult(result),
      isBusy: false,
    );
  }

  void refreshRuntimeStatus() {
    if (isBusy || status != RuntimePreviewStatus.running) {
      return;
    }

    final runtimeState = _plugin.runtimeState;
    switch (runtimeState) {
      case BesfaRuntimeState.running:
        return;
      case BesfaRuntimeState.stopped:
      case BesfaRuntimeState.exited:
        _apply(
          status: RuntimePreviewStatus.stopped,
          message: 'Preview window closed.',
        );
      case BesfaRuntimeState.failed:
        _apply(
          status: RuntimePreviewStatus.failed,
          message: _errorMessage('Could not read runtime status.'),
        );
    }
  }

  @override
  void dispose() {
    _statusTimer?.cancel();
    super.dispose();
  }

  void _syncInitialStatus() {
    final runtimeState = _plugin.runtimeState;
    status = switch (runtimeState) {
      BesfaRuntimeState.running => RuntimePreviewStatus.running,
      BesfaRuntimeState.failed => RuntimePreviewStatus.failed,
      BesfaRuntimeState.stopped ||
      BesfaRuntimeState.exited => RuntimePreviewStatus.stopped,
    };
    message = switch (runtimeState) {
      BesfaRuntimeState.failed => _errorMessage(
        'Could not read runtime status.',
      ),
      BesfaRuntimeState.exited => 'Preview window closed.',
      BesfaRuntimeState.running || BesfaRuntimeState.stopped => null,
    };
  }

  void _apply({RuntimePreviewStatus? status, bool? isBusy, String? message}) {
    if (status != null) {
      this.status = status;
    }
    if (isBusy != null) {
      this.isBusy = isBusy;
    }
    this.message = message;
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

  String? _messageForStartResult(BesfaRuntimeCommandResult result) {
    return switch (result) {
      BesfaRuntimeCommandResult.ok ||
      BesfaRuntimeCommandResult.alreadyRunning => null,
      BesfaRuntimeCommandResult.notRunning ||
      BesfaRuntimeCommandResult.failed => _errorMessage(
        'Preview runtime could not start.',
      ),
    };
  }

  String? _messageForStopResult(BesfaRuntimeCommandResult result) {
    return switch (result) {
      BesfaRuntimeCommandResult.ok => null,
      BesfaRuntimeCommandResult.notRunning => 'Preview already stopped.',
      BesfaRuntimeCommandResult.alreadyRunning ||
      BesfaRuntimeCommandResult.failed => _errorMessage(
        'Preview runtime could not stop.',
      ),
    };
  }

  String _errorMessage(String fallback) {
    final error = _plugin.runtimeLastError;
    if (error == BesfaRuntimeErrorCode.none) {
      return fallback;
    }

    return error.message;
  }
}
