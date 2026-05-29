#ifndef FLUTTER_PLUGIN_BESFA_FLUTTER_PLUGIN_H_
#define FLUTTER_PLUGIN_BESFA_FLUTTER_PLUGIN_H_

#include <flutter/method_channel.h>
#include <flutter/plugin_registrar_windows.h>

#include <memory>

namespace besfa_flutter_plugin {

class BesfaFlutterPlugin : public flutter::Plugin {
 public:
  static void RegisterWithRegistrar(flutter::PluginRegistrarWindows *registrar);

  BesfaFlutterPlugin();

  virtual ~BesfaFlutterPlugin();

  // Disallow copy and assign.
  BesfaFlutterPlugin(const BesfaFlutterPlugin&) = delete;
  BesfaFlutterPlugin& operator=(const BesfaFlutterPlugin&) = delete;

  // Called when a method is called on this plugin's channel from Dart.
  void HandleMethodCall(
      const flutter::MethodCall<flutter::EncodableValue> &method_call,
      std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);
};

}  // namespace besfa_flutter_plugin

#endif  // FLUTTER_PLUGIN_BESFA_FLUTTER_PLUGIN_H_
