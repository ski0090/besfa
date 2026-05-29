#include "besfa_flutter_plugin.h"

// This must be included before many other Windows headers.
#include <windows.h>

// For getPlatformVersion; remove unless needed for your plugin implementation.
#include <VersionHelpers.h>

#include <flutter/method_channel.h>
#include <flutter/plugin_registrar_windows.h>
#include <flutter/standard_method_codec.h>

#include <memory>
#include <sstream>
#include <variant>

namespace besfa_flutter_plugin {
namespace {

int GetOptionalIntArgument(const flutter::EncodableMap &arguments,
                           const char *key, int fallback) {
  auto iterator = arguments.find(flutter::EncodableValue(key));
  if (iterator == arguments.end()) {
    return fallback;
  }

  if (const auto *value = std::get_if<int32_t>(&iterator->second)) {
    return *value;
  }
  if (const auto *value = std::get_if<int64_t>(&iterator->second)) {
    return static_cast<int>(*value);
  }

  return fallback;
}

int64_t GetTextureIdArgument(const flutter::EncodableValue *arguments) {
  if (!arguments) {
    return 0;
  }
  if (const auto *value = std::get_if<int32_t>(arguments)) {
    return *value;
  }
  if (const auto *value = std::get_if<int64_t>(arguments)) {
    return *value;
  }

  return 0;
}

} // namespace

// static
void BesfaFlutterPlugin::RegisterWithRegistrar(
    flutter::PluginRegistrarWindows *registrar) {
  auto channel =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          registrar->messenger(), "besfa_flutter_plugin",
          &flutter::StandardMethodCodec::GetInstance());

  auto plugin =
      std::make_unique<BesfaFlutterPlugin>(registrar->texture_registrar());

  channel->SetMethodCallHandler(
      [plugin_pointer = plugin.get()](const auto &call, auto result) {
        plugin_pointer->HandleMethodCall(call, std::move(result));
      });

  registrar->AddPlugin(std::move(plugin));
}

BesfaFlutterPlugin::BesfaFlutterPlugin(
    flutter::TextureRegistrar *texture_registrar)
    : texture_registrar_(texture_registrar) {}

BesfaFlutterPlugin::~BesfaFlutterPlugin() {
  if (texture_registrar_) {
    for (const auto &entry : preview_textures_) {
      auto texture = entry.second;
      texture_registrar_->UnregisterTexture(entry.first, [texture]() {});
    }
  }
}

void BesfaFlutterPlugin::HandleMethodCall(
    const flutter::MethodCall<flutter::EncodableValue> &method_call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  if (method_call.method_name().compare("getPlatformVersion") == 0) {
    std::ostringstream version_stream;
    version_stream << "Windows ";
    if (IsWindows10OrGreater()) {
      version_stream << "10+";
    } else if (IsWindows8OrGreater()) {
      version_stream << "8";
    } else if (IsWindows7OrGreater()) {
      version_stream << "7";
    }
    result->Success(flutter::EncodableValue(version_stream.str()));
  } else if (method_call.method_name().compare("createPreviewTexture") == 0) {
    HandleCreatePreviewTexture(method_call, std::move(result));
  } else if (method_call.method_name().compare("disposePreviewTexture") == 0) {
    HandleDisposePreviewTexture(method_call, std::move(result));
  } else {
    result->NotImplemented();
  }
}

void BesfaFlutterPlugin::HandleCreatePreviewTexture(
    const flutter::MethodCall<flutter::EncodableValue> &method_call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  if (!texture_registrar_) {
    result->Error("texture_unavailable",
                  "Flutter texture registrar is unavailable.");
    return;
  }

  int width = 640;
  int height = 360;
  if (const auto *arguments =
          std::get_if<flutter::EncodableMap>(method_call.arguments())) {
    width = GetOptionalIntArgument(*arguments, "width", width);
    height = GetOptionalIntArgument(*arguments, "height", height);
  }

  auto texture = PreviewTexture::Create(static_cast<size_t>(width),
                                        static_cast<size_t>(height));
  if (!texture) {
    result->Error("texture_create_failed",
                  "D3D12 shared preview texture could not be created.");
    return;
  }

  const int64_t texture_id =
      texture_registrar_->RegisterTexture(texture->texture_variant());
  preview_textures_[texture_id] = texture;
  texture_registrar_->MarkTextureFrameAvailable(texture_id);
  result->Success(flutter::EncodableValue(texture_id));
}

void BesfaFlutterPlugin::HandleDisposePreviewTexture(
    const flutter::MethodCall<flutter::EncodableValue> &method_call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  const int64_t texture_id = GetTextureIdArgument(method_call.arguments());
  auto iterator = preview_textures_.find(texture_id);
  if (iterator == preview_textures_.end()) {
    result->Success(flutter::EncodableValue(false));
    return;
  }

  auto texture = iterator->second;
  preview_textures_.erase(iterator);
  texture_registrar_->UnregisterTexture(texture_id, [texture]() {});
  result->Success(flutter::EncodableValue(true));
}

} // namespace besfa_flutter_plugin
