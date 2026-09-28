#ifndef RUNNER_VIEWPORT_TEXTURE_H_
#define RUNNER_VIEWPORT_TEXTURE_H_

#include <d3d11.h>
#include <flutter/method_channel.h>
#include <flutter/plugin_registrar_windows.h>
#include <flutter/texture_registrar.h>
#include <wrl/client.h>

#include <atomic>
#include <map>
#include <memory>
#include <string>
#include <thread>

// A texture the game renders into and Flutter displays.
//
// The game opens |shared_| by name on its own D3D12 device and renders into
// it. Flutter's ANGLE device can only open legacy DXGI handles, so each frame
// is copied into |display_|, which Flutter samples.
class ViewportTexture {
 public:
  // Returns nullptr and fills |error| on failure.
  static std::unique_ptr<ViewportTexture> Create(
      IDXGIAdapter* adapter, flutter::TextureRegistrar* registrar,
      UINT width, UINT height, std::string* error);

  ~ViewportTexture();

  int64_t id() const { return id_; }
  const std::wstring& name() const { return name_; }

  // Stops frame notifications. Call before unregistering the texture.
  void Stop();

 private:
  ViewportTexture() = default;

  const FlutterDesktopGpuSurfaceDescriptor* ObtainDescriptor();

  Microsoft::WRL::ComPtr<ID3D11Device> device_;
  Microsoft::WRL::ComPtr<ID3D11DeviceContext> context_;
  Microsoft::WRL::ComPtr<ID3D11Texture2D> shared_;
  Microsoft::WRL::ComPtr<ID3D11Texture2D> display_;
  Microsoft::WRL::ComPtr<ID3D11Query> copy_done_;
  // Keeps the named handle alive so the game can open it by name.
  HANDLE shared_handle_ = nullptr;
  std::wstring name_;

  FlutterDesktopGpuSurfaceDescriptor descriptor_ = {};
  std::unique_ptr<flutter::TextureVariant> variant_;
  flutter::TextureRegistrar* registrar_ = nullptr;
  int64_t id_ = -1;

  std::atomic<bool> running_ = false;
  std::thread ticker_;
};

// Serves the "besfa/viewport" channel:
//   create {width, height} -> {textureId, name, adapter}
//   dispose textureId
class ViewportChannel {
 public:
  explicit ViewportChannel(flutter::PluginRegistrarWindows* registrar);
  ~ViewportChannel();

 private:
  void HandleMethodCall(
      const flutter::MethodCall<flutter::EncodableValue>& call,
      std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);

  flutter::PluginRegistrarWindows* registrar_;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>> channel_;
  std::map<int64_t, std::unique_ptr<ViewportTexture>> textures_;
};

#endif  // RUNNER_VIEWPORT_TEXTURE_H_
