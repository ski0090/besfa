//! Pictures of models for the editor's Assets tab. The game renders each
//! model on its own, off screen, the way the model preview shows it, and
//! writes a PNG where the editor asked; one model at a time.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use besfa_protocol::{LightPreset, Report, View};
use bevy::{
    asset::LoadState,
    camera::{Exposure, RenderTarget, primitives::Aabb, visibility::RenderLayers},
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_resource::{PipelineCache, TextureFormat},
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    world_serialization::{WorldAssetRoot, WorldInstance, WorldInstanceSpawner},
};

use crate::{
    edit::subtree,
    inspect::send_report,
    preview::{Isolated, bounds, framing, lit},
    scene_view::EditorOnly,
};

/// The thumbnails' render layer, apart from the preview's and a game's.
const LAYER: RenderLayers = RenderLayers::layer(30);

/// Pixels a side.
const SIZE: u32 = 256;

/// Frames a model renders before it is captured, and frames in a row the
/// renderer must have compiled nothing: a mesh shows only once its
/// pipeline has compiled. A capture with nothing on it is taken again, up
/// to `TRIES` times.
pub(crate) const RENDER_FRAMES: u32 = 10;
pub(crate) const IDLE_FRAMES: u32 = 3;
const TRIES: u32 = 15;

/// Frames a model may take to load and get bounds before it is captured
/// as it is.
pub(crate) const LOAD_FRAMES: u32 = 600;

pub(crate) struct ThumbnailPlugin;

impl Plugin for ThumbnailPlugin {
    fn build(&self, app: &mut App) {
        let compiling = Compiling::default();
        app.init_resource::<Thumbnails>()
            .insert_resource(compiling.clone())
            .add_systems(Startup, spawn_camera_and_light)
            .add_systems(Update, work);
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .insert_resource(compiling)
                .add_systems(Render, count_compiling.in_set(RenderSystems::Cleanup));
        }
    }
}

/// The pipelines the renderer was compiling at the end of its last frame.
#[derive(Resource, Clone, Default)]
pub(crate) struct Compiling(pub(crate) Arc<AtomicUsize>);

fn count_compiling(cache: Res<PipelineCache>, compiling: Res<Compiling>) {
    let waiting = cache.waiting_pipelines().count();
    compiling.0.store(waiting, Ordering::Relaxed);
}

#[derive(Resource, Default)]
struct Thumbnails {
    /// Model paths, relative to the asset directory, with the file each
    /// picture goes to.
    queue: VecDeque<(String, PathBuf)>,
    job: Option<Job>,
}

struct Job {
    path: String,
    file: PathBuf,
    root: Entity,
    frames: u32,
    /// Frames in a row the renderer compiled nothing.
    idle: u32,
    stage: Stage,
}

#[derive(PartialEq)]
enum Stage {
    Loading,
    Rendering,
    Capturing,
}

#[derive(Component)]
struct ThumbnailCamera;

#[derive(Component)]
struct ThumbnailLight;

