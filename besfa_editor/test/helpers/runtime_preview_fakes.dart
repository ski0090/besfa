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
    return createdTextureId;
  }

  @override
  Future<int?> attachPreviewSurface(
    BesfaPreviewSurfaceDescriptor descriptor,
  ) async {
    createdTextureId = 13;
    return createdTextureId;
  }

  @override
  Future<bool> markPreviewTextureFrameAvailable(int textureId) async {
    return createdTextureId == textureId;
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
  int createEntityCalls = 0;
  RuntimeVector3? lastTranslation;
  ({double viewportX, double viewportY})? lastPick;
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
}
