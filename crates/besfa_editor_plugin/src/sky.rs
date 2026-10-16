//! The sky: a gradient from the ground up to the zenith, or a cubemap
//! image, which the scene file keeps as one [`Sky`] component. The plugin
//! draws it with Bevy's `Skybox` on every camera that shows the scene, and
//! follows the component whenever it changes, so a game's code can move
//! the sky by writing to it, in edit mode and in play alike.
//!
//! A gradient, with the sun and moon over it, is painted into a cubemap on
//! the CPU, so both kinds share the one `Skybox` path and need no shader of
//! their own.

use std::{
    f32::consts::{PI, TAU},
    sync::OnceLock,
};

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

/// Pixels along a side of each face of a gradient's cubemap: about three
/// a degree, so a sun a few degrees across is a round disc.
const FACE: u32 = 256;

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
    /// Drawn on the gradient only, as is the moon.
    pub sun: Sun,
    pub moon: Moon,
}

/// The sun's disc and the glow around it.
#[derive(Reflect, Clone, Debug, PartialEq)]
#[reflect(Default)]
pub struct Sun {
    pub show: bool,
    /// Toward the sun in the world, of any length: a `DirectionalLight`'s
    /// `Transform::back()`, since it shines forward.
    pub direction: Vec3,
    /// Degrees across.
    pub size: f32,
    pub color: Color,
    /// How strong the lens flare is, 0 for none to 1: rings of the sun's
    /// color across the picture while the sun is in sight.
    pub flare: f32,
}

impl Default for Sun {
    fn default() -> Self {
        Self {
            show: true,
            direction: Vec3::new(0.4, 0.6, -0.5),
            size: 3.0,
            color: Color::srgb(1.0, 0.96, 0.86),
            flare: 0.0,
        }
    }
}

/// The moon's disc, lit as far as its phase.
#[derive(Reflect, Clone, Debug, PartialEq)]
#[reflect(Default)]
pub struct Moon {
    pub show: bool,
    /// Toward the moon in the world, of any length.
    pub direction: Vec3,
    /// Degrees across.
    pub size: f32,
    /// The lit side's color; the dark side shows the sky.
    pub color: Color,
    /// Around once from 0 to 1: 0 new, 0.25 first quarter (the right half
    /// lit), 0.5 full, 0.75 last quarter (the left half).
    pub phase: f32,
}

impl Default for Moon {
    fn default() -> Self {
        Self {
            show: false,
            direction: Vec3::new(-0.4, 0.5, 0.5),
            size: 4.0,
            color: Color::srgb(0.88, 0.90, 0.95),
            phase: 0.5,
        }
    }
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
            sun: Sun::default(),
            moon: Moon::default(),
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

    /// What the gradient shows along the unit `direction` in the world:
    /// its color with the sun and moon over it, which the ground cuts off.
    pub fn shade(&self, direction: Vec3) -> LinearRgba {
        let mut color = self.color(direction);
        if direction.y < 0.0 {
            return color;
        }
        if self.sun.show {
            let sun = self.sun.direction.normalize_or(Vec3::Y);
            let radius = (self.sun.size.max(0.01) / 2.0).to_radians();
            let cosine = direction.dot(sun);
            // The glow reaches four times the radius out.
            if cosine > (radius * 4.0).min(PI).cos() {
                let angle = cosine.clamp(-1.0, 1.0).acos();
                let tint = self.sun.color.to_linear();
                let glow = (1.0 - angle / (radius * 4.0)).max(0.0).powi(2) * 0.35;
                color = color + tint * glow;
                color = color.mix(&tint, edge(radius - angle));
            }
        }
        if self.moon.show {
            color = self.moon_over(color, direction);
        }
        color
    }

    /// `color` with the moon's lit side over it, where `direction` meets it.
    fn moon_over(&self, color: LinearRgba, direction: Vec3) -> LinearRgba {
        let moon = self.moon.direction.normalize_or(Vec3::Y);
        let radius = (self.moon.size.max(0.01) / 2.0).to_radians();
        if direction.dot(moon) < (radius + EDGE).min(PI).cos() {
            return color;
        }
        // The disc as seen: up toward the zenith, right as the eye has it.
        let up = (Vec3::Y - moon * moon.y).normalize_or(Vec3::NEG_Z);
        let right = moon.cross(up);
        let across = radius.sin();
        let (x, y) = (direction.dot(right) / across, direction.dot(up) / across);
        let off_center = (x * x + y * y).sqrt();
        let inside = edge((1.0 - off_center) * radius);
        // The light goes round once a phase: from behind it, to its right,
        // to the eye's side, to its left. `z` faces the eye.
        let z = (1.0 - off_center.min(1.0).powi(2)).sqrt();
        let turn = self.moon.phase * TAU;
        let lit = x.min(1.0) * turn.sin() - z * turn.cos();
        let lit = ((lit + 0.08) / 0.16).clamp(0.0, 1.0);
        color.mix(&self.moon.color.to_linear(), inside * lit)
    }
}

