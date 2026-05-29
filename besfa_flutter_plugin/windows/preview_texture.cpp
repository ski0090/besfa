#include "preview_texture.h"

#include <algorithm>
#include <utility>

namespace besfa_flutter_plugin {
namespace {

constexpr size_t kMinimumTextureSize = 16;
constexpr size_t kMaximumTextureSize = 4096;

size_t ClampTextureSize(size_t value) {
  return std::clamp(value, kMinimumTextureSize, kMaximumTextureSize);
}

} // namespace

std::shared_ptr<PreviewTexture> PreviewTexture::Create(size_t width,
                                                       size_t height) {
  auto texture = std::shared_ptr<PreviewTexture>(
      new PreviewTexture(ClampTextureSize(width), ClampTextureSize(height)));
  if (!texture->Initialize()) {
    return nullptr;
  }

  return texture;
}

std::shared_ptr<PreviewTexture> PreviewTexture::Attach(
    size_t width, size_t height, std::wstring shared_handle_name) {
  auto texture = std::shared_ptr<PreviewTexture>(
      new PreviewTexture(ClampTextureSize(width), ClampTextureSize(height)));
  texture->shared_handle_name_ = std::move(shared_handle_name);
  if (!texture->InitializeAttached()) {
    return nullptr;
  }

  return texture;
}

PreviewTexture::PreviewTexture(size_t width, size_t height)
    : width_(width), height_(height) {}

PreviewTexture::~PreviewTexture() {
  if (shared_handle_) {
    CloseHandle(shared_handle_);
  }
  if (fence_event_) {
    CloseHandle(fence_event_);
  }
}

bool PreviewTexture::Initialize() {
  HRESULT result = D3D12CreateDevice(nullptr, D3D_FEATURE_LEVEL_11_0,
                                     IID_PPV_ARGS(&device_));
  if (FAILED(result)) {
    return false;
  }

  D3D12_COMMAND_QUEUE_DESC queue_desc = {};
  queue_desc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
  result =
      device_->CreateCommandQueue(&queue_desc, IID_PPV_ARGS(&command_queue_));
  if (FAILED(result)) {
    return false;
  }

  result = device_->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT,
                                           IID_PPV_ARGS(&command_allocator_));
  if (FAILED(result)) {
    return false;
  }

  result = device_->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT,
                                      command_allocator_.Get(), nullptr,
                                      IID_PPV_ARGS(&command_list_));
  if (FAILED(result)) {
    return false;
  }

  D3D12_DESCRIPTOR_HEAP_DESC rtv_heap_desc = {};
  rtv_heap_desc.NumDescriptors = 1;
  rtv_heap_desc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_RTV;
  result =
      device_->CreateDescriptorHeap(&rtv_heap_desc, IID_PPV_ARGS(&rtv_heap_));
  if (FAILED(result)) {
    return false;
  }

  D3D12_HEAP_PROPERTIES heap_properties = {};
  heap_properties.Type = D3D12_HEAP_TYPE_DEFAULT;

  D3D12_RESOURCE_DESC texture_desc = {};
  texture_desc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
  texture_desc.Width = width_;
  texture_desc.Height = static_cast<UINT>(height_);
  texture_desc.MipLevels = 1;
  texture_desc.DepthOrArraySize = 1;
  texture_desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
  texture_desc.SampleDesc.Count = 1;
  texture_desc.Layout = D3D12_TEXTURE_LAYOUT_UNKNOWN;
  texture_desc.Flags = D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET;

  D3D12_CLEAR_VALUE clear_value = {};
  clear_value.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
  clear_value.Color[0] = 0.08f;
  clear_value.Color[1] = 0.56f;
  clear_value.Color[2] = 0.47f;
  clear_value.Color[3] = 1.0f;

  result = device_->CreateCommittedResource(
      &heap_properties, D3D12_HEAP_FLAG_SHARED, &texture_desc,
      D3D12_RESOURCE_STATE_RENDER_TARGET, &clear_value,
      IID_PPV_ARGS(&texture_));
  if (FAILED(result)) {
    return false;
  }

  device_->CreateRenderTargetView(
      texture_.Get(), nullptr, rtv_heap_->GetCPUDescriptorHandleForHeapStart());

  if (!ClearTestPattern()) {
    return false;
  }

  result =
      device_->CreateFence(0, D3D12_FENCE_FLAG_NONE, IID_PPV_ARGS(&fence_));
  if (FAILED(result)) {
    return false;
  }

  fence_event_ = CreateEvent(nullptr, FALSE, FALSE, nullptr);
  if (!fence_event_) {
    return false;
  }

  if (!WaitForGpu()) {
    return false;
  }

  result = device_->CreateSharedHandle(texture_.Get(), nullptr, GENERIC_ALL,
                                       nullptr, &shared_handle_);
  if (FAILED(result)) {
    return false;
  }

  descriptor_.struct_size = sizeof(FlutterDesktopGpuSurfaceDescriptor);
  descriptor_.width = width_;
  descriptor_.height = height_;
  descriptor_.visible_width = width_;
  descriptor_.visible_height = height_;
  descriptor_.format = kFlutterDesktopPixelFormatBGRA8888;
  descriptor_.release_callback = ReleaseSurfaceHandle;

  texture_variant_ = std::make_unique<flutter::TextureVariant>(
      flutter::GpuSurfaceTexture(kFlutterDesktopGpuSurfaceTypeDxgiSharedHandle,
                                 [this](size_t width, size_t height) {
                                   return ObtainDescriptor(width, height);
                                 }));
  return true;
}

