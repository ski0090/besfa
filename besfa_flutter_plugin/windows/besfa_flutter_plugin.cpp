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
#include <string>
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

std::string GetOptionalStringArgument(const flutter::EncodableMap &arguments,
                                      const char *key) {
  auto iterator = arguments.find(flutter::EncodableValue(key));
  if (iterator == arguments.end()) {
    return "";
  }

  if (const auto *value = std::get_if<std::string>(&iterator->second)) {
    return *value;
  }

  return "";
}

std::wstring Utf8ToWide(const std::string &value) {
  if (value.empty()) {
    return L"";
  }

  const int length = MultiByteToWideChar(CP_UTF8, 0, value.c_str(),
                                         static_cast<int>(value.size()),
                                         nullptr, 0);
  if (length <= 0) {
    return L"";
  }

  std::wstring wide(length, L'\0');
  MultiByteToWideChar(CP_UTF8, 0, value.c_str(),
                      static_cast<int>(value.size()), wide.data(), length);
  return wide;
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
  } else if (method_call.method_name().compare("attachPreviewSurface") == 0) {
    HandleAttachPreviewSurface(method_call, std::move(result));
  } else if (method_call.method_name().compare(
                 "markPreviewTextureFrameAvailable") == 0) {
    HandleMarkPreviewTextureFrameAvailable(method_call, std::move(result));
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

void BesfaFlutterPlugin::HandleAttachPreviewSurface(
    const flutter::MethodCall<flutter::EncodableValue> &method_call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  if (!texture_registrar_) {
    result->Error("texture_unavailable",
                  "Flutter texture registrar is unavailable.");
    return;
  }

  const auto *arguments =
      std::get_if<flutter::EncodableMap>(method_call.arguments());
  if (!arguments) {
    result->Error("invalid_surface", "Preview surface descriptor is invalid.");
    return;
  }

  const int width = GetOptionalIntArgument(*arguments, "width", 640);
  const int height = GetOptionalIntArgument(*arguments, "height", 360);
  const std::string shared_handle_name =
      GetOptionalStringArgument(*arguments, "shared_handle_name");
  auto texture = PreviewTexture::Attach(static_cast<size_t>(width),
                                        static_cast<size_t>(height),
                                        Utf8ToWide(shared_handle_name));
  if (!texture) {
    result->Error("texture_attach_failed",
                  "D3D12 shared preview surface could not be attached.");
    return;
  }

  const int64_t texture_id =
      texture_registrar_->RegisterTexture(texture->texture_variant());
  preview_textures_[texture_id] = texture;
  texture_registrar_->MarkTextureFrameAvailable(texture_id);
  result->Success(flutter::EncodableValue(texture_id));
}

void BesfaFlutterPlugin::HandleMarkPreviewTextureFrameAvailable(
    const flutter::MethodCall<flutter::EncodableValue> &method_call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  if (!texture_registrar_) {
    result->Success(flutter::EncodableValue(false));
    return;
  }

  const int64_t texture_id = GetTextureIdArgument(method_call.arguments());
  if (preview_textures_.find(texture_id) == preview_textures_.end()) {
    result->Success(flutter::EncodableValue(false));
    return;
  }

  texture_registrar_->MarkTextureFrameAvailable(texture_id);
  result->Success(flutter::EncodableValue(true));
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