/// About a pixel of the cubemap, in radians, over which a disc's edge fades.
const EDGE: f32 = std::f32::consts::FRAC_PI_2 / FACE as f32;

/// How much of a pixel `inside` radians within an edge is covered.
fn edge(inside: f32) -> f32 {
    (inside / EDGE + 0.5).clamp(0.0, 1.0)
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

/// The gradient with the sun and moon as a cubemap, sRGB, faces in Bevy's
/// order. The skybox turns the cubemap by the sky's rotation and samples it
/// with Z flipped, so each texel is painted with what shows in the world
/// where it ends up.
fn gradient_image(sky: &Sky) -> Image {
    let format = TextureFormat::Rgba8UnormSrgb;
    let turn = Quat::from_rotation_y(sky.rotation.to_radians());
    let mut data = Vec::with_capacity(texels().len() * format.pixel_size().unwrap());
    for texel in texels() {
        let world = turn * Vec3::new(texel.x, texel.y, -texel.z);
        let color = Srgba::from(sky.shade(world));
        data.extend_from_slice(&color.to_u8_array());
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

/// The unit direction through each texel of a cubemap, face by face, row
/// by row.
fn texels() -> &'static [Vec3] {
    static TEXELS: OnceLock<Vec<Vec3>> = OnceLock::new();
    TEXELS.get_or_init(|| {
        let mut texels = Vec::with_capacity((FACE * FACE * 6) as usize);
        for face in 0..6 {
            for y in 0..FACE {
                for x in 0..FACE {
                    let u = (x as f32 + 0.5) / FACE as f32 * 2.0 - 1.0;
                    let v = (y as f32 + 0.5) / FACE as f32 * 2.0 - 1.0;
                    texels.push(face_direction(face, u, v).normalize());
                }
            }
        }
        texels
    })
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
    fn shows_the_sun_where_it_is_unless_hidden_or_below_the_ground() {
        let mut sky = Sky::default();
        let sun = sky.sun.direction.normalize();
        assert!(close(sky.shade(sun), sky.sun.color));
        let beside = Quat::from_rotation_y(0.5) * sun;
        assert!(
            close(sky.shade(beside), Color::from(sky.color(beside))),
            "far off"
        );

        sky.sun.show = false;
        assert!(close(sky.shade(sun), Color::from(sky.color(sun))));
        sky.sun.show = true;
        sky.sun.direction = Vec3::new(0.0, -0.5, 1.0);
        let down = sky.sun.direction.normalize();
        assert!(close(sky.shade(down), sky.ground), "set below the horizon");
    }

    #[test]
    fn lights_the_moon_as_far_as_its_phase() {
        let mut sky = Sky::default();
        sky.sun.show = false;
        sky.moon.show = true;
        let moon = sky.moon.direction.normalize();
        let up = (Vec3::Y - moon * moon.y).normalize();
        let right = moon.cross(up);
        let half = (sky.moon.size / 4.0).to_radians().sin();
        let (east, west) = (
            (moon + right * half).normalize(),
            (moon - right * half).normalize(),
        );
        let lit = |sky: &Sky, at: Vec3| close(sky.shade(at), sky.moon.color);
        let dark = |sky: &Sky, at: Vec3| close(sky.shade(at), Color::from(sky.color(at)));

        sky.moon.phase = 0.5;
        assert!(
            lit(&sky, moon) && lit(&sky, east) && lit(&sky, west),
            "full"
        );
        sky.moon.phase = 0.0;
        assert!(
            dark(&sky, moon) && dark(&sky, east) && dark(&sky, west),
            "new"
        );
        sky.moon.phase = 0.25;
        assert!(lit(&sky, east) && dark(&sky, west), "first quarter");
        sky.moon.phase = 0.75;
        assert!(dark(&sky, east) && lit(&sky, west), "last quarter");
    }

    #[test]
    fn keeps_the_sun_where_it_is_in_the_world_however_the_sky_turns() {
        let sky = Sky {
            rotation: 90.0,
            ..default()
        };
        let sun = sky.sun.direction.normalize();
        // The skybox turns the texel by the rotation and flips its Z.
        let seen = Quat::from_rotation_y(-90f32.to_radians()) * sun;
        let texel = Vec3::new(seen.x, seen.y, -seen.z);
        let nearest = (0..texels().len())
            .max_by(|&a, &b| texels()[a].dot(texel).total_cmp(&texels()[b].dot(texel)))
            .unwrap();
        let image = gradient_image(&sky);
        let pixel = &image.data.as_ref().unwrap()[nearest * 4..nearest * 4 + 3];
        let want = Srgba::from(sky.sun.color).to_u8_array();
        assert!(
            pixel.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 3),
            "{pixel:?}"
        );
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
