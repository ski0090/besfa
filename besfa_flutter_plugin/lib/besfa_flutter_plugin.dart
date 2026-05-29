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

  BesfaRuntimeCommandResult stopRuntime() {
    return BesfaRuntimeCommandResult.fromNativeCode(native.besfaRuntimeStop());
  }
}
