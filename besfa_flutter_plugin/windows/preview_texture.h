#ifndef FLUTTER_PLUGIN_BESFA_PREVIEW_TEXTURE_H_
#define FLUTTER_PLUGIN_BESFA_PREVIEW_TEXTURE_H_

#include <d3d12.h>
#include <flutter/texture_registrar.h>
#include <windows.h>
#include <wrl/client.h>

#include <memory>

namespace besfa_flutter_plugin {

class PreviewTexture {
public:
  static std::shared_ptr<PreviewTexture> Create(size_t width, size_t height);

  PreviewTexture(const PreviewTexture &) = delete;
  PreviewTexture &operator=(const PreviewTexture &) = delete;
  ~PreviewTexture();

  flutter::TextureVariant *texture_variant() { return texture_variant_.get(); }

private:
  PreviewTexture(size_t width, size_t height);

  bool Initialize();
  bool ClearTestPattern();
  bool WaitForGpu();
  const FlutterDesktopGpuSurfaceDescriptor *ObtainDescriptor(size_t width,
                                                             size_t height);

  static void ReleaseSurfaceHandle(void *release_context);

  size_t width_;
  size_t height_;
  Microsoft::WRL::ComPtr<ID3D12Device> device_;
  Microsoft::WRL::ComPtr<ID3D12CommandQueue> command_queue_;
  Microsoft::WRL::ComPtr<ID3D12CommandAllocator> command_allocator_;
  Microsoft::WRL::ComPtr<ID3D12GraphicsCommandList> command_list_;
  Microsoft::WRL::ComPtr<ID3D12DescriptorHeap> rtv_heap_;
  Microsoft::WRL::ComPtr<ID3D12Resource> texture_;
  Microsoft::WRL::ComPtr<ID3D12Fence> fence_;
  UINT64 fence_value_ = 0;
  HANDLE fence_event_ = nullptr;
  HANDLE shared_handle_ = nullptr;
  FlutterDesktopGpuSurfaceDescriptor descriptor_ = {};
  std::unique_ptr<flutter::TextureVariant> texture_variant_;
};

} // namespace besfa_flutter_plugin

#endif // FLUTTER_PLUGIN_BESFA_PREVIEW_TEXTURE_H_
