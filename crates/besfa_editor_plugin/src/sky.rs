//! The sky: a gradient from the ground up to the zenith, or a cubemap
//! image, which the scene file keeps as one [`Sky`] component. The plugin
//! draws it with Bevy's `Skybox` on every camera that shows the scene, and
//! follows the component whenever it changes, so a game's code can move
//! the sky by writing to it, in edit mode and in play alike.
//!
//! A gradient is painted into a small cubemap on the CPU, so both kinds
//! share the one `Skybox` path and need no shader of their own.

use bevy::{
    asset::{LoadState, RenderAssetUsages},
    camera::{Exposure, visibility::RenderLayers},
    image::TextureFormatPixelInfo,
    light::Skybox,
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    },
};

/// Pixels along a side of each face of a gradient's cubemap.
const FACE: u32 = 64;

pub(crate) struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SkyImages>()
            .add_systems(PostUpdate, (paint, show).chain());
    }
}

/// The scene's sky, which every camera showing the scene draws behind
/// everything else. With more than one in the scene, the first is drawn.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct Sky {
    pub mode: SkyMode,
    /// Straight up.
    pub zenith: Color,
    /// Level with the eye.
    pub horizon: Color,
    /// Below the horizon.
    pub ground: Color,
    /// How far the horizon's color reaches up: the height above the horizon
    /// (0 to 1) is raised to this power to mix toward the zenith, so less
    /// gives more of the zenith's color.
    pub blend: f32,
    /// A cubemap image relative to the asset folder: one Bevy reads as a
    /// cubemap (`.ktx2`, `.dds`), or the six faces +X, -X, +Y, -Y, +Z, -Z
    /// stacked top to bottom in one image six times as tall as it is wide.
    pub cubemap: String,
    /// 1 shows the colors as picked, whatever the camera's exposure.
    pub brightness: f32,
    /// Degrees the sky turns counterclockwise, seen from above.
    pub rotation: f32,
}

#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[reflect(Default)]
pub enum SkyMode {
    #[default]
    Gradient,
    /// The [`Sky::cubemap`] image; the gradient shows while it loads, or if
    /// it cannot.
    Cubemap,
}

impl Default for Sky {
    fn default() -> Self {
        Self {
            mode: SkyMode::Gradient,
            zenith: Color::srgb(0.20, 0.42, 0.78),
            horizon: Color::srgb(0.72, 0.84, 0.95),
            ground: Color::srgb(0.32, 0.30, 0.28),
            blend: 0.5,
            cubemap: String::new(),
            brightness: 1.0,
            rotation: 0.0,
        }
    }
}

impl Sky {
    /// The gradient's color looking along `direction`, which need not be
    /// normalized. Colors mix in linear space.
    pub fn color(&self, direction: Vec3) -> LinearRgba {
        let height = direction.normalize_or(Vec3::Y).y;
        let horizon = self.horizon.to_linear();
        let (other, amount) = if height >= 0.0 {
            (self.zenith, height.powf(self.blend.max(0.01)))
        } else {
            (self.ground, (-4.0 * height).min(1.0))
        };
        horizon.mix(&other.to_linear(), amount)
    }
}

/// Put on a camera with the `Skybox` this plugin gave it, which goes with
/// the sky. A camera the game gave a skybox itself is left alone.
#[derive(Component)]
pub(crate) struct SkyApplied;

/// The images the sky shows.
#[derive(Resource, Default)]
struct SkyImages {
    /// The gradient painted as a cubemap.
    gradient: Option<Handle<Image>>,
    /// The cubemap file asked for, and whether it is ready to show.
    cubemap: Option<(String, Handle<Image>)>,
    ready: bool,
    /// Said once per file.
    failed: bool,
}

/// Paints the gradient again when the sky changes and loads the cubemap
/// it names.
fn paint(
    skies: Query<Ref<Sky>>,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<SkyImages>,
) {
    let Some(sky) = skies.iter().next() else {
        return;
    };
    match &state.gradient {
        None => state.gradient = Some(images.add(gradient_image(&sky))),
        Some(handle) if sky.is_changed() => {
            if let Some(mut image) = images.get_mut(handle) {
                image.data = gradient_image(&sky).data;
            }
        }
        Some(_) => {}
    }

    if sky.mode != SkyMode::Cubemap || sky.cubemap.is_empty() {
        return;
    }
    if state
        .cubemap
        .as_ref()
        .is_none_or(|(path, _)| *path != sky.cubemap)
    {
        let handle = assets.load(sky.cubemap.clone());
        state.cubemap = Some((sky.cubemap.clone(), handle));
        state.ready = false;
        state.failed = false;
    }
    if state.ready || state.failed {
        return;
    }
    let (path, handle) = state.cubemap.clone().unwrap();
    match assets.load_state(&handle) {
        LoadState::Loaded => {
            let Some(mut image) = images.get_mut(&handle) else {
                return;
            };
            match as_cubemap(&mut image) {
                Ok(()) => state.ready = true,
                Err(error) => {
                    error!("Cannot show {path} as the sky: {error}");
                    state.failed = true;
                }
            }
        }
        LoadState::Failed(error) => {
            error!("Cannot load the sky {path}: {error}");
            state.failed = true;
        }
        _ => {}
    }
}

