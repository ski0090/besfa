import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:besfa_editor/features/runtime_ipc/application/runtime_ipc_client.dart';
import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:besfa_flutter_plugin/besfa_flutter_plugin.dart';
import 'package:flutter/foundation.dart';

/// Coordinates preview runtime process control and runtime IPC state.
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
  Timer? _previewTextureFrameTimer;
  Timer? _nativeLogTimer;
  String? _nativeLogPath;
  int _nativeLogOffset = 0;
  String _nativeLogRemainder = '';
  bool _isMarkingPreviewFrame = false;
  bool _isReadingNativeLog = false;
  bool _isRuntimeIpcReady = false;
  bool _disposed = false;

  late final Future<String?> platformVersion;
  late final int abiVersion;

  /// Current preview runtime status shown by the editor.
  RuntimePreviewStatus status = RuntimePreviewStatus.stopped;

  /// Whether a preview command is in progress.
  bool isBusy = false;

  /// Last user-facing runtime message.
  String? message;

  /// Latest scene hierarchy snapshot received from the runtime.
  RuntimeSceneSnapshot? sceneSnapshot;

  /// Latest runtime frame timing telemetry.
  RuntimeFrameStats? frameStats;

  /// Recent runtime log entries.
  List<RuntimeLogEntry> logs = const [];

  /// Flutter texture id for the runtime-owned preview surface.
  int? previewTextureId;
  String? _previewSurfaceHandleName;

  /// Ensures the editor-owned scene runtime is running and IPC-ready.
  Future<void> ensureRuntimeReady() async {
    if (isBusy ||
        (status == RuntimePreviewStatus.running && _isRuntimeIpcReady)) {
      return;
    }

    await _startRuntimeSession(restartTrackedRuntime: true);
  }

  /// Starts the scene runtime and waits for IPC readiness.
  Future<void> runPreview() async {
    await ensureRuntimeReady();
  }

  /// Stops the preview runtime process and clears runtime data.
  Future<void> stopPreview() async {
    if (isBusy) {
      return;
    }

    _apply(isBusy: true);
    await _ipcClient.disconnect();
    _isRuntimeIpcReady = false;
    final result = _plugin.stopRuntime();
    _clearRuntimeData();
    _apply(
      status: _statusForStopResult(result),
      message: _messageForStopResult(result),
      isBusy: false,
    );
  }

  /// Restarts the editor-owned scene runtime process.
  Future<void> restartRuntime() async {
    if (isBusy) {
      return;
    }

    _apply(
      status: RuntimePreviewStatus.starting,
      message: 'Restarting scene runtime.',
      isBusy: true,
    );
    await _ipcClient.disconnect();
    _isRuntimeIpcReady = false;
    final stopResult = _plugin.stopRuntime();
    _clearRuntimeData();
    if (stopResult == BesfaRuntimeCommandResult.failed) {
      _apply(
        status: RuntimePreviewStatus.failed,
        message: _messageForStopResult(stopResult),
        isBusy: false,
      );
      return;
    }

    await _startRuntimeSession(
      restartTrackedRuntime: false,
      startingMessage: 'Restarting scene runtime.',
    );
  }

  /// Reloads the running scene, or restarts the runtime if it is stopped.
  Future<void> reloadRuntime() async {
    if (isBusy) {
      return;
    }

    if (status == RuntimePreviewStatus.running && _isRuntimeIpcReady) {
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

    await ensureRuntimeReady();
  }

  /// Polls the native bridge for process state changes.
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
        unawaited(
          _recoverRuntimeAfterExit('Scene runtime closed; restarting.'),
        );
      case BesfaRuntimeState.failed:
        unawaited(_ipcClient.disconnect());
        _isRuntimeIpcReady = false;
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
    _previewTextureFrameTimer?.cancel();
    _stopNativeRuntimeLogTail();
    unawaited(_ipcEventsSubscription?.cancel());
    final textureId = previewTextureId;
    if (textureId != null) {
      unawaited(_plugin.disposePreviewTexture(textureId));
    }
    unawaited(_ipcClient.disconnect());
    _isRuntimeIpcReady = false;
    _plugin.stopRuntime();
    super.dispose();
  }

  /// Sends a runtime entity selection command.
  Future<void> selectEntity(String entityId) async {
    if (status != RuntimePreviewStatus.running ||
        !_isRuntimeIpcReady ||
        entityId.isEmpty) {
      return;
    }

    try {
      await _ipcClient.selectEntity(entityId);
      _selectEntityInCurrentSnapshot(entityId);
    } on Object {
      _apply(message: 'Runtime entity could not be selected.');
    }
  }

  /// Picks the runtime entity under normalized viewport coordinates.
  Future<void> pickViewportEntity({
    required double viewportX,
    required double viewportY,
  }) async {
    if (status != RuntimePreviewStatus.running || !_isRuntimeIpcReady) {
      return;
    }

    try {
      final entityId = await _ipcClient.pickEntity(
        viewportX: viewportX,
        viewportY: viewportY,
      );
      _selectEntityInCurrentSnapshot(entityId);
    } on Object {
      _apply(message: 'Runtime viewport pick failed.');
    }
  }

  /// Creates a cube entity in the runtime scene.
  Future<void> createCube() async {
    if (isBusy ||
        status != RuntimePreviewStatus.running ||
        !_isRuntimeIpcReady) {
      return;
    }

    _apply(isBusy: true);
    try {
      await _ipcClient.createEntity(kind: 'cube');
      _apply(isBusy: false);
    } on Object {
      _apply(message: 'Runtime cube could not be created.', isBusy: false);
    }
  }

  /// Updates the selected runtime entity translation.
  Future<void> setSelectedEntityTranslation(RuntimeVector3 translation) async {
    final entityId = sceneSnapshot?.selectedEntityId;
    if (isBusy ||
        status != RuntimePreviewStatus.running ||
        !_isRuntimeIpcReady ||
        entityId == null) {
      return;
    }

    _apply(isBusy: true);
    try {
      await _ipcClient.setTransform(
        entityId: entityId,
        translation: translation,
      );
      _apply(isBusy: false);
    } on Object {
      _apply(message: 'Runtime transform could not be updated.', isBusy: false);
    }
  }

  /// Applies editor Scene View camera navigation without changing scene data.
  Future<void> applyEditorCameraInput({
    double rotateDeltaX = 0,
    double rotateDeltaY = 0,
    double moveForward = 0,
    double moveRight = 0,
    double moveUp = 0,
    double speedMultiplier = 1,
    double deltaSeconds = 0,
  }) async {
    if (status != RuntimePreviewStatus.running || !_isRuntimeIpcReady) {
      return;
    }

    try {
      await _ipcClient.editorCameraInput(
        rotateDeltaX: rotateDeltaX,
        rotateDeltaY: rotateDeltaY,
        moveForward: moveForward,
        moveRight: moveRight,
        moveUp: moveUp,
        speedMultiplier: speedMultiplier,
        deltaSeconds: deltaSeconds,
      );
    } on Object {
      _apply(message: 'Runtime editor camera could not be updated.');
    }
  }

  Future<void> _recoverRuntimeAfterExit(String startingMessage) async {
    if (_disposed || isBusy) {
      return;
    }

    _apply(
      status: RuntimePreviewStatus.starting,
      message: startingMessage,
      isBusy: true,
    );
    await _ipcClient.disconnect();
    _isRuntimeIpcReady = false;
    _clearRuntimeData();
    await _startRuntimeSession(
      restartTrackedRuntime: false,
      startingMessage: startingMessage,
    );
  }

  Future<void> _startRuntimeSession({
    required bool restartTrackedRuntime,
    String? startingMessage,
  }) async {
    _isRuntimeIpcReady = false;
    _apply(
      status: RuntimePreviewStatus.starting,
      message: startingMessage,
      isBusy: true,
    );

    try {
      final handshake = await _startRuntimeProcess(
        restartTrackedRuntime: restartTrackedRuntime,
      );
      if (handshake == null) {
        return;
      }

      await _ipcClient.connectAndWaitReady(handshake);
      if (_disposed) {
        return;
      }

      _isRuntimeIpcReady = true;
      _apply(status: RuntimePreviewStatus.running, isBusy: false);
    } on Object {
      final runtimeState = _plugin.runtimeState;
      _plugin.stopRuntime();
      await _ipcClient.disconnect();
      _isRuntimeIpcReady = false;
      await _pollNativeRuntimeLog();
      _clearRuntimeData(clearLogs: false);
      _apply(
        status: RuntimePreviewStatus.failed,
        message: _runtimeReadyFailureMessage(runtimeState),
        isBusy: false,
      );
    }
  }

  Future<RuntimeIpcHandshake?> _startRuntimeProcess({
    required bool restartTrackedRuntime,
  }) async {
    var handshake = await _ipcClient.reserveHandshake();
    var result = _plugin.startRuntimeWithIpc(
      port: handshake.port,
      token: handshake.token,
    );

    if (result == BesfaRuntimeCommandResult.alreadyRunning &&
        restartTrackedRuntime) {
      _plugin.stopRuntime();
      await _ipcClient.disconnect();
      _clearRuntimeData();
      handshake = await _ipcClient.reserveHandshake();
      result = _plugin.startRuntimeWithIpc(
        port: handshake.port,
        token: handshake.token,
      );
    }

    if (result != BesfaRuntimeCommandResult.ok) {
      _apply(
        status: _statusForStartResult(result),
        message: _messageForStartResult(result),
        isBusy: false,
      );
      return null;
    }

    _beginNativeRuntimeLogTail(reset: true);
    return handshake;
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
      case RuntimeIpcEventKind.previewSurfaceReady:
        unawaited(
          _attachPreviewSurface(
            RuntimePreviewSurface.fromPayload(event.payload),
          ),
        );
      case RuntimeIpcEventKind.log:
        final log = RuntimeLogEntry.fromPayload(event.payload);
        _appendLog(log, updateMessage: true);
      case RuntimeIpcEventKind.unknown:
        return;
    }
  }

  void _selectEntityInCurrentSnapshot(String? entityId) {
    final snapshot = sceneSnapshot;
    if (snapshot == null || snapshot.selectedEntityId == entityId) {
      return;
    }

    sceneSnapshot = snapshot.withSelectedEntityId(entityId);
    notifyListeners();
  }

  void _appendLog(RuntimeLogEntry log, {bool updateMessage = false}) {
    _appendLogs([log], updateMessage: updateMessage);
  }

  void _appendLogs(
    List<RuntimeLogEntry> newLogs, {
    bool updateMessage = false,
  }) {
    if (_disposed || newLogs.isEmpty) {
      return;
    }

    final nextLogs = [...logs, ...newLogs];
    logs = nextLogs.length <= 200
        ? nextLogs
        : nextLogs.sublist(nextLogs.length - 200);
    if (updateMessage) {
      message = newLogs.last.message;
    }
    notifyListeners();
  }

  void _beginNativeRuntimeLogTail({bool reset = false}) {
    final path = _plugin.runtimeLogPath;
    if (path == null || path.isEmpty) {
      return;
    }

    _nativeLogPath = path;
    if (reset) {
      _nativeLogOffset = 0;
      _nativeLogRemainder = '';
    }

    _nativeLogTimer?.cancel();
    _nativeLogTimer = Timer.periodic(const Duration(milliseconds: 500), (_) {
      unawaited(_pollNativeRuntimeLog());
    });
    unawaited(_pollNativeRuntimeLog());
  }

  void _stopNativeRuntimeLogTail() {
    _nativeLogTimer?.cancel();
    _nativeLogTimer = null;
    _nativeLogPath = null;
    _nativeLogOffset = 0;
    _nativeLogRemainder = '';
    _isReadingNativeLog = false;
  }

  Future<void> _pollNativeRuntimeLog() async {
    final path = _nativeLogPath;
    if (_disposed || path == null || _isReadingNativeLog) {
      return;
    }

    _isReadingNativeLog = true;
    try {
      final file = File(path);
      if (!await file.exists()) {
        return;
      }

      final length = await file.length();
      if (length < _nativeLogOffset) {
        _nativeLogOffset = 0;
        _nativeLogRemainder = '';
      }
      if (length == _nativeLogOffset) {
        return;
      }

      final reader = await file.open();
      try {
        await reader.setPosition(_nativeLogOffset);
        final bytes = await reader.read(length - _nativeLogOffset);
        _nativeLogOffset = length;
        _appendNativeLogText(utf8.decode(bytes, allowMalformed: true));
      } finally {
        await reader.close();
      }
    } finally {
      _isReadingNativeLog = false;
    }
  }

  void _appendNativeLogText(String text) {
    if (text.isEmpty) {
      return;
    }

    final combined = '$_nativeLogRemainder$text'
        .replaceAll('\r\n', '\n')
        .replaceAll('\r', '\n');
    final endsWithNewline = combined.endsWith('\n');
    final lines = combined.split('\n');
    _nativeLogRemainder = endsWithNewline ? '' : lines.removeLast();

    final entries = [
      for (final line in lines)
        if (line.trim().isNotEmpty)
          RuntimeLogEntry(level: 'native', message: line),
    ];
    _appendLogs(entries);
  }

  void _clearRuntimeData({bool clearLogs = true}) {
    _stopNativeRuntimeLogTail();
    sceneSnapshot = null;
    frameStats = null;
    if (clearLogs) {
      logs = const [];
    }
    _previewTextureFrameTimer?.cancel();
    _previewTextureFrameTimer = null;
    _isMarkingPreviewFrame = false;
    _previewSurfaceHandleName = null;
    final textureId = previewTextureId;
    previewTextureId = null;
    if (textureId != null) {
      unawaited(_plugin.disposePreviewTexture(textureId));
    }
  }

  Future<void> _attachPreviewSurface(RuntimePreviewSurface surface) async {
    if (surface.sharedHandleName.isEmpty ||
        surface.width <= 0 ||
        surface.height <= 0) {
      return;
    }

    try {
      if (_previewSurfaceHandleName == surface.sharedHandleName &&
          previewTextureId != null) {
        _ensurePreviewTextureFrameTimer();
        return;
      }

      final oldTextureId = previewTextureId;
      final textureId = await _plugin.attachPreviewSurface(
        BesfaPreviewSurfaceDescriptor(
          sharedHandleName: surface.sharedHandleName,
          width: surface.width,
          height: surface.height,
          format: surface.format,
        ),
      );
      if (_disposed || textureId == null || textureId <= 0) {
        return;
      }

      previewTextureId = textureId;
      _previewSurfaceHandleName = surface.sharedHandleName;
      _ensurePreviewTextureFrameTimer();
      notifyListeners();
      if (oldTextureId != null && oldTextureId != textureId) {
        unawaited(_plugin.disposePreviewTexture(oldTextureId));
      }
    } on Object {
      _apply(message: 'Runtime preview surface could not attach.');
    }
  }

  void _ensurePreviewTextureFrameTimer() {
    if (_previewTextureFrameTimer != null) {
      return;
    }

    _previewTextureFrameTimer = Timer.periodic(
      const Duration(milliseconds: 16),
      (_) {
        final textureId = previewTextureId;
        if (_disposed ||
            textureId == null ||
            status != RuntimePreviewStatus.running ||
            _isMarkingPreviewFrame) {
          return;
        }

        _isMarkingPreviewFrame = true;
        unawaited(
          _plugin.markPreviewTextureFrameAvailable(textureId).whenComplete(() {
            _isMarkingPreviewFrame = false;
          }),
        );
      },
    );
  }

  RuntimePreviewStatus _statusForStartResult(BesfaRuntimeCommandResult result) {
    return switch (result) {
      BesfaRuntimeCommandResult.ok => RuntimePreviewStatus.running,
      BesfaRuntimeCommandResult.alreadyRunning ||
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
      BesfaRuntimeCommandResult.ok => null,
      BesfaRuntimeCommandResult.alreadyRunning =>
        'Scene runtime is already running but could not be attached.',
      BesfaRuntimeCommandResult.notRunning ||
      BesfaRuntimeCommandResult.failed => _errorMessage(
        'Scene runtime could not start.',
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

  String _runtimeReadyFailureMessage(BesfaRuntimeState runtimeState) {
    return switch (runtimeState) {
      BesfaRuntimeState.exited ||
      BesfaRuntimeState.stopped => 'Scene runtime exited before IPC was ready.',
      BesfaRuntimeState.failed => _errorMessage(
        'Scene runtime status could not be read.',
      ),
      BesfaRuntimeState.running =>
        'Scene runtime IPC did not become ready after 20 seconds.',
    };
  }
}
