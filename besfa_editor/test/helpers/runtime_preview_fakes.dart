import 'dart:async';

import 'package:besfa_editor/features/runtime_ipc/application/runtime_ipc_client.dart';
import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_flutter_plugin/besfa_flutter_plugin.dart';

class FakeBesfaFlutterPlugin extends BesfaFlutterPlugin {
  BesfaRuntimeState state = BesfaRuntimeState.stopped;
  BesfaRuntimeErrorCode error = BesfaRuntimeErrorCode.none;
  BesfaRuntimeCommandResult startResult = BesfaRuntimeCommandResult.ok;
  BesfaRuntimeCommandResult stopResult = BesfaRuntimeCommandResult.ok;
  String? runtimeLogPathValue;
  int? createdTextureId;
  final Set<int> createdTextureIds = <int>{};
  int _nextAttachedTextureId = 13;
  int stopCalls = 0;

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
    stopCalls += 1;
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
  String? get runtimeLogPath => runtimeLogPathValue;

  @override
  Future<int?> createPreviewTexture({int width = 640, int height = 360}) async {
    createdTextureId = 11;
    createdTextureIds.add(createdTextureId!);
    return createdTextureId;
  }

  @override
  Future<int?> attachPreviewSurface(
    BesfaPreviewSurfaceDescriptor descriptor,
  ) async {
    createdTextureId = _nextAttachedTextureId++;
    createdTextureIds.add(createdTextureId!);
    return createdTextureId;
  }

  @override
  Future<bool> markPreviewTextureFrameAvailable(int textureId) async {
    return createdTextureIds.contains(textureId);
  }

  @override
  Future<bool> disposePreviewTexture(int textureId) async {
    if (createdTextureIds.remove(textureId)) {
      if (createdTextureId == textureId) {
        createdTextureId = createdTextureIds.isEmpty
            ? null
            : createdTextureIds.last;
      }
      return true;
    }
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
  int createEntityCalls = 0;
  int playSceneCalls = 0;
  int stopSceneCalls = 0;
  int alignSelectedCameraToEditorCalls = 0;
  int beginTransformAxisDragCalls = 0;
  int updateTransformAxisDragCalls = 0;
  int endTransformAxisDragCalls = 0;
  RuntimeTransformAxis? transformAxisDragResult;
  ({double viewportX, double viewportY})? lastTransformAxisDragBegin;
  ({double viewportX, double viewportY})? lastTransformAxisDragUpdate;
  RuntimeVector3? lastTranslation;
  ({double viewportX, double viewportY})? lastPick;
  ({
    double rotateDeltaX,
    double rotateDeltaY,
    double moveForward,
    double moveRight,
    double moveUp,
    double speedMultiplier,
    double deltaSeconds,
  })?
  lastEditorCameraInput;
  String? pickResult = 'picked_entity';
  Object? connectError;

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
  }) async {
    final error = connectError;
    if (error != null) {
      throw error;
    }
  }

  @override
  Future<void> disconnect() async {}

  @override
  Future<void> reloadScene() async {}

  @override
  Future<void> playScene() async {
    playSceneCalls += 1;
  }

  @override
  Future<void> stopScene() async {
    stopSceneCalls += 1;
  }

  @override
  Future<void> selectEntity(String entityId) async {}

  @override
  Future<String?> createEntity({
    required String kind,
    String? name,
    String? parentEntityId,
  }) async {
    createEntityCalls += 1;
    return 'created_$createEntityCalls';
  }

  @override
  Future<String?> pickEntity({
    required double viewportX,
    required double viewportY,
  }) async {
    lastPick = (viewportX: viewportX, viewportY: viewportY);
    return pickResult;
  }

  @override
  Future<void> setTransform({
    required String entityId,
    required RuntimeVector3 translation,
  }) async {
    lastTranslation = translation;
  }

  @override
  Future<void> editorCameraInput({
    double rotateDeltaX = 0,
    double rotateDeltaY = 0,
    double moveForward = 0,
    double moveRight = 0,
    double moveUp = 0,
    double speedMultiplier = 1,
    double deltaSeconds = 0,
  }) async {
    lastEditorCameraInput = (
      rotateDeltaX: rotateDeltaX,
      rotateDeltaY: rotateDeltaY,
      moveForward: moveForward,
      moveRight: moveRight,
      moveUp: moveUp,
      speedMultiplier: speedMultiplier,
      deltaSeconds: deltaSeconds,
    );
  }

  @override
  Future<void> alignSelectedCameraToEditor() async {
    alignSelectedCameraToEditorCalls += 1;
  }

  @override
  Future<RuntimeTransformAxis?> beginTransformAxisDrag({
    required double viewportX,
    required double viewportY,
  }) async {
    beginTransformAxisDragCalls += 1;
    lastTransformAxisDragBegin = (viewportX: viewportX, viewportY: viewportY);
    return transformAxisDragResult;
  }

  @override
  Future<RuntimeVector3?> updateTransformAxisDrag({
    required double viewportX,
    required double viewportY,
  }) async {
    updateTransformAxisDragCalls += 1;
    lastTransformAxisDragUpdate = (viewportX: viewportX, viewportY: viewportY);
    return const RuntimeVector3(x: 1, y: 0, z: 0);
  }

  @override
  Future<void> endTransformAxisDrag() async {
    endTransformAxisDragCalls += 1;
  }
}
