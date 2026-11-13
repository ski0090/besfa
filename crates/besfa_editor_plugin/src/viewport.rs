//! Renders the game's cameras into a texture the editor shares with it.
//!
//! The editor creates a D3D12 texture, publishes it under a named NT handle
//! and passes the name in `BESFA_VIEWPORT`. This module opens that texture on
//! Bevy's D3D12 device. Every window camera draws into a texture of the game's
//! own of the same size, which is copied into the editor's once all cameras
//! have drawn the frame: the editor reads its texture whenever it likes, and
//! would otherwise show the scene from before the panels the game draws over
//! it. When the editor resizes its viewport it makes a new texture and sends
//! its name in a `viewport` command, and [`reopen`] switches to it.

use bevy::{
    camera::{ManualTextureViewHandle, RenderTarget},
    prelude::*,
    render::{
        Extract, ExtractSchedule, RenderApp,
        renderer::{RenderDevice, RenderGraph, RenderGraphSystems, RenderQueue},
        texture::{ManualTextureView, ManualTextureViews},
    },
    window::WindowRef,
};

pub(crate) const VIEWPORT_HANDLE: ManualTextureViewHandle = ManualTextureViewHandle(0xBE5FA);

/// The game writes linear-to-sRGB encoded BGRA, which the editor shows as is.
const VIEWPORT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;

/// In [`crate::default_plugins`] when the editor set `BESFA_VIEWPORT`.
pub(crate) struct ViewportPlugin;

impl Plugin for ViewportPlugin {
    fn build(&self, app: &mut App) {
        // Before the game's Startup systems spawn their cameras.
        app.add_systems(PreStartup, open_viewport)
            .add_observer(retarget_camera);
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .add_systems(ExtractSchedule, extract_viewport)
                // After every camera's work for the frame is submitted.
                .add_systems(RenderGraph, copy_frame.in_set(RenderGraphSystems::Finish));
        }
    }
}

/// The texture the viewport's cameras draw into, and the editor's texture
/// each frame is copied into. Held for as long as the view renders.
#[derive(Resource, Clone)]
pub(crate) struct ViewportTexture {
    frame: wgpu::Texture,
    shared: wgpu::Texture,
}

impl ViewportTexture {
    /// The editor's `shared` texture, and one of the game's own of its size
    /// to draw into.
    fn new(device: &RenderDevice, shared: wgpu::Texture) -> Self {
        let frame = device
            .wgpu_device()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("besfa_viewport_frame"),
                size: shared.size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: VIEWPORT_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
        Self { frame, shared }
    }
}

/// The render world copies what the main world opened.
fn extract_viewport(mut commands: Commands, viewport: Extract<Option<Res<ViewportTexture>>>) {
    if let Some(viewport) = viewport.as_ref()
        && viewport.is_changed()
    {
        commands.insert_resource(ViewportTexture::clone(viewport));
    }
}

/// Copies the frame every camera has drawn into the editor's texture, in
/// one go.
fn copy_frame(
    viewport: Option<Res<ViewportTexture>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let Some(viewport) = viewport else {
        return;
    };
    let mut encoder = device
        .wgpu_device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_texture(
        viewport.frame.as_image_copy(),
        viewport.shared.as_image_copy(),
        viewport.frame.size(),
    );
    queue.submit([encoder.finish()]);
}

/// Opens the texture `BESFA_VIEWPORT` names, the editor's shared handle.
/// The size comes from the texture itself.
fn open_viewport(
    mut commands: Commands,
    device: Res<RenderDevice>,
    mut views: ResMut<ManualTextureViews>,
    mut exit: MessageWriter<AppExit>,
    opened: Option<Res<ViewportTexture>>,
) {
    // Stop runs the startup schedules again in this process. The viewport is
    // still open, and after a resize it is another texture than the one
    // `BESFA_VIEWPORT` names, which the editor has let go of.
    if opened.is_some() {
        return;
    }
    let name = std::env::var("BESFA_VIEWPORT").unwrap_or_default();
    let texture = match shared::open(device.wgpu_device(), &name) {
        Ok(texture) => texture,
        Err(error) => {
            // Without a window or a viewport the game would run unseen.
            error!("Could not open the editor viewport '{name}': {error}");
            exit.write(AppExit::error());
            return;
        }
    };

    let viewport = ViewportTexture::new(&device, texture);
    let size = show(&mut views, &viewport);
    commands.insert_resource(viewport);
    info!("Rendering into the editor viewport {}x{}.", size.x, size.y);
}

/// Makes the frame of `viewport` what its cameras render into.
fn show(views: &mut ManualTextureViews, viewport: &ViewportTexture) -> UVec2 {
    let frame = &viewport.frame;
    let size = UVec2::new(frame.width(), frame.height());
    let view = frame.create_view(&wgpu::TextureViewDescriptor::default());
    views.insert(
        VIEWPORT_HANDLE,
        ManualTextureView {
            texture_view: view.into(),
            size,
            view_format: VIEWPORT_FORMAT,
        },
    );
    size
}

