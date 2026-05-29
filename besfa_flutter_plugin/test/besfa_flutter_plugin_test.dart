import 'package:flutter_test/flutter_test.dart';
import 'package:besfa_flutter_plugin/besfa_flutter_plugin.dart';
import 'package:besfa_flutter_plugin/besfa_flutter_plugin_platform_interface.dart';
import 'package:besfa_flutter_plugin/besfa_flutter_plugin_method_channel.dart';
import 'package:plugin_platform_interface/plugin_platform_interface.dart';

class MockBesfaFlutterPluginPlatform
    with MockPlatformInterfaceMixin
    implements BesfaFlutterPluginPlatform {
  @override
  Future<String?> getPlatformVersion() => Future.value('42');
}

void main() {
  final BesfaFlutterPluginPlatform initialPlatform =
      BesfaFlutterPluginPlatform.instance;

  test('$MethodChannelBesfaFlutterPlugin is the default instance', () {
    expect(initialPlatform, isInstanceOf<MethodChannelBesfaFlutterPlugin>());
  });

  test('getPlatformVersion', () async {
    BesfaFlutterPlugin besfaFlutterPlugin = BesfaFlutterPlugin();
    MockBesfaFlutterPluginPlatform fakePlatform =
        MockBesfaFlutterPluginPlatform();
    BesfaFlutterPluginPlatform.instance = fakePlatform;

    expect(await besfaFlutterPlugin.getPlatformVersion(), '42');
  });

  test('calls Rust FFI smoke functions', () {
    final besfaFlutterPlugin = BesfaFlutterPlugin();

    expect(besfaFlutterPlugin.abiVersion, 1);
    expect(besfaFlutterPlugin.add(2, 3), 5);
  });

  test('reads runtime status from Rust FFI', () {
    final besfaFlutterPlugin = BesfaFlutterPlugin();

    expect(besfaFlutterPlugin.runtimeState, BesfaRuntimeState.stopped);
    expect(besfaFlutterPlugin.runtimeLastError, BesfaRuntimeErrorCode.none);
  });
}
