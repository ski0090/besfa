//! Renders the game's cameras into a texture the editor shares with it.
//!
//! The editor creates a D3D11 texture, publishes it under a named NT handle
//! and passes the name in `BESFA_VIEWPORT`. This module opens that texture on
//! Bevy's D3D12 device and points every window camera at it.

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
    size: UVec2,
}

impl ViewportConfig {
    /// Reads `BESFA_VIEWPORT` (shared handle name) and `BESFA_VIEWPORT_SIZE`
    /// (`<width>x<height>`), set by the editor.
    pub(crate) fn from_env() -> Option<Self> {
        let name = std::env::var("BESFA_VIEWPORT").ok()?;
        let size = std::env::var("BESFA_VIEWPORT_SIZE").ok()?;
        Some(Self {
            name,
            size: parse_size(&size)?,
        })
    }
}

fn parse_size(size: &str) -> Option<UVec2> {
    let (width, height) = size.split_once('x')?;
    let size = UVec2::new(width.trim().parse().ok()?, height.trim().parse().ok()?);
    (size.x > 0 && size.y > 0).then_some(size)
}

pub(crate) struct ViewportPlugin(pub(crate) ViewportConfig);

impl Plugin for ViewportPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone())
            .add_systems(Startup, open_viewport)
            .add_observer(retarget_camera);
    }
}

/// Keeps the wgpu texture alive for as long as the view renders into it.
#[derive(Resource)]
struct ViewportTexture(#[expect(dead_code, reason = "held only to keep the texture alive")] wgpu::Texture);

fn open_viewport(
    mut commands: Commands,
    config: Res<ViewportConfig>,
    device: Res<RenderDevice>,
    mut views: ResMut<ManualTextureViews>,
) {
    let texture = match shared::open(device.wgpu_device(), &config.name, config.size) {
        Ok(texture) => texture,
        Err(error) => {
            error!("Could not open the editor viewport '{}': {error}", config.name);
            return;
        }
    };

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    views.insert(
        VIEWPORT_HANDLE,
        ManualTextureView {
            texture_view: view.into(),
            size: config.size,
            view_format: VIEWPORT_FORMAT,
        },
    );
    commands.insert_resource(ViewportTexture(texture));
    info!("Rendering into the editor viewport {}x{}.", config.size.x, config.size.y);
}

/// Sends cameras that would render to the (absent) primary window into the
/// viewport. Cameras with any other target are left alone.
fn retarget_camera(add: On<Add, Camera>, mut targets: Query<&mut RenderTarget>) {
    if let Ok(mut target) = targets.get_mut(add.entity)
        && matches!(*target, RenderTarget::Window(WindowRef::Primary))
    {
        *target = RenderTarget::TextureView(VIEWPORT_HANDLE);
    }
}

#[cfg(windows)]
mod shared {
    use bevy::math::UVec2;
    use windows::{
        Win32::{
            Foundation::{CloseHandle, GENERIC_ALL},
            Graphics::Direct3D12::ID3D12Resource,
        },
        core::HSTRING,
    };
    use wgpu::hal::{api::Dx12, dx12};

    use super::VIEWPORT_FORMAT;

    /// Opens the editor's named shared texture on Bevy's D3D12 device.
    pub(super) fn open(
        device: &wgpu::Device,
        name: &str,
        size: UVec2,
    ) -> Result<wgpu::Texture, String> {
        let extent = wgpu::Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        };

        // SAFETY: the device is only used to open a resource, and the raw
        // texture is handed straight to wgpu with a matching description.
        let hal_texture = unsafe {
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

            dx12::Device::texture_from_raw(
                resource,
                VIEWPORT_FORMAT,
                wgpu::TextureDimension::D2,
                extent,
                1,
                1,
            )
        };

        // SAFETY: the description matches the resource created by the editor.
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
    use bevy::math::UVec2;

    pub(super) fn open(_: &wgpu::Device, _: &str, _: UVec2) -> Result<wgpu::Texture, String> {
        Err("the editor viewport is only supported on Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_viewport_sizes() {
        assert_eq!(parse_size("1280x720"), Some(UVec2::new(1280, 720)));
        assert_eq!(parse_size(" 640 x 360 "), Some(UVec2::new(640, 360)));
        assert_eq!(parse_size("0x720"), None);
        assert_eq!(parse_size("1280"), None);
        assert_eq!(parse_size("wide x tall"), None);
    }
}
