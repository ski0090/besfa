//! Renders the game's cameras into a texture the editor shares with it.
//!
//! The editor creates a D3D11 texture, publishes it under a named NT handle
//! and passes the name in `BESFA_VIEWPORT`. This module opens that texture on
//! Bevy's D3D12 device and points every window camera at it. When the editor
//! resizes its viewport it makes a new texture and sends its name in a
//! `viewport` command, and [`reopen`] switches to it.

use bevy::{
    camera::{ManualTextureViewHandle, RenderTarget},
    prelude::*,
    render::{
        renderer::RenderDevice,
        texture::{ManualTextureView, ManualTextureViews},
    },
    window::WindowRef,
};

const VIEWPORT_HANDLE: ManualTextureViewHandle = ManualTextureViewHandle(0xBE5FA);

/// The editor writes linear-to-sRGB encoded BGRA, which Flutter displays as is.
const VIEWPORT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;

#[derive(Resource, Clone)]
pub(crate) struct ViewportConfig {
    name: String,
}

impl ViewportConfig {
    /// Reads `BESFA_VIEWPORT`, the shared handle name set by the editor. The
    /// size comes from the texture itself.
    pub(crate) fn from_env() -> Option<Self> {
        Some(Self {
            name: std::env::var("BESFA_VIEWPORT").ok()?,
        })
    }
}

pub(crate) struct ViewportPlugin(pub(crate) ViewportConfig);

impl Plugin for ViewportPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone())
            // Before the game's Startup systems spawn their cameras.
            .add_systems(PreStartup, open_viewport)
            .add_observer(retarget_camera);
    }
}

/// Keeps the wgpu texture alive for as long as the view renders into it.
#[derive(Resource)]
struct ViewportTexture(
    #[expect(dead_code, reason = "held only to keep the texture alive")] wgpu::Texture,
);

fn open_viewport(
    mut commands: Commands,
    config: Res<ViewportConfig>,
    device: Res<RenderDevice>,
    mut views: ResMut<ManualTextureViews>,
    mut exit: MessageWriter<AppExit>,
) {
    let texture = match shared::open(device.wgpu_device(), &config.name) {
        Ok(texture) => texture,
        Err(error) => {
            // Without a window or a viewport the game would run unseen.
            error!(
                "Could not open the editor viewport '{}': {error}",
                config.name
            );
            exit.write(AppExit::error());
            return;
        }
    };

    let size = show(&mut views, &texture);
    commands.insert_resource(ViewportTexture(texture));
    info!("Rendering into the editor viewport {}x{}.", size.x, size.y);
}

/// Makes `texture` what the viewport's cameras render into.
fn show(views: &mut ManualTextureViews, texture: &wgpu::Texture) -> UVec2 {
    let size = UVec2::new(texture.width(), texture.height());
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
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
    let size = show(&mut world.resource_mut::<ManualTextureViews>(), &texture);
    world.insert_resource(ViewportTexture(texture));
    world.resource_mut::<ViewportConfig>().name = name.into();
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
    add: On<Add, Camera>,
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
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                },
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
