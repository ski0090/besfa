import 'dart:async';

import 'package:besfa_editor/features/runtime_ipc/application/runtime_ipc_client.dart';
import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:besfa_flutter_plugin/besfa_flutter_plugin.dart';
import 'package:flutter/foundation.dart';

class RuntimePreviewController extends ChangeNotifier {
  RuntimePreviewController({
    BesfaFlutterPlugin? plugin,
    RuntimeIpcClient? ipcClient,
  }) : _plugin = plugin ?? BesfaFlutterPlugin(),
       _ipcClient = ipcClient ?? RuntimeIpcClient() {
    platformVersion = _plugin.getPlatformVersion();
    abiVersion = _plugin.abiVersion;
    _syncInitialStatus();
    _ipcEventsSubscription = _ipcClient.events.listen(_handleRuntimeIpcEvent);
    _statusTimer = Timer.periodic(const Duration(seconds: 1), (_) {
      refreshRuntimeStatus();
    });
  }

  final BesfaFlutterPlugin _plugin;
  final RuntimeIpcClient _ipcClient;
  StreamSubscription<RuntimeIpcEvent>? _ipcEventsSubscription;
  Timer? _statusTimer;
  bool _disposed = false;

  late final Future<String?> platformVersion;
  late final int abiVersion;

  RuntimePreviewStatus status = RuntimePreviewStatus.stopped;
  bool isBusy = false;
  String? message;
  RuntimeSceneSnapshot? sceneSnapshot;
  RuntimeFrameStats? frameStats;
  List<RuntimeLogEntry> logs = const [];

  Future<void> runPreview() async {
    if (isBusy) {
      return;
    }

    _apply(isBusy: true);
    try {
      final handshake = await _ipcClient.reserveHandshake();
      final result = _plugin.startRuntimeWithIpc(
        port: handshake.port,
        token: handshake.token,
      );

      if (result != BesfaRuntimeCommandResult.ok) {
        _apply(
          status: _statusForStartResult(result),
          message: _messageForStartResult(result),
          isBusy: false,
        );
        return;
      }

      await _ipcClient.connectAndWaitReady(handshake);
      _apply(status: RuntimePreviewStatus.running, isBusy: false);
    } on Object {
      _plugin.stopRuntime();
      await _ipcClient.disconnect();
      _apply(
        status: RuntimePreviewStatus.failed,
        message: 'Runtime IPC did not become ready.',
        isBusy: false,
      );
    }
  }

  Future<void> stopPreview() async {
    if (isBusy) {
      return;
    }

    _apply(isBusy: true);
    await _ipcClient.disconnect();
    final result = _plugin.stopRuntime();
    _clearRuntimeData();
    _apply(
      status: _statusForStopResult(result),
      message: _messageForStopResult(result),
      isBusy: false,
    );
  }

  Future<void> reloadRuntime() async {
    if (isBusy) {
      return;
    }

    if (status == RuntimePreviewStatus.running) {
      _apply(isBusy: true);
      try {
        await _ipcClient.reloadScene();
        _apply(isBusy: false);
      } on Object {
        _apply(
          status: RuntimePreviewStatus.failed,
          message: 'Runtime scene could not reload.',
          isBusy: false,
        );
      }
      return;
    }

    _apply(isBusy: true);
    await _ipcClient.disconnect();
    _plugin.stopRuntime();
    try {
      final handshake = await _ipcClient.reserveHandshake();
      final result = _plugin.startRuntimeWithIpc(
        port: handshake.port,
        token: handshake.token,
      );

      if (result != BesfaRuntimeCommandResult.ok) {
        _apply(
          status: _statusForStartResult(result),
          message: _messageForStartResult(result),
          isBusy: false,
        );
        return;
      }

      await _ipcClient.connectAndWaitReady(handshake);
      _apply(status: RuntimePreviewStatus.running, isBusy: false);
    } on Object {
      _plugin.stopRuntime();
      await _ipcClient.disconnect();
      _apply(
        status: RuntimePreviewStatus.failed,
        message: 'Runtime IPC did not become ready.',
        isBusy: false,
      );
    }
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
        unawaited(_ipcClient.disconnect());
        _clearRuntimeData();
        _apply(
          status: RuntimePreviewStatus.stopped,
          message: 'Preview window closed.',
        );
      case BesfaRuntimeState.failed:
        unawaited(_ipcClient.disconnect());
        _clearRuntimeData();
        _apply(
          status: RuntimePreviewStatus.failed,
          message: _errorMessage('Could not read runtime status.'),
        );
    }
  }

  @override
  void dispose() {
    _disposed = true;
    _statusTimer?.cancel();
    unawaited(_ipcEventsSubscription?.cancel());
    unawaited(_ipcClient.disconnect());
    super.dispose();
  }

  Future<void> selectEntity(String entityId) async {
    if (status != RuntimePreviewStatus.running || entityId.isEmpty) {
      return;
    }

    try {
      await _ipcClient.selectEntity(entityId);
    } on Object {
      _apply(message: 'Runtime entity could not be selected.');
    }
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
    if (_disposed) {
      return;
    }

    if (status != null) {
      this.status = status;
    }
    if (isBusy != null) {
      this.isBusy = isBusy;
    }
    this.message = message;
    notifyListeners();
  }

  void _handleRuntimeIpcEvent(RuntimeIpcEvent event) {
    if (_disposed) {
      return;
    }

    switch (event.kind) {
      case RuntimeIpcEventKind.runtimeReady:
        return;
      case RuntimeIpcEventKind.sceneSnapshot:
        sceneSnapshot = RuntimeSceneSnapshot.fromPayload(event.payload);
        notifyListeners();
      case RuntimeIpcEventKind.frameStats:
        frameStats = RuntimeFrameStats.fromPayload(event.payload);
        notifyListeners();
      case RuntimeIpcEventKind.log:
        final log = RuntimeLogEntry.fromPayload(event.payload);
        logs = [...logs.take(19), log];
        message = log.message;
        notifyListeners();
      case RuntimeIpcEventKind.unknown:
        return;
    }
  }

  void _clearRuntimeData() {
    sceneSnapshot = null;
    frameStats = null;
    logs = const [];
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
