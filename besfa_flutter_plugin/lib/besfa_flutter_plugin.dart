import 'besfa_flutter_plugin_platform_interface.dart';
import 'src/native_api.dart' as native;

class BesfaFlutterPlugin {
  Future<String?> getPlatformVersion() {
    return BesfaFlutterPluginPlatform.instance.getPlatformVersion();
  }

  int get abiVersion => native.besfaFlutterPluginAbiVersion();

  int add(int left, int right) {
    return native.besfaFlutterPluginAdd(left, right);
  }
}