bool PreviewTexture::InitializeAttached() {
  if (shared_handle_name_.empty()) {
    return false;
  }

  HRESULT result = D3D12CreateDevice(nullptr, D3D_FEATURE_LEVEL_11_0,
                                     IID_PPV_ARGS(&device_));
  if (FAILED(result)) {
    return false;
  }

  result = device_->OpenSharedHandleByName(shared_handle_name_.c_str(),
                                           GENERIC_ALL, &shared_handle_);
  if (FAILED(result)) {
    return false;
  }

  descriptor_.struct_size = sizeof(FlutterDesktopGpuSurfaceDescriptor);
  descriptor_.width = width_;
  descriptor_.height = height_;
  descriptor_.visible_width = width_;
  descriptor_.visible_height = height_;
  descriptor_.format = kFlutterDesktopPixelFormatBGRA8888;
  descriptor_.release_callback = ReleaseSurfaceHandle;

  texture_variant_ = std::make_unique<flutter::TextureVariant>(
      flutter::GpuSurfaceTexture(kFlutterDesktopGpuSurfaceTypeDxgiSharedHandle,
                                 [this](size_t width, size_t height) {
                                   return ObtainDescriptor(width, height);
                                 }));
  return true;
}

bool PreviewTexture::ClearTestPattern() {
  const float clear_color[] = {0.08f, 0.56f, 0.47f, 1.0f};
  command_list_->ClearRenderTargetView(
      rtv_heap_->GetCPUDescriptorHandleForHeapStart(), clear_color, 0, nullptr);

  D3D12_RESOURCE_BARRIER barrier = {};
  barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
  barrier.Transition.pResource = texture_.Get();
  barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_RENDER_TARGET;
  barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_COMMON;
  barrier.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
  command_list_->ResourceBarrier(1, &barrier);

  HRESULT result = command_list_->Close();
  if (FAILED(result)) {
    return false;
  }

  ID3D12CommandList *command_lists[] = {command_list_.Get()};
  command_queue_->ExecuteCommandLists(1, command_lists);
  return true;
}

bool PreviewTexture::WaitForGpu() {
  const UINT64 fence_value = ++fence_value_;
  HRESULT result = command_queue_->Signal(fence_.Get(), fence_value);
  if (FAILED(result)) {
    return false;
  }

  if (fence_->GetCompletedValue() >= fence_value) {
    return true;
  }

  result = fence_->SetEventOnCompletion(fence_value, fence_event_);
  if (FAILED(result)) {
    return false;
  }

  WaitForSingleObject(fence_event_, INFINITE);
  return true;
}

const FlutterDesktopGpuSurfaceDescriptor *
PreviewTexture::ObtainDescriptor(size_t width, size_t height) {
  HANDLE frame_handle = nullptr;
  const bool duplicated =
      DuplicateHandle(GetCurrentProcess(), shared_handle_, GetCurrentProcess(),
                      &frame_handle, 0, FALSE, DUPLICATE_SAME_ACCESS);
  descriptor_.handle = duplicated ? frame_handle : nullptr;
  descriptor_.release_context = duplicated ? frame_handle : nullptr;
  return &descriptor_;
}

void PreviewTexture::ReleaseSurfaceHandle(void *release_context) {
  if (release_context) {
    CloseHandle(static_cast<HANDLE>(release_context));
  }
}

} // namespace besfa_flutter_plugin
