import 'package:flutter/services.dart';

/// A native texture a game process renders into, shown with a `Texture`
/// widget. Backed by `windows/runner/viewport_texture.cpp`.
class ViewportTexture {
  ViewportTexture._({
    required this.textureId,
    required this.sharedName,
    required this.adapterName,
    required this.width,
    required this.height,
  });

  static const _channel = MethodChannel('besfa/viewport');

  final int textureId;

  /// Name of the shared D3D handle the game opens.
  final String sharedName;

  /// GPU the editor renders on; the game must use the same one.
  final String adapterName;

  final int width;
  final int height;

  static Future<ViewportTexture> create({
    required int width,
    required int height,
  }) async {
    final result = await _channel.invokeMapMethod<String, Object?>('create', {
      'width': width,
      'height': height,
    });
    return ViewportTexture._(
      textureId: result!['textureId']! as int,
      sharedName: result['name']! as String,
      adapterName: result['adapter']! as String,
      width: width,
      height: height,
    );
  }

  Future<void> dispose() => _channel.invokeMethod('dispose', textureId);
}
