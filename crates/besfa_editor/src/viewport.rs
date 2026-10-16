//! The texture a game renders into and the editor shows, and the viewport's
//! pointer and keys, which go to the game as commands.
//!
//! The editor creates a D3D12 texture on its own device, publishes it under
//! a named NT handle and passes the name to the game in `BESFA_VIEWPORT`;
//! the game's `besfa_editor_plugin` opens it by that name. The shared
//! texture is sRGB so the game's shaders encode colors on write, while egui
//! samples gamma-encoded bytes, so each frame is copied into a plain BGRA
//! texture that egui shows.

use std::{collections::HashSet, sync::Arc};

use besfa_protocol::{Button, Command, PointerEvent};
use eframe::{
    egui::{self, Event, EventFilter, Key, PointerButton, Pos2, Rect, Sense, Vec2, mutex::RwLock},
    egui_wgpu::{self, RenderState},
};

const SHARED_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;

pub struct ViewportTexture {
    /// Name of the shared handle the game opens.
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// What egui draws: the copy of the game's last frame.
    pub texture_id: egui::TextureId,
    shared: wgpu::Texture,
    display: wgpu::Texture,
    /// Keeps the named handle alive so the game can open it by name.
    handle: shared::Handle,
    renderer: Arc<RwLock<egui_wgpu::Renderer>>,
}

