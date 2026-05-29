#ifndef FLUTTER_PLUGIN_BESFA_FLUTTER_PLUGIN_H_
#define FLUTTER_PLUGIN_BESFA_FLUTTER_PLUGIN_H_

#include <flutter/method_channel.h>
#include <flutter/plugin_registrar_windows.h>

#include <map>
#include <memory>

#include "preview_texture.h"

namespace besfa_flutter_plugin {

class BesfaFlutterPlugin : public flutter::Plugin {
public:
  static void RegisterWithRegistrar(flutter::PluginRegistrarWindows *registrar);

  explicit BesfaFlutterPlugin(
      flutter::TextureRegistrar *texture_registrar = nullptr);

  virtual ~BesfaFlutterPlugin();

  // Disallow copy and assign.
  BesfaFlutterPlugin(const BesfaFlutterPlugin &) = delete;
  BesfaFlutterPlugin &operator=(const BesfaFlutterPlugin &) = delete;

  // Called when a method is called on this plugin's channel from Dart.
  void HandleMethodCall(
      const flutter::MethodCall<flutter::EncodableValue> &method_call,
      std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);

private:
  void HandleCreatePreviewTexture(
      const flutter::MethodCall<flutter::EncodableValue> &method_call,
      std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);
  void HandleAttachPreviewSurface(
      const flutter::MethodCall<flutter::EncodableValue> &method_call,
      std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);
  void HandleMarkPreviewTextureFrameAvailable(
      const flutter::MethodCall<flutter::EncodableValue> &method_call,
      std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);
  void HandleDisposePreviewTexture(
      const flutter::MethodCall<flutter::EncodableValue> &method_call,
      std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);

  flutter::TextureRegistrar *texture_registrar_;
  std::map<int64_t, std::shared_ptr<PreviewTexture>> preview_textures_;
};

} // namespace besfa_flutter_plugin

#endif // FLUTTER_PLUGIN_BESFA_FLUTTER_PLUGIN_H_
