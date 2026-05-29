import 'package:plugin_platform_interface/plugin_platform_interface.dart';

import 'besfa_flutter_plugin_method_channel.dart';
import 'src/preview_surface_descriptor.dart';

/// Platform interface for Besfa Flutter plugin implementations.
abstract class BesfaFlutterPluginPlatform extends PlatformInterface {
  /// Constructs a BesfaFlutterPluginPlatform.
  BesfaFlutterPluginPlatform() : super(token: _token);

  static final Object _token = Object();

  static BesfaFlutterPluginPlatform _instance =
      MethodChannelBesfaFlutterPlugin();

  /// The default instance of [BesfaFlutterPluginPlatform] to use.
  ///
  /// Defaults to [MethodChannelBesfaFlutterPlugin].
  static BesfaFlutterPluginPlatform get instance => _instance;

  /// Platform-specific implementations should set this with their own
  /// platform-specific class that extends [BesfaFlutterPluginPlatform] when
  /// they register themselves.
  static set instance(BesfaFlutterPluginPlatform instance) {
    PlatformInterface.verifyToken(instance, _token);
    _instance = instance;
  }

  /// Returns a platform version string from the host implementation.
  Future<String?> getPlatformVersion() {
    throw UnimplementedError('platformVersion() has not been implemented.');
  }

  /// Creates a native preview texture and returns its Flutter texture id.
  Future<int?> createPreviewTexture({required int width, required int height}) {
    throw UnimplementedError(
      'createPreviewTexture() has not been implemented.',
    );
  }

  /// Attaches a shared preview surface and returns its Flutter texture id.
  Future<int?> attachPreviewSurface(BesfaPreviewSurfaceDescriptor descriptor) {
    throw UnimplementedError(
      'attachPreviewSurface() has not been implemented.',
    );
  }

  /// Signals that Flutter should sample a fresh frame for a preview texture.
  Future<bool> markPreviewTextureFrameAvailable(int textureId) {
    throw UnimplementedError(
      'markPreviewTextureFrameAvailable() has not been implemented.',
    );
  }

  /// Disposes a native preview texture by Flutter texture id.
  Future<bool> disposePreviewTexture(int textureId) {
    throw UnimplementedError(
      'disposePreviewTexture() has not been implemented.',
    );
  }
}