impl ViewportTexture {
    pub fn create(render: &RenderState, width: u32, height: u32) -> Result<Self, String> {
        let (shared, handle, name) = shared::create(&render.device, width, height)?;
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        // Start from black instead of whatever the memory held.
        let mut encoder = render.device.create_command_encoder(&Default::default());
        let view = shared.create_view(&Default::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        render.queue.submit([encoder.finish()]);

        // Same bytes, read without sRGB decoding.
        let display = render.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("besfa_viewport_display"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SHARED_FORMAT.remove_srgb_suffix(),
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let texture_id = render.renderer.write().register_native_texture(
            &render.device,
            &display.create_view(&Default::default()),
            wgpu::FilterMode::Linear,
        );
        Ok(Self {
            name,
            width,
            height,
            texture_id,
            shared,
            display,
            handle,
            renderer: render.renderer.clone(),
        })
    }

    /// Copies the game's latest frame into the texture egui shows.
    // ponytail: no lock against the game's writes, so a frame can tear. Add
    // a shared fence or keyed mutex if that shows up.
    pub fn copy_frame(&self, render: &RenderState) {
        let mut encoder = render.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(
            self.shared.as_image_copy(),
            self.display.as_image_copy(),
            self.shared.size(),
        );
        render.queue.submit([encoder.finish()]);
    }
}

impl Drop for ViewportTexture {
    fn drop(&mut self) {
        self.renderer.write().free_texture(&self.texture_id);
        self.handle.close();
    }
}

/// The GPU the editor renders on; the game must use the same one to open
/// the shared texture.
pub fn adapter_name(render: &RenderState) -> String {
    render.adapter.get_info().name
}

/// Pixels one wheel notch scrolls, which the game zooms by.
// ponytail: a guess near Windows' 3 lines per notch; tune it if zooming
// feels too slow or fast.
const PIXELS_PER_LINE: f32 = 50.0;

/// The keys that fly the scene view while the right button is held.
const FLY_KEYS: [Key; 6] = [Key::W, Key::A, Key::S, Key::D, Key::Q, Key::E];

/// Forwards the viewport's pointer to the game, in texture pixels, as
/// `pointer` commands: the scene view's camera, click selection and handles
/// run in the game, and once it plays, the pointer is its mouse. While it
/// plays, keys pressed with the viewport focused go to it as `key`
/// commands; Play and clicking the viewport focus it, clicking elsewhere
/// takes the focus away and lets held keys go.
#[derive(Default)]
pub struct ViewportInput {
    /// The button held down; others are ignored until it is released.
    pressed: Option<Button>,
    /// Fly keys held while looking around in edit mode, by physical key.
    held: HashSet<Key>,
    /// The last `fly` sent; another goes only when it changes.
    flying: Option<Command>,
    focused: bool,
    was_playing: bool,
    /// Takes the keyboard on the next frame.
    pub focus_next: bool,
}

impl ViewportInput {
    /// The right button is held in edit mode. Its keys fly the scene view
    /// then, so the editor's shortcuts wait.
    pub fn looking(&self, playing: bool) -> bool {
        self.pressed == Some(Button::Right) && !playing
    }

    /// Handles this frame's input over `rect`, which shows a texture of
    /// `texture_size` pixels.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        texture_size: Vec2,
        playing: bool,
        send: &mut dyn FnMut(Command),
    ) -> egui::Response {
        let response = ui.interact(
            rect,
            ui.id().with("scene_viewport"),
            Sense::click_and_drag(),
        );
        if playing != self.was_playing {
            self.was_playing = playing;
            self.held.clear();
        }
        if std::mem::take(&mut self.focus_next) {
            response.request_focus();
        }
        if response.clicked_elsewhere() {
            response.surrender_focus();
        }

        let at = |pos: Pos2| (pos - rect.min) * texture_size / rect.size();
        let over = response.contains_pointer();
        let focused = response.has_focus();
        let (events, shift) = ui.input(|i| (i.events.clone(), i.modifiers.shift));
        for (index, event) in events.iter().enumerate() {
            match event {
                Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    ..
                } if over && self.pressed.is_none() => {
                    if let Some(button) = button_name(*button) {
                        response.request_focus();
                        self.pressed = Some(button);
                        let Vec2 { x, y } = at(*pos);
                        send(Command::Pointer(PointerEvent::Down { x, y, button }));
                        if self.looking(playing) {
                            self.flying = Some(fly([0.0; 3], false));
                        }
                    }
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    ..
                } if self.pressed.is_some() && button_name(*button) == self.pressed => {
                    // The game stops flying when the button goes up.
                    let Vec2 { x, y } = at(*pos);
                    let button = self.pressed.take().unwrap();
                    send(Command::Pointer(PointerEvent::Up { x, y, button }));
                    self.held.clear();
                }
                Event::PointerMoved(pos) if over || self.pressed.is_some() => {
                    let Vec2 { x, y } = at(*pos);
                    send(Command::Pointer(PointerEvent::Move { x, y }));
                }
                // Positive scrolls down, which zooms out.
                Event::MouseWheel { unit, delta, .. } if over => {
                    let pixels = match unit {
                        egui::MouseWheelUnit::Point => delta.y,
                        egui::MouseWheelUnit::Line => delta.y * PIXELS_PER_LINE,
                        egui::MouseWheelUnit::Page => delta.y * rect.height(),
                    };
                    send(Command::Pointer(PointerEvent::Scroll { delta: -pixels }));
                }
                // A held key stays pressed in the game until it is released,
                // so repeats are not sent.
                Event::Key {
                    key,
                    physical_key,
                    pressed,
                    repeat: false,
                    ..
                } => {
                    let key = physical_key.unwrap_or(*key);
                    if playing && focused {
                        if let Some(code) = key_code(key) {
                            let text = match (*pressed, events.get(index + 1)) {
                                (true, Some(Event::Text(text))) => Some(text.clone()),
                                _ => None,
                            };
                            send(Command::Key {
                                code,
                                pressed: *pressed,
                                text,
                            });
                        }
                    } else if self.looking(playing) && FLY_KEYS.contains(&key) {
                        if *pressed {
                            self.held.insert(key);
                        } else {
                            self.held.remove(&key);
                        }
                    }
                }
                _ => {}
            }
        }

        // While the right button is held in edit mode, W, A, S and D fly the
        // scene view's camera, Q and E down and up, and Shift speeds it up.
        // They stay the editor's keys: the game's input never sees them.
        if self.looking(playing) {
            let held = |key| f32::from(u8::from(self.held.contains(&key)));
            let axis = |plus, minus| held(plus) - held(minus);
            let command = fly(
                [
                    axis(Key::D, Key::A),
                    axis(Key::E, Key::Q),
                    axis(Key::W, Key::S),
                ],
                shift,
            );
            if self.flying.as_ref() != Some(&command) {
                send(command.clone());
                self.flying = Some(command);
            }
        }

        if focused {
            // Tab, arrows and Escape are the game's, not focus moves.
            let keys = EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            };
            ui.memory_mut(|memory| memory.set_focus_lock_filter(response.id, keys));
        } else if self.focused {
            if playing {
                send(Command::FocusLost);
            } else {
                self.held.clear();
            }
        }
        self.focused = focused;
        response
    }
}

fn fly([right, up, forward]: [f32; 3], fast: bool) -> Command {
    Command::Fly {
        right,
        up,
        forward,
        fast,
    }
}

fn button_name(button: PointerButton) -> Option<Button> {
    match button {
        PointerButton::Primary => Some(Button::Left),
        PointerButton::Middle => Some(Button::Middle),
        PointerButton::Secondary => Some(Button::Right),
        _ => None,
    }
}

