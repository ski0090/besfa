import 'package:plugin_platform_interface/plugin_platform_interface.dart';

import 'besfa_flutter_plugin_method_channel.dart';

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
}