/// Gives every camera that shows the scene the sky, and takes it away
/// again once the scene has none.
fn show(
    mut commands: Commands,
    skies: Query<&Sky>,
    state: Res<SkyImages>,
    mut cameras: Query<
        (
            Entity,
            Option<&RenderLayers>,
            Option<&Exposure>,
            Option<&mut Skybox>,
            Has<SkyApplied>,
        ),
        With<Camera3d>,
    >,
) {
    let sky = skies.iter().next();
    let image = match (sky, &state.cubemap) {
        (Some(sky), Some((path, handle)))
            if sky.mode == SkyMode::Cubemap && state.ready && *path == sky.cubemap =>
        {
            Some(handle.clone())
        }
        _ => state.gradient.clone(),
    };
    for (camera, layers, exposure, skybox, applied) in &mut cameras {
        let (Some(sky), Some(image)) = (sky, image.clone()) else {
            if applied {
                commands.entity(camera).remove::<(Skybox, SkyApplied)>();
            }
            continue;
        };
        // The model preview and thumbnails draw layers of their own.
        let shows_scene = layers.is_none_or(|layers| layers.intersects(&RenderLayers::default()));
        if !shows_scene || (skybox.is_some() && !applied) {
            continue;
        }
        let exposure = exposure.copied().unwrap_or_default().exposure();
        let wanted = Skybox {
            image: Some(image),
            brightness: sky.brightness / exposure,
            rotation: Quat::from_rotation_y(sky.rotation.to_radians()),
        };
        match skybox {
            Some(mut skybox) => {
                let same = skybox.image == wanted.image
                    && skybox.brightness == wanted.brightness
                    && skybox.rotation == wanted.rotation;
                if !same {
                    *skybox = wanted;
                }
            }
            None => {
                commands.entity(camera).insert((wanted, SkyApplied));
            }
        }
    }
}

/// The gradient as a cubemap, sRGB, faces in Bevy's order.
fn gradient_image(sky: &Sky) -> Image {
    let format = TextureFormat::Rgba8UnormSrgb;
    let mut data = Vec::with_capacity((FACE * FACE * 6) as usize * format.pixel_size().unwrap());
    for face in 0..6 {
        for y in 0..FACE {
            for x in 0..FACE {
                let u = (x as f32 + 0.5) / FACE as f32 * 2.0 - 1.0;
                let v = (y as f32 + 0.5) / FACE as f32 * 2.0 - 1.0;
                let color = Srgba::from(sky.color(face_direction(face, u, v)));
                data.extend_from_slice(&color.to_u8_array());
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: FACE,
            height: FACE,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        format,
        // Kept in the main world as well, to paint again.
        RenderAssetUsages::default(),
    );
    image.texture_view_descriptor = Some(cube_view());
    image
}

/// The direction through the point `u`, `v` (each -1 to 1, `v` down) of a
/// cubemap's face, in the order +X, -X, +Y, -Y, +Z, -Z.
fn face_direction(face: u32, u: f32, v: f32) -> Vec3 {
    match face {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        _ => Vec3::new(-u, -v, -1.0),
    }
}

fn cube_view() -> TextureViewDescriptor<'static> {
    TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    }
}