/// Bevy's `KeyCode` name for the physical `key`, which is the key's W3C code
/// name, like `KeyW`; None for keys the game is not told about. Physical, so
/// a game's WASD stays where it is on any keyboard layout.
fn key_code(key: Key) -> Option<String> {
    // egui's variant names are mostly the W3C names already.
    let name = format!("{key:?}");
    Some(match name.as_str() {
        letter if letter.len() == 1 => format!("Key{letter}"),
        digit if digit.starts_with("Num") => format!("Digit{}", &digit[3..]),
        "Equals" => "Equal".into(),
        "OpenBracket" => "BracketLeft".into(),
        "CloseBracket" => "BracketRight".into(),
        "Backtick" => "Backquote".into(),
        // Logical keys with no physical key of their own.
        "Copy" | "Cut" | "Paste" | "Colon" | "Pipe" | "Questionmark" | "Exclamationmark"
        | "OpenCurlyBracket" | "CloseCurlyBracket" | "Plus" | "BrowserBack" => return None,
        _ => name,
    })
}

mod shared {
    use std::sync::atomic::{AtomicU32, Ordering};

    use wgpu::hal::{api::Dx12, dx12};
    use windows::{
        Win32::{
            Foundation::{CloseHandle, GENERIC_ALL, HANDLE},
            Graphics::{
                Direct3D12::*,
                Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_SAMPLE_DESC},
            },
        },
        core::HSTRING,
    };

    use super::SHARED_FORMAT;

    pub(super) struct Handle(HANDLE);

    impl Handle {
        pub(super) fn close(&mut self) {
            // SAFETY: the handle came from CreateSharedHandle and is closed once.
            let _ = unsafe { CloseHandle(std::mem::take(&mut self.0)) };
        }
    }

    fn next_name() -> String {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        format!(
            "Local\\besfa_viewport_{}_{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }

    /// Creates a render target the game can open by the returned name: a
    /// single-sample 2D BGRA8 sRGB texture, which the plugin checks for.
    pub(super) fn create(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Result<(wgpu::Texture, Handle, String), String> {
        let name = next_name();
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        // SAFETY: the device only creates a resource and a handle to it, and
        // the raw texture is handed straight to wgpu with its own description.
        let (hal_texture, handle) = unsafe {
            let hal = device
                .as_hal::<Dx12>()
                .ok_or("the editor is not rendering with DX12")?;
            let raw = hal.raw_device();
            let heap = D3D12_HEAP_PROPERTIES {
                Type: D3D12_HEAP_TYPE_DEFAULT,
                ..Default::default()
            };
            let desc = D3D12_RESOURCE_DESC {
                Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
                Width: width.into(),
                Height: height,
                DepthOrArraySize: 1,
                MipLevels: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                // Simultaneous access: two processes' devices use it.
                Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET
                    | D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
                ..Default::default()
            };
            let mut resource: Option<ID3D12Resource> = None;
            raw.CreateCommittedResource(
                &heap,
                D3D12_HEAP_FLAG_SHARED,
                &desc,
                D3D12_RESOURCE_STATE_COMMON,
                None,
                &mut resource,
            )
            .map_err(|error| format!("CreateCommittedResource failed: {error}"))?;
            let resource = resource.ok_or("CreateCommittedResource returned no resource")?;
            let handle = raw
                .CreateSharedHandle(&resource, None, GENERIC_ALL.0, &HSTRING::from(&name))
                .map_err(|error| format!("CreateSharedHandle failed: {error}"))?;
            let texture = dx12::Device::texture_from_raw(
                resource,
                SHARED_FORMAT,
                wgpu::TextureDimension::D2,
                extent,
                1,
                1,
            );
            (texture, Handle(handle))
        };
        // SAFETY: the description matches the resource created above.
        let texture = unsafe {
            device.create_texture_from_hal::<Dx12>(
                hal_texture,
                &wgpu::TextureDescriptor {
                    label: Some("besfa_viewport_shared"),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: SHARED_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                },
                // A shared resource starts in the common state.
                wgpu::TextureUses::UNINITIALIZED,
            )
        };
        Ok((texture, handle, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_keys_the_way_bevy_does() {
        let codes: Vec<_> = [
            Key::W,
            Key::Num1,
            Key::Num0,
            Key::F12,
            Key::Equals,
            Key::OpenBracket,
        ]
        .into_iter()
        .filter_map(key_code)
        .collect();
        assert_eq!(
            codes,
            ["KeyW", "Digit1", "Digit0", "F12", "Equal", "BracketLeft"]
        );
        assert_eq!(key_code(Key::Backtick).as_deref(), Some("Backquote"));
        assert_eq!(key_code(Key::ShiftLeft).as_deref(), Some("ShiftLeft"));
        assert_eq!(key_code(Key::ArrowUp).as_deref(), Some("ArrowUp"));
        assert_eq!(key_code(Key::Copy), None);
    }
}