/// Switches to the editor's texture `name`, after the editor resized the
/// viewport. The old texture is dropped once nothing renders into it.
pub(crate) fn reopen(world: &mut World, name: &str) -> Result<(), String> {
    if world.get_resource::<ViewportTexture>().is_none() {
        return Err("the game was not started in the editor viewport".into());
    }
    let device = world.resource::<RenderDevice>().clone();
    let texture = shared::open(device.wgpu_device(), name)?;
    let viewport = ViewportTexture::new(&device, texture);
    let size = show(&mut world.resource_mut::<ManualTextureViews>(), &viewport);
    world.insert_resource(viewport);
    // Cameras recompute their target's size when their projection changes.
    let mut projections = world.query_filtered::<&mut Projection, With<Camera>>();
    for mut projection in projections.iter_mut(world) {
        projection.set_changed();
    }
    info!("Rendering into the editor viewport {}x{}.", size.x, size.y);
    Ok(())
}

/// Sends cameras that would render to the (absent) primary window into the
/// viewport. Cameras with any other target are left alone, and so is every
/// camera when the viewport could not be opened: a missing view would panic.
fn retarget_camera(
    add: On<Add<Camera>>,
    viewport: Option<Res<ViewportTexture>>,
    mut targets: Query<&mut RenderTarget>,
) {
    if viewport.is_some()
        && let Ok(mut target) = targets.get_mut(add.entity)
        && matches!(*target, RenderTarget::Window(WindowRef::Primary))
    {
        *target = RenderTarget::TextureView(VIEWPORT_HANDLE);
    }
}

#[cfg(windows)]
mod shared {
    use wgpu::hal::{api::Dx12, dx12};
    use windows::{
        Win32::{
            Foundation::{CloseHandle, GENERIC_ALL},
            Graphics::{
                Direct3D12::{D3D12_RESOURCE_DIMENSION_TEXTURE2D, ID3D12Resource},
                Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
            },
        },
        core::HSTRING,
    };

    use super::VIEWPORT_FORMAT;

    /// Opens the editor's named shared texture on Bevy's D3D12 device. Size
    /// and format are read from the resource, not trusted from the editor.
    pub(super) fn open(device: &wgpu::Device, name: &str) -> Result<wgpu::Texture, String> {
        // SAFETY: the device is only used to open a resource, and the raw
        // texture is handed straight to wgpu with its own description.
        let (hal_texture, extent) = unsafe {
            let hal = device
                .as_hal::<Dx12>()
                .ok_or("the renderer is not using DX12; set WGPU_BACKEND=dx12")?;
            let raw = hal.raw_device();

            let handle = raw
                .OpenSharedHandleByName(&HSTRING::from(name), GENERIC_ALL.0)
                .map_err(|error| format!("OpenSharedHandleByName failed: {error}"))?;
            let mut resource: Option<ID3D12Resource> = None;
            let opened = raw.OpenSharedHandle(handle, &mut resource);
            let _ = CloseHandle(handle);
            opened.map_err(|error| format!("OpenSharedHandle failed: {error}"))?;
            let resource = resource.ok_or("OpenSharedHandle returned no resource")?;

            let desc = resource.GetDesc();
            if desc.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D
                || desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
                || desc.DepthOrArraySize != 1
                || desc.MipLevels != 1
                || desc.SampleDesc.Count != 1
            {
                return Err(format!(
                    "expected a single-sample BGRA8 sRGB 2D texture, got {:?} {:?}",
                    desc.Dimension, desc.Format
                ));
            }
            let extent = wgpu::Extent3d {
                width: u32::try_from(desc.Width).map_err(|_| "the texture is too wide")?,
                height: desc.Height,
                depth_or_array_layers: 1,
            };

            let texture = dx12::Device::texture_from_raw(
                resource,
                VIEWPORT_FORMAT,
                wgpu::TextureDimension::D2,
                extent,
                1,
                1,
            );
            (texture, extent)
        };

        // SAFETY: the description matches the resource, checked above.
        Ok(unsafe {
            device.create_texture_from_hal::<Dx12>(
                hal_texture,
                &wgpu::TextureDescriptor {
                    label: Some("besfa_viewport"),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: VIEWPORT_FORMAT,
                    // Each frame is copied in whole ([`super::copy_frame`]).
                    usage: wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                // A shared resource starts in the common state.
                wgpu::TextureUses::UNINITIALIZED,
            )
        })
    }
}

#[cfg(not(windows))]
mod shared {
    pub(super) fn open(_: &wgpu::Device, _: &str) -> Result<wgpu::Texture, String> {
        Err("the editor viewport is only supported on Windows".into())
    }
}
