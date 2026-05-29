use besfa_ipc::{PreviewSurfacePayload, preview_surface_ready_message};
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::{RenderAssets, prepare_assets},
        render_resource::{
            Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
            TextureViewDescriptor,
        },
        renderer::RenderDevice,
        texture::{DefaultImageSampler, GpuImage},
        view::prepare_view_attachments,
    },
};
use std::sync::{
    Mutex,
    mpsc::{self, Receiver, Sender, TryRecvError},
};

use crate::runtime_ipc::RuntimeIpcServer;

pub(crate) const PREVIEW_SURFACE_WIDTH: u32 = 640;
pub(crate) const PREVIEW_SURFACE_HEIGHT: u32 = 360;
pub(crate) const PREVIEW_SURFACE_FORMAT: &str = "bgra8_unorm";

/// Installs the runtime-owned shared preview render target bridge.
pub(crate) struct BesfaExternalPreviewPlugin;

impl Plugin for BesfaExternalPreviewPlugin {
    fn build(&self, app: &mut App) {
        let (event_tx, event_rx) = mpsc::channel();
        app.insert_resource(PreviewSurfaceEventReceiver(Mutex::new(event_rx)))
            .init_resource::<PreviewSurfaceEventState>()
            .add_plugins(ExtractResourcePlugin::<PreviewSurfaceTarget>::default())
            .add_systems(Update, broadcast_preview_surface_events);

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(PreviewSurfaceEventSender(event_tx))
                .init_resource::<PreviewSurfaceGpuState>()
                .add_systems(
                    Render,
                    prepare_shared_preview_surface
                        .in_set(RenderSystems::ManageViews)
                        .after(prepare_assets::<GpuImage>)
                        .before(prepare_view_attachments),
                );
        }
    }
}

/// Creates the main-world image used for camera target sizing.
pub(crate) fn create_preview_surface_image(
    images: &mut Assets<Image>,
) -> (Handle<Image>, PreviewSurfaceTarget) {
    let size = Extent3d {
        width: PREVIEW_SURFACE_WIDTH,
        height: PREVIEW_SURFACE_HEIGHT,
        depth_or_array_layers: 1,
    };
    let mut image = Image::new_uninit(
        size,
        TextureDimension::D2,
        TextureFormat::Bgra8Unorm,
        RenderAssetUsages::MAIN_WORLD,
    );
    image.texture_descriptor.label = Some("besfa_preview_surface");
    image.texture_descriptor.usage =
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC;

    let image_handle = images.add(image);
    let target = PreviewSurfaceTarget {
        image: image_handle.clone(),
        shared_handle_name: format!("Local\\BesfaPreviewSurface-{}", std::process::id()),
        width: PREVIEW_SURFACE_WIDTH,
        height: PREVIEW_SURFACE_HEIGHT,
        format: PREVIEW_SURFACE_FORMAT.to_string(),
    };

    (image_handle, target)
}

#[derive(Clone, Resource, ExtractResource)]
pub(crate) struct PreviewSurfaceTarget {
    image: Handle<Image>,
    shared_handle_name: String,
    width: u32,
    height: u32,
    format: String,
}

#[derive(Resource)]
struct PreviewSurfaceEventReceiver(Mutex<Receiver<PreviewSurfacePayload>>);

#[derive(Default, Resource)]
struct PreviewSurfaceEventState {
    latest: Option<PreviewSurfacePayload>,
    rebroadcast_elapsed_secs: f32,
}

#[derive(Resource)]
struct PreviewSurfaceEventSender(Sender<PreviewSurfacePayload>);

#[derive(Default, Resource)]
struct PreviewSurfaceGpuState {
    published_handle_name: Option<String>,
    _shared_handle: Option<SharedPreviewHandle>,
}