fn spawn_camera_and_light(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = Image::new_target_texture(SIZE, SIZE, TextureFormat::Rgba8UnormSrgb, None);
    commands.spawn((
        Name::new("Thumbnail camera"),
        EditorOnly,
        ThumbnailCamera,
        Camera3d::default(),
        Camera {
            is_active: false,
            // Before the views that show; nothing else draws into it.
            order: -100,
            // See-through around the model.
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        RenderTarget::Image(images.add(image).into()),
        Projection::Perspective(PerspectiveProjection {
            near: 0.01,
            ..default()
        }),
        LAYER,
    ));
    commands.spawn((
        Name::new("Thumbnail light"),
        EditorOnly,
        ThumbnailLight,
        DirectionalLight::default(),
        LAYER,
    ));
}

/// Queues a picture of the model at `path` for `file`.
pub(crate) fn request(world: &mut World, path: String, file: PathBuf) {
    let mut thumbnails = world.resource_mut::<Thumbnails>();
    if !thumbnails.queue.iter().any(|(queued, _)| *queued == path) {
        thumbnails.queue.push_back((path, file));
    }
}

/// Takes the next model, waits for it to load, frames and renders it, and
/// asks for its picture once it has rendered for a while.
fn work(world: &mut World) {
    let compiling = world.resource::<Compiling>().0.load(Ordering::Relaxed);
    let mut thumbnails = world.resource_mut::<Thumbnails>();
    if thumbnails.job.is_none() {
        let Some((path, file)) = thumbnails.queue.pop_front() else {
            return;
        };
        let scene = world
            .resource::<AssetServer>()
            .load(format!("{path}#Scene0"));
        let root = world
            .spawn((
                Name::new("Thumbnail"),
                EditorOnly,
                Isolated,
                LAYER,
                Transform::default(),
                WorldAssetRoot(scene),
            ))
            .id();
        world.resource_mut::<Thumbnails>().job = Some(Job {
            path,
            file,
            root,
            frames: 0,
            idle: 0,
            stage: Stage::Loading,
        });
        return;
    }
    let job = thumbnails.job.as_mut().unwrap();
    job.frames += 1;
    job.idle = if compiling == 0 { job.idle + 1 } else { 0 };
    let (root, frames) = (job.root, job.frames);
    match job.stage {
        Stage::Loading => {
            if let Some(error) = load_error(world, root) {
                return finish(world, Some(error));
            }
            if !loaded(world, root) && frames < LOAD_FRAMES {
                return;
            }
            let entities = subtree(world, root);
            let (min, max) =
                bounds(world, &entities).unwrap_or((Vec3::splat(-0.5), Vec3::splat(0.5)));
            let view = framing(min, max, View::ThreeQuarter).transform();
            let mut camera = world
                .query_filtered::<(&mut Camera, &mut Transform, Option<&Exposure>), With<ThumbnailCamera>>();
            let Ok((mut camera, mut transform, exposure)) = camera.single_mut(world) else {
                return finish(world, Some("no thumbnail camera".into()));
            };
            camera.is_active = true;
            *transform = view;
            let (light, from, ambient) = lit(LightPreset::Studio, exposure);
            let camera = world
                .query_filtered::<Entity, With<ThumbnailCamera>>()
                .single(world)
                .unwrap();
            world.entity_mut(camera).insert(ambient);
            let mut lights = world
                .query_filtered::<(&mut DirectionalLight, &mut Transform), With<ThumbnailLight>>();
            if let Ok((mut now, mut at)) = lights.single_mut(world) {
                (*now, *at) = (light, from);
            }
            let job = world
                .resource_mut::<Thumbnails>()
                .into_inner()
                .job
                .as_mut()
                .unwrap();
            job.stage = Stage::Rendering;
            job.frames = 0;
        }
        // Shown once every pipeline has compiled, or as it is after a while.
        Stage::Rendering
            if frames >= RENDER_FRAMES && (job.idle >= IDLE_FRAMES || frames >= LOAD_FRAMES) =>
        {
            job.stage = Stage::Capturing;
            let target = world
                .query_filtered::<&RenderTarget, With<ThumbnailCamera>>()
                .single(world)
                .unwrap()
                .clone();
            world.spawn(Screenshot(target)).observe(captured);
        }
        Stage::Rendering | Stage::Capturing => {}
    }
}

/// Why the model did not load, if it did not.
fn load_error(world: &World, root: Entity) -> Option<String> {
    let scene = world.get::<WorldAssetRoot>(root)?.0.id();
    match world.resource::<AssetServer>().load_state(scene) {
        LoadState::Failed(error) => Some(error.to_string().trim_end().to_owned()),
        _ => None,
    }
}

/// The model spawned and its meshes have bounds.
fn loaded(world: &World, root: Entity) -> bool {
    let spawned = world.get::<WorldInstance>(root).is_some_and(|instance| {
        world
            .resource::<WorldInstanceSpawner>()
            .instance_is_ready(**instance)
    });
    spawned
        && subtree(world, root).iter().all(|&entity| {
            world.get::<Mesh3d>(entity).is_none() || world.get::<Aabb>(entity).is_some()
        })
}

/// The picture: written if something shows on it, else rendered for longer.
fn captured(capture: On<ScreenshotCaptured>, mut commands: Commands) {
    let image = capture.image.clone();
    commands.queue(move |world: &mut World| {
        let mut thumbnails = world.resource_mut::<Thumbnails>();
        let Some(job) = thumbnails
            .job
            .as_mut()
            .filter(|job| job.stage == Stage::Capturing)
        else {
            return;
        };
        // See-through all over: the model has not drawn yet.
        let empty = image
            .data
            .as_ref()
            .is_none_or(|data| data.chunks(4).all(|pixel| pixel[3] == 0));
        if empty && job.frames < RENDER_FRAMES * TRIES {
            job.stage = Stage::Rendering;
            return;
        }
        let error = save(&image, &job.file, false).err();
        finish(world, error);
    });
}

/// Writes `image` to the PNG `file`, without its alpha when `opaque`.
pub(crate) fn save(image: &Image, file: &Path, opaque: bool) -> Result<(), String> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let image = image
        .clone()
        .try_into_dynamic()
        .map_err(|error| error.to_string())?;
    let saved = if opaque {
        image.to_rgb8().save(file)
    } else {
        image.to_rgba8().save(file)
    };
    saved.map_err(|error| error.to_string())
}

/// Reports the picture, drops the model and stops rendering.
fn finish(world: &mut World, error: Option<String>) {
    let Some(job) = world.resource_mut::<Thumbnails>().job.take() else {
        return;
    };
    world.entity_mut(job.root).despawn();
    let mut cameras = world.query_filtered::<&mut Camera, With<ThumbnailCamera>>();
    for mut camera in cameras.iter_mut(world) {
        camera.is_active = false;
    }
    if let Some(error) = &error {
        warn!("Could not picture {}: {error}", job.path);
    }
    send_report(&Report::Thumbnail {
        path: job.path,
        error,
    });
}
