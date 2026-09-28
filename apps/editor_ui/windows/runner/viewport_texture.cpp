#include "viewport_texture.h"

#include <dxgi1_2.h>
#include <flutter/standard_method_codec.h>

#include <chrono>
#include <optional>

#include "utils.h"

using Microsoft::WRL::ComPtr;

namespace {

// Flutter asks for a new frame at most this often while the game runs.
constexpr auto kFrameInterval = std::chrono::milliseconds(16);

std::string HresultMessage(const char* what, HRESULT hr) {
  char buffer[64];
  snprintf(buffer, sizeof(buffer), " failed (0x%08lX).",
           static_cast<unsigned long>(hr));
  return std::string(what) + buffer;
}

std::wstring NextSharedName() {
  static std::atomic<int> counter = 0;
  return L"Local\\besfa_viewport_" + std::to_wstring(GetCurrentProcessId()) +
         L"_" + std::to_wstring(counter++);
}

std::string AdapterName(IDXGIAdapter* adapter) {
  DXGI_ADAPTER_DESC desc;
  return SUCCEEDED(adapter->GetDesc(&desc)) ? Utf8FromUtf16(desc.Description)
                                            : std::string();
}

std::optional<int64_t> ToInt(const flutter::EncodableValue* value) {
  if (!value) return std::nullopt;
  if (auto v = std::get_if<int32_t>(value)) return *v;
  if (auto v = std::get_if<int64_t>(value)) return *v;
  return std::nullopt;
}

}  // namespace

std::unique_ptr<ViewportTexture> ViewportTexture::Create(
    IDXGIAdapter* adapter, flutter::TextureRegistrar* registrar, UINT width,
    UINT height, std::string* error) {
  std::unique_ptr<ViewportTexture> texture(new ViewportTexture());

  // Same adapter as Flutter, so both devices can open the shared textures.
  HRESULT hr = D3D11CreateDevice(
      adapter, D3D_DRIVER_TYPE_UNKNOWN, nullptr,
      D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
      &texture->device_, nullptr, &texture->context_);
  if (FAILED(hr)) {
    *error = HresultMessage("D3D11CreateDevice", hr);
    return nullptr;
  }

  D3D11_TEXTURE2D_DESC desc = {};
  desc.Width = width;
  desc.Height = height;
  desc.MipLevels = 1;
  desc.ArraySize = 1;
  desc.SampleDesc.Count = 1;
  desc.Usage = D3D11_USAGE_DEFAULT;
  desc.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;

  // Written by the game; sRGB so its shaders encode colors on write.
  desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM_SRGB;
  desc.MiscFlags =
      D3D11_RESOURCE_MISC_SHARED | D3D11_RESOURCE_MISC_SHARED_NTHANDLE;
  hr = texture->device_->CreateTexture2D(&desc, nullptr, &texture->shared_);
  if (FAILED(hr)) {
    *error = HresultMessage("CreateTexture2D (shared)", hr);
    return nullptr;
  }

  ComPtr<IDXGIResource1> shared_resource;
  texture->shared_.As(&shared_resource);
  texture->name_ = NextSharedName();
  hr = shared_resource->CreateSharedHandle(
      nullptr, DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE,
      texture->name_.c_str(), &texture->shared_handle_);
  if (FAILED(hr)) {
    *error = HresultMessage("CreateSharedHandle", hr);
    return nullptr;
  }

  // Read by Flutter. Same bytes, read without sRGB decoding.
  desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
  desc.MiscFlags = D3D11_RESOURCE_MISC_SHARED;
  hr = texture->device_->CreateTexture2D(&desc, nullptr, &texture->display_);
  if (FAILED(hr)) {
    *error = HresultMessage("CreateTexture2D (display)", hr);
    return nullptr;
  }

  ComPtr<IDXGIResource> display_resource;
  texture->display_.As(&display_resource);
  HANDLE display_handle = nullptr;
  hr = display_resource->GetSharedHandle(&display_handle);
  if (FAILED(hr)) {
    *error = HresultMessage("GetSharedHandle", hr);
    return nullptr;
  }

  // Start from black instead of whatever the memory held.
  ComPtr<ID3D11RenderTargetView> view;
  if (SUCCEEDED(texture->device_->CreateRenderTargetView(
          texture->shared_.Get(), nullptr, &view))) {
    const float black[4] = {0, 0, 0, 1};
    texture->context_->ClearRenderTargetView(view.Get(), black);
  }

  D3D11_QUERY_DESC query_desc = {D3D11_QUERY_EVENT, 0};
  texture->device_->CreateQuery(&query_desc, &texture->copy_done_);

  texture->descriptor_.struct_size = sizeof(FlutterDesktopGpuSurfaceDescriptor);
  texture->descriptor_.handle = display_handle;
  texture->descriptor_.width = texture->descriptor_.visible_width = width;
  texture->descriptor_.height = texture->descriptor_.visible_height = height;
  texture->descriptor_.format = kFlutterDesktopPixelFormatBGRA8888;

  auto* raw = texture.get();
  texture->variant_ =
      std::make_unique<flutter::TextureVariant>(flutter::GpuSurfaceTexture(
          kFlutterDesktopGpuSurfaceTypeDxgiSharedHandle,
          [raw](size_t, size_t) { return raw->ObtainDescriptor(); }));
  texture->registrar_ = registrar;
  texture->id_ = registrar->RegisterTexture(texture->variant_.get());

  // ponytail: fixed-rate polling, no frame signal from the game. Replace with
  // a shared fence the game signals once frames need to be exact.
  texture->running_ = true;
  texture->ticker_ = std::thread([raw] {
    while (raw->running_) {
      raw->registrar_->MarkTextureFrameAvailable(raw->id_);
      std::this_thread::sleep_for(kFrameInterval);
    }
  });

  return texture;
}

