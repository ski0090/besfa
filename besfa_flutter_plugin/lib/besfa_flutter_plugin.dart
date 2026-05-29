import 'besfa_flutter_plugin_platform_interface.dart';
import 'src/native_api.dart' as native;
import 'src/preview_surface_descriptor.dart';

export 'src/preview_surface_descriptor.dart';

/// Result of a native runtime start or stop command.
enum BesfaRuntimeCommandResult {
  /// The command completed successfully.
  ok,

  /// A start request found an already running runtime.
  alreadyRunning,

  /// A stop request found no running runtime.
  notRunning,

  /// The native bridge reported a failure.
  failed;

  /// Converts the integer status code returned by Rust FFI.
  static BesfaRuntimeCommandResult fromNativeCode(int code) {
    return switch (code) {
      0 => BesfaRuntimeCommandResult.ok,
      1 => BesfaRuntimeCommandResult.alreadyRunning,
      2 => BesfaRuntimeCommandResult.notRunning,
      _ => BesfaRuntimeCommandResult.failed,
    };
  }
}

/// Current native runtime process state.
enum BesfaRuntimeState {
  /// No runtime child process is tracked.
  stopped,

  /// The runtime child process is still running.
  running,

  /// The runtime child process exited since the last status check.
  exited,

  /// The native bridge could not read process state.
  failed;

  /// Converts the integer status code returned by Rust FFI.
  static BesfaRuntimeState fromNativeCode(int code) {
    return switch (code) {
      0 => BesfaRuntimeState.stopped,
      1 => BesfaRuntimeState.running,
      2 => BesfaRuntimeState.exited,
      _ => BesfaRuntimeState.failed,
    };
  }
}

/// Last native runtime bridge error.
enum BesfaRuntimeErrorCode {
  /// No native bridge error has been reported.
  none,

  /// Runtime process state lock was poisoned.
  lockPoisoned,

  /// Runtime executable discovery failed.
  executableNotFound,

  /// Runtime process spawn failed.
  spawnFailed,

  /// Runtime process status could not be read.
  statusFailed,

  /// Runtime process stop failed.
  stopFailed,

  /// Runtime launch arguments were invalid.
  invalidArgument,

  /// Unknown native error code.
  unknown;

  /// Converts the integer error code returned by Rust FFI.
  static BesfaRuntimeErrorCode fromNativeCode(int code) {
    return switch (code) {
      0 => BesfaRuntimeErrorCode.none,
      1 => BesfaRuntimeErrorCode.lockPoisoned,
      2 => BesfaRuntimeErrorCode.executableNotFound,
      3 => BesfaRuntimeErrorCode.spawnFailed,
      4 => BesfaRuntimeErrorCode.statusFailed,
      5 => BesfaRuntimeErrorCode.stopFailed,
      6 => BesfaRuntimeErrorCode.invalidArgument,
      _ => BesfaRuntimeErrorCode.unknown,
    };
  }

  /// Human-readable message for editor status surfaces.
  String get message {
    return switch (this) {
      BesfaRuntimeErrorCode.none => '',
      BesfaRuntimeErrorCode.lockPoisoned =>
        'Runtime process state is unavailable.',
      BesfaRuntimeErrorCode.executableNotFound =>
        'Runtime executable was not found.',
      BesfaRuntimeErrorCode.spawnFailed => 'Runtime process could not start.',
      BesfaRuntimeErrorCode.statusFailed =>
        'Runtime process status could not be read.',
      BesfaRuntimeErrorCode.stopFailed => 'Runtime process could not stop.',
      BesfaRuntimeErrorCode.invalidArgument =>
        'Runtime launch arguments were invalid.',
      BesfaRuntimeErrorCode.unknown => 'Unknown runtime error.',
    };
  }
}

/// Dart wrapper around the native Besfa Flutter plugin bridge.
class BesfaFlutterPlugin {
  /// Returns the host platform version from the platform channel.
  Future<String?> getPlatformVersion() {
    return BesfaFlutterPluginPlatform.instance.getPlatformVersion();
  }

  /// Creates a native D3D preview texture and returns its Flutter texture id.
  Future<int?> createPreviewTexture({int width = 640, int height = 360}) {
    return BesfaFlutterPluginPlatform.instance.createPreviewTexture(
      width: width,
      height: height,
    );
  }

  /// Attaches a runtime-owned shared preview surface as a Flutter texture.
  Future<int?> attachPreviewSurface(BesfaPreviewSurfaceDescriptor descriptor) {
    return BesfaFlutterPluginPlatform.instance.attachPreviewSurface(descriptor);
  }

  /// Signals that Flutter should sample a fresh frame for a preview texture.
  Future<bool> markPreviewTextureFrameAvailable(int textureId) {
    return BesfaFlutterPluginPlatform.instance.markPreviewTextureFrameAvailable(
      textureId,
    );
  }

  /// Disposes a previously created native preview texture.
  Future<bool> disposePreviewTexture(int textureId) {
    return BesfaFlutterPluginPlatform.instance.disposePreviewTexture(textureId);
  }

  /// ABI version reported by the native Rust bridge.
  int get abiVersion => native.besfaFlutterPluginAbiVersion();

  /// Calls the native smoke-test addition function.
  int add(int left, int right) {
    return native.besfaFlutterPluginAdd(left, right);
  }

  /// Starts the preview runtime without IPC.
  BesfaRuntimeCommandResult startRuntime() {
    return BesfaRuntimeCommandResult.fromNativeCode(native.besfaRuntimeStart());
  }

  /// Starts the preview runtime with localhost IPC launch arguments.
  BesfaRuntimeCommandResult startRuntimeWithIpc({
    required int port,
    required int token,
  }) {
    return BesfaRuntimeCommandResult.fromNativeCode(
      native.besfaRuntimeStartWithIpc(port, token),
    );
  }

  /// Stops the tracked preview runtime process.
  BesfaRuntimeCommandResult stopRuntime() {
    return BesfaRuntimeCommandResult.fromNativeCode(native.besfaRuntimeStop());
  }

  /// Reads the tracked preview runtime process state.
  BesfaRuntimeState get runtimeState {
    return BesfaRuntimeState.fromNativeCode(native.besfaRuntimeStatus());
  }

  /// Reads the last native runtime bridge error.
  BesfaRuntimeErrorCode get runtimeLastError {
    return BesfaRuntimeErrorCode.fromNativeCode(
      native.besfaRuntimeLastErrorCode(),
    );
  }
}
