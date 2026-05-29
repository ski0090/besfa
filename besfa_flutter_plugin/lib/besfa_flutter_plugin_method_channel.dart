import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'besfa_flutter_plugin_platform_interface.dart';

/// An implementation of [BesfaFlutterPluginPlatform] that uses method channels.
class MethodChannelBesfaFlutterPlugin extends BesfaFlutterPluginPlatform {
  /// The method channel used to interact with the native platform.
  @visibleForTesting
  final methodChannel = const MethodChannel('besfa_flutter_plugin');

  @override
  Future<String?> getPlatformVersion() async {
    final version = await methodChannel.invokeMethod<String>(
      'getPlatformVersion',
    );
    return version;
  }
}