fn broadcast_preview_surface_events(
    receiver: Res<PreviewSurfaceEventReceiver>,
    mut state: ResMut<PreviewSurfaceEventState>,
    time: Res<Time>,
    server: Option<Res<RuntimeIpcServer>>,
) {
    let Ok(receiver) = receiver.0.lock() else {
        return;
    };

    let mut changed = false;
    loop {
        match receiver.try_recv() {
            Ok(payload) => {
                state.latest = Some(payload);
                state.rebroadcast_elapsed_secs = 0.0;
                changed = true;
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }

    let Some(server) = server else {
        return;
    };
    let Some(payload) = state.latest.clone() else {
        return;
    };

    state.rebroadcast_elapsed_secs += time.delta_secs();
    if changed || state.rebroadcast_elapsed_secs >= 1.0 {
        server.broadcast(preview_surface_ready_message(payload));
        state.rebroadcast_elapsed_secs = 0.0;
    }
}

fn prepare_shared_preview_surface(
    target: Option<Res<PreviewSurfaceTarget>>,
    render_device: Res<RenderDevice>,
    default_sampler: Res<DefaultImageSampler>,
    mut gpu_images: ResMut<RenderAssets<GpuImage>>,
    mut state: ResMut<PreviewSurfaceGpuState>,
    sender: Res<PreviewSurfaceEventSender>,
) {
    let Some(target) = target else {
        return;
    };
    if state.published_handle_name.as_deref() == Some(target.shared_handle_name.as_str()) {
        return;
    }

    match create_shared_gpu_image(&render_device, &default_sampler, &target) {
        Ok((gpu_image, shared_handle)) => {
            gpu_images.insert(target.image.id(), gpu_image);
            state.published_handle_name = Some(target.shared_handle_name.clone());
            state._shared_handle = Some(shared_handle);
            let _ = sender.0.send(PreviewSurfacePayload {
                shared_handle_name: target.shared_handle_name.clone(),
                width: target.width,
                height: target.height,
                format: target.format.clone(),
            });
        }
        Err(error) => {
            bevy::log::warn!("Preview shared surface could not be prepared: {error}");
        }
    }
}

#[cfg(windows)]
fn create_shared_gpu_image(
    render_device: &RenderDevice,
    default_sampler: &DefaultImageSampler,
    target: &PreviewSurfaceTarget,
) -> Result<(GpuImage, SharedPreviewHandle), String> {
    use windows::{
        Win32::{
            Foundation::GENERIC_ALL,
            Graphics::{
                Direct3D12::{
                    D3D12_CLEAR_VALUE, D3D12_HEAP_FLAG_SHARED, D3D12_HEAP_PROPERTIES,
                    D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_DESC,
                    D3D12_RESOURCE_DIMENSION_TEXTURE2D, D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
                    D3D12_RESOURCE_STATE_COMMON, D3D12_TEXTURE_LAYOUT_UNKNOWN,
                },
                Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
            },
        },
        core::HSTRING,
    };

    let wgpu_device = render_device.wgpu_device();
    let Some(hal_device) = (unsafe { wgpu_device.as_hal::<wgpu_hal::api::Dx12>() }) else {
        return Err("runtime is not using the DX12 wgpu backend".to_string());
    };

    let raw_device = hal_device.raw_device();
    let texture_desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        Alignment: 0,
        Width: target.width as u64,
        Height: target.height,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
    };
    let heap_properties = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_DEFAULT,
        ..Default::default()
    };
    let clear_value = D3D12_CLEAR_VALUE {
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        Anonymous: windows::Win32::Graphics::Direct3D12::D3D12_CLEAR_VALUE_0 {
            Color: [0.06, 0.07, 0.08, 1.0],
        },
    };

    let mut resource = None;
    unsafe {
        raw_device
            .CreateCommittedResource(
                &heap_properties,
                D3D12_HEAP_FLAG_SHARED,
                &texture_desc,
                D3D12_RESOURCE_STATE_COMMON,
                Some(&clear_value),
                &mut resource,
            )
            .map_err(|error| format!("CreateCommittedResource failed: {error}"))?;
    }
    let resource =
        resource.ok_or_else(|| "CreateCommittedResource returned no resource".to_string())?;

    let shared_name = HSTRING::from(target.shared_handle_name.as_str());
    let shared_handle = unsafe {
        raw_device
            .CreateSharedHandle(&resource, None, GENERIC_ALL.0, &shared_name)
            .map_err(|error| format!("CreateSharedHandle failed: {error}"))?
    };

    let size = Extent3d {
        width: target.width,
        height: target.height,
        depth_or_array_layers: 1,
    };
    let hal_texture = unsafe {
        wgpu_hal::dx12::Device::texture_from_raw(
            resource,
            TextureFormat::Bgra8Unorm,
            TextureDimension::D2,
            size,
            1,
            1,
        )
    };
    let texture = unsafe {
        wgpu_device.create_texture_from_hal::<wgpu_hal::api::Dx12>(
            hal_texture,
            &TextureDescriptor {
                label: Some("besfa_preview_surface_shared"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Bgra8Unorm,
                usage: TextureUsages::RENDER_ATTACHMENT
                    | TextureUsages::TEXTURE_BINDING
                    | TextureUsages::COPY_SRC,
                view_formats: &[],
            },
        )
    };
    let texture_view = texture.create_view(&TextureViewDescriptor::default());

    Ok((
        GpuImage {
            texture: texture.into(),
            texture_view: texture_view.into(),
            texture_format: TextureFormat::Bgra8Unorm,
            texture_view_format: None,
            sampler: (***default_sampler).clone().into(),
            size,
            mip_level_count: 1,
            had_data: false,
        },
        SharedPreviewHandle(shared_handle),
    ))
}

#[cfg(not(windows))]
fn create_shared_gpu_image(
    _render_device: &RenderDevice,
    _default_sampler: &DefaultImageSampler,
    _target: &PreviewSurfaceTarget,
) -> Result<(GpuImage, SharedPreviewHandle), String> {
    Err("shared preview surfaces are only implemented on Windows".to_string())
}

#[cfg(windows)]
struct SharedPreviewHandle(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for SharedPreviewHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
unsafe impl Send for SharedPreviewHandle {}

#[cfg(windows)]
unsafe impl Sync for SharedPreviewHandle {}

#[cfg(not(windows))]
struct SharedPreviewHandle;
