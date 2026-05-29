import 'besfa_flutter_plugin_platform_interface.dart';
import 'src/native_api.dart' as native;

enum BesfaRuntimeCommandResult {
  ok,
  alreadyRunning,
  notRunning,
  failed;

  static BesfaRuntimeCommandResult fromNativeCode(int code) {
    return switch (code) {
      0 => BesfaRuntimeCommandResult.ok,
      1 => BesfaRuntimeCommandResult.alreadyRunning,
      2 => BesfaRuntimeCommandResult.notRunning,
      _ => BesfaRuntimeCommandResult.failed,
    };
  }
}

enum BesfaRuntimeState {
  stopped,
  running,
  exited,
  failed;

  static BesfaRuntimeState fromNativeCode(int code) {
    return switch (code) {
      0 => BesfaRuntimeState.stopped,
      1 => BesfaRuntimeState.running,
      2 => BesfaRuntimeState.exited,
      _ => BesfaRuntimeState.failed,
    };
  }
}

enum BesfaRuntimeErrorCode {
  none,
  lockPoisoned,
  executableNotFound,
  spawnFailed,
  statusFailed,
  stopFailed,
  invalidArgument,
  unknown;

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

class BesfaFlutterPlugin {
  Future<String?> getPlatformVersion() {
    return BesfaFlutterPluginPlatform.instance.getPlatformVersion();
  }

  int get abiVersion => native.besfaFlutterPluginAbiVersion();

  int add(int left, int right) {
    return native.besfaFlutterPluginAdd(left, right);
  }

  BesfaRuntimeCommandResult startRuntime() {
    return BesfaRuntimeCommandResult.fromNativeCode(native.besfaRuntimeStart());
  }

  BesfaRuntimeCommandResult startRuntimeWithIpc({
    required int port,
    required int token,
  }) {
    return BesfaRuntimeCommandResult.fromNativeCode(
      native.besfaRuntimeStartWithIpc(port, token),
    );
  }

  BesfaRuntimeCommandResult stopRuntime() {
    return BesfaRuntimeCommandResult.fromNativeCode(native.besfaRuntimeStop());
  }

  BesfaRuntimeState get runtimeState {
    return BesfaRuntimeState.fromNativeCode(native.besfaRuntimeStatus());
  }

  BesfaRuntimeErrorCode get runtimeLastError {
    return BesfaRuntimeErrorCode.fromNativeCode(
      native.besfaRuntimeLastErrorCode(),
    );
  }
}