ViewportTexture::~ViewportTexture() {
  Stop();
  if (shared_handle_) CloseHandle(shared_handle_);
}

void ViewportTexture::Stop() {
  running_ = false;
  if (ticker_.joinable()) ticker_.join();
}

// Runs on Flutter's raster thread, the only thread using |context_| after
// creation.
const FlutterDesktopGpuSurfaceDescriptor* ViewportTexture::ObtainDescriptor() {
  // ponytail: no lock against the game's writes, so a frame can tear. Add a
  // shared fence or keyed mutex if that shows up.
  context_->CopyResource(display_.Get(), shared_.Get());
  context_->End(copy_done_.Get());
  context_->Flush();
  // Flutter reads |display_| on another device right after this returns.
  while (context_->GetData(copy_done_.Get(), nullptr, 0, 0) == S_FALSE) {
    std::this_thread::yield();
  }
  return &descriptor_;
}

ViewportChannel::ViewportChannel(flutter::PluginRegistrarWindows* registrar)
    : registrar_(registrar) {
  channel_ = std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
      registrar->messenger(), "besfa/viewport",
      &flutter::StandardMethodCodec::GetInstance());
  channel_->SetMethodCallHandler([this](const auto& call, auto result) {
    HandleMethodCall(call, std::move(result));
  });
}

ViewportChannel::~ViewportChannel() {
  channel_->SetMethodCallHandler(nullptr);
  // The engine is shutting down with the window; only stop the threads.
  for (auto& [id, texture] : textures_) texture->Stop();
}

void ViewportChannel::HandleMethodCall(
    const flutter::MethodCall<flutter::EncodableValue>& call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  if (call.method_name() == "create") {
    const auto* args = std::get_if<flutter::EncodableMap>(call.arguments());
    auto arg = [args](const char* key) -> std::optional<int64_t> {
      if (!args) return std::nullopt;
      auto it = args->find(flutter::EncodableValue(key));
      return it == args->end() ? std::nullopt : ToInt(&it->second);
    };
    const auto width = arg("width"), height = arg("height");
    if (!width || !height || *width <= 0 || *height <= 0) {
      result->Error("bad_args", "width and height must be positive integers.");
      return;
    }

    ComPtr<IDXGIAdapter> adapter;
    if (!registrar_->GetGraphicsAdapter(&adapter)) {
      result->Error("no_adapter", "Flutter is not rendering with a GPU.");
      return;
    }

    std::string error;
    auto texture = ViewportTexture::Create(
        adapter.Get(), registrar_->texture_registrar(),
        static_cast<UINT>(*width), static_cast<UINT>(*height), &error);
    if (!texture) {
      result->Error("create_failed", error);
      return;
    }

    const int64_t id = texture->id();
    flutter::EncodableMap response = {
        {flutter::EncodableValue("textureId"), flutter::EncodableValue(id)},
        {flutter::EncodableValue("name"),
         flutter::EncodableValue(Utf8FromUtf16(texture->name().c_str()))},
        {flutter::EncodableValue("adapter"),
         flutter::EncodableValue(AdapterName(adapter.Get()))},
    };
    textures_[id] = std::move(texture);
    result->Success(flutter::EncodableValue(response));
  } else if (call.method_name() == "dispose") {
    const auto id = ToInt(call.arguments());
    auto it = id ? textures_.find(*id) : textures_.end();
    if (it == textures_.end()) {
      result->Success();
      return;
    }
    it->second->Stop();
    // Flutter may still be sampling it; free it once the engine lets go.
    registrar_->texture_registrar()->UnregisterTexture(
        *id, [texture = it->second.release()] { delete texture; });
    textures_.erase(it);
    result->Success();
  } else {
    result->NotImplemented();
  }
}
