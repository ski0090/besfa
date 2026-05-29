#include "include/besfa_flutter_plugin/besfa_flutter_plugin_c_api.h"

#include <flutter/plugin_registrar_windows.h>

#include "besfa_flutter_plugin.h"

void BesfaFlutterPluginCApiRegisterWithRegistrar(
    FlutterDesktopPluginRegistrarRef registrar) {
  besfa_flutter_plugin::BesfaFlutterPlugin::RegisterWithRegistrar(
      flutter::PluginRegistrarManager::GetInstance()
          ->GetRegistrar<flutter::PluginRegistrarWindows>(registrar));
}