/// Makes a loaded image a cubemap: one already is, and six faces stacked
/// top to bottom become one.
fn as_cubemap(image: &mut Image) -> Result<(), String> {
    if image.texture_descriptor.array_layer_count() == 6 {
        image.texture_view_descriptor = Some(cube_view());
        return Ok(());
    }
    if image.texture_descriptor.array_layer_count() != 1 || image.height() != image.width() * 6 {
        return Err(format!(
            "a {}×{} image is not six square faces stacked top to bottom",
            image.width(),
            image.height()
        ));
    }
    image
        .reinterpret_stacked_2d_as_array(6)
        .map_err(|error| error.to_string())?;
    image.texture_view_descriptor = Some(cube_view());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .add_plugins(SkyPlugin);
        app
    }

    fn close(a: LinearRgba, b: Color) -> bool {
        let b = b.to_linear();
        (a.red - b.red).abs() < 1e-4
            && (a.green - b.green).abs() < 1e-4
            && (a.blue - b.blue).abs() < 1e-4
    }

    #[test]
    fn mixes_from_the_horizon_up_to_the_zenith_and_down_to_the_ground() {
        let sky = Sky::default();
        assert!(close(sky.color(Vec3::Y), sky.zenith));
        assert!(close(sky.color(Vec3::X), sky.horizon));
        assert!(close(sky.color(Vec3::NEG_Y), sky.ground));
        assert!(
            close(sky.color(Vec3::new(1.0, -1.0, 0.0)), sky.ground),
            "well below"
        );
        let half = sky.color(Vec3::new(1.0, 1.0, 0.0));
        assert!(!close(half, sky.zenith) && !close(half, sky.horizon));
    }

    #[test]
    fn paints_six_faces_with_the_zenith_on_top_and_the_ground_below() {
        let sky = Sky::default();
        let image = gradient_image(&sky);
        assert_eq!(image.texture_descriptor.array_layer_count(), 6);
        let data = image.data.as_ref().unwrap();
        let face = (FACE * FACE * 4) as usize;
        let middle = |layer: usize| {
            let at = layer * face + ((FACE / 2 * FACE + FACE / 2) * 4) as usize;
            &data[at..at + 4]
        };
        let srgb = |color: Color| Srgba::from(color).to_u8_array();
        let near = |a: &[u8], b: [u8; 4]| a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2);
        assert!(near(middle(2), srgb(sky.zenith)), "+Y");
        assert!(near(middle(3), srgb(sky.ground)), "-Y");
    }

    #[test]
    fn makes_a_stacked_image_a_cubemap() {
        let mut stacked = Image::new_fill(
            Extent3d {
                width: 4,
                height: 24,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        as_cubemap(&mut stacked).unwrap();
        assert_eq!(stacked.texture_descriptor.array_layer_count(), 6);
        let mut wide = Image::default();
        assert!(as_cubemap(&mut wide).is_err());
    }

    #[test]
    fn follows_the_sky_on_cameras_that_show_the_scene() {
        let mut app = app();
        let camera = app.world_mut().spawn(Camera3d::default()).id();
        let preview = app
            .world_mut()
            .spawn((Camera3d::default(), RenderLayers::layer(31)))
            .id();
        app.update();
        assert!(app.world().get::<Skybox>(camera).is_none(), "no sky yet");

        let sky = app.world_mut().spawn(Sky::default()).id();
        app.update();
        let skybox = app.world().get::<Skybox>(camera).unwrap();
        let exposure = Exposure::default().exposure();
        assert!((skybox.brightness * exposure - 1.0).abs() < 1e-4);
        assert!(skybox.image.is_some());
        assert!(app.world().get::<Skybox>(preview).is_none());

        // The game moves the sky, and the camera follows.
        app.world_mut().get_mut::<Sky>(sky).unwrap().rotation = 90.0;
        app.world_mut().get_mut::<Sky>(sky).unwrap().zenith = Color::BLACK;
        app.update();
        let skybox = app.world().get::<Skybox>(camera).unwrap();
        assert!(
            skybox
                .rotation
                .abs_diff_eq(Quat::from_rotation_y(90f32.to_radians()), 1e-5)
        );
        let handle = skybox.image.clone().unwrap();
        let images = app.world().resource::<Assets<Image>>();
        let data = images.get(&handle).unwrap().data.clone().unwrap();
        let top = 2 * (FACE * FACE * 4) as usize + ((FACE / 2 * FACE + FACE / 2) * 4) as usize;
        assert!(
            data[top..top + 3].iter().all(|&byte| byte < 8),
            "painted again"
        );

        app.world_mut().entity_mut(sky).remove::<Sky>();
        app.update();
        assert!(app.world().get::<Skybox>(camera).is_none());
        assert!(app.world().get::<SkyApplied>(camera).is_none());
    }

    #[test]
    fn leaves_a_skybox_the_game_gave_alone() {
        let mut app = app();
        let camera = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                Skybox {
                    brightness: 7.0,
                    ..default()
                },
            ))
            .id();
        app.world_mut().spawn(Sky::default());
        app.update();
        assert_eq!(app.world().get::<Skybox>(camera).unwrap().brightness, 7.0);
    }
}
