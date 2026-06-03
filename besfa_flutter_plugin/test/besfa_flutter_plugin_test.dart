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

  @override
  Future<int?> createPreviewTexture({
    required int width,
    required int height,
  }) async {
    return 7;
  }

  @override
  Future<int?> attachPreviewSurface(
    BesfaPreviewSurfaceDescriptor descriptor,
  ) async {
    return 9;
  }

  @override
  Future<bool> markPreviewTextureFrameAvailable(int textureId) async {
    return textureId == 9;
  }

  @override
  Future<bool> disposePreviewTexture(int textureId) async {
    return true;
  }
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

  test('creates and disposes preview textures', () async {
    final besfaFlutterPlugin = BesfaFlutterPlugin();
    final fakePlatform = MockBesfaFlutterPluginPlatform();
    BesfaFlutterPluginPlatform.instance = fakePlatform;

    expect(await besfaFlutterPlugin.createPreviewTexture(), 7);
    expect(await besfaFlutterPlugin.disposePreviewTexture(7), isTrue);
  });

  test('attaches preview surfaces', () async {
    final besfaFlutterPlugin = BesfaFlutterPlugin();
    final fakePlatform = MockBesfaFlutterPluginPlatform();
    BesfaFlutterPluginPlatform.instance = fakePlatform;

    expect(
      await besfaFlutterPlugin.attachPreviewSurface(
        const BesfaPreviewSurfaceDescriptor(
          sharedHandleName: 'Local\\BesfaPreviewSurface-42',
          width: 640,
          height: 360,
          format: 'bgra8_unorm',
        ),
      ),
      9,
    );
  });

  test('marks preview texture frames available', () async {
    final besfaFlutterPlugin = BesfaFlutterPlugin();
    final fakePlatform = MockBesfaFlutterPluginPlatform();
    BesfaFlutterPluginPlatform.instance = fakePlatform;

    expect(
      await besfaFlutterPlugin.markPreviewTextureFrameAvailable(9),
      isTrue,
    );
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
    expect(besfaFlutterPlugin.runtimeLogPath, isNull);
  });
}
