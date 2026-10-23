//! Pictures of the scene for an agent, which has no other way to see it. A
//! camera of its own renders the scene off screen the way the scene view
//! shows it, without the grid and handles and, unless asked, the markers,
//! and the picture goes to a PNG file; one picture at a time, once the
//! scene's models have loaded.

use std::{collections::VecDeque, path::PathBuf, sync::atomic::Ordering};

use besfa_protocol::Report;
use bevy::{
    asset::{LoadState, UntypedAssetId},
    camera::{RenderTarget, ScalingMode, visibility::RenderLayers},
    prelude::*,
    render::{
        render_resource::TextureFormat,
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    world_serialization::{DynamicWorldRoot, WorldAssetRoot, WorldInstance, WorldInstanceSpawner},
};

use crate::{
    inspect::send_report,
    scene_view::{EditorOnly, MARKER_LAYER},
    thumbnail::{Compiling, IDLE_FRAMES, LOAD_FRAMES, RENDER_FRAMES, save},
};

/// The largest side of a picture, in pixels.
const MAX_SIZE: u32 = 4096;

/// How high above the target the view from above stands; it sees as far
/// below the target.
const TOP_HEIGHT: f32 = 1000.0;

pub(crate) struct ShotPlugin;

impl Plugin for ShotPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shots>().add_systems(Update, work);
    }
}

/// Where a picture is taken from and how big it is.
#[derive(Debug)]
pub(crate) struct Shot {
    transform: Transform,
    projection: Projection,
    size: UVec2,
    /// Whether it draws the markers too.
    markers: bool,
}

impl Shot {
    /// Looking from `eye` at `target`, or with `top`, straight down on
    /// `target` over a square `top` meters wide, north (-Z) up; with
    /// `markers`, the markers drawn as the scene view draws them.
    pub(crate) fn new(
        target: Vec3,
        eye: Option<Vec3>,
        top: Option<f32>,
        width: Option<u32>,
        height: Option<u32>,
        markers: bool,
    ) -> Result<Self, String> {
        let (transform, projection) = match (top, eye) {
            (Some(top), _) if top > 0.0 && top.is_finite() => (
                Transform::from_translation(target + Vec3::Y * TOP_HEIGHT)
                    .looking_at(target, Vec3::NEG_Z),
                Projection::Orthographic(OrthographicProjection {
                    scaling_mode: ScalingMode::FixedHorizontal {
                        viewport_width: top,
                    },
                    far: 2.0 * TOP_HEIGHT,
                    ..OrthographicProjection::default_3d()
                }),
            ),
            (Some(top), _) => return Err(format!("top is a width in meters above 0, not {top}")),
            (None, Some(eye)) if eye.distance(target) > 1e-3 => {
                // Looking straight up or down, Y cannot be the picture's up.
                let up = if (target - eye).normalize().y.abs() > 0.999 {
                    Vec3::NEG_Z
                } else {
                    Vec3::Y
                };
                (
                    Transform::from_translation(eye).looking_at(target, up),
                    Projection::Perspective(PerspectiveProjection {
                        far: 5000.0,
                        ..default()
                    }),
                )
            }
            (None, Some(_)) => return Err("eye and target are the same point".into()),
            (None, None) => return Err("give an eye, or top for a view from above".into()),
        };
        let width = width.unwrap_or(1024);
        let height = height.unwrap_or(if top.is_some() { width } else { 768 });
        if !(1..=MAX_SIZE).contains(&width) || !(1..=MAX_SIZE).contains(&height) {
            return Err(format!(
                "a picture is 1 to {MAX_SIZE} pixels a side, not {width} by {height}"
            ));
        }
        Ok(Self {
            transform,
            projection,
            size: UVec2::new(width, height),
            markers,
        })
    }
}

#[derive(Resource, Default)]
struct Shots {
    /// The pictures asked for, with the path the report names and the file
    /// each goes to.
    queue: VecDeque<(String, PathBuf, Shot)>,
    job: Option<Job>,
}

struct Job {
    path: String,
    file: PathBuf,
    camera: Entity,
    frames: u32,
    /// Frames in a row nothing loaded or compiled.
    idle: u32,
    capturing: bool,
}

/// Queues `shot` for `file`; the report names it `path`.
pub(crate) fn request(world: &mut World, path: String, file: PathBuf, shot: Shot) {
    world
        .resource_mut::<Shots>()
        .queue
        .push_back((path, file, shot));
}

/// Takes the next picture: renders the scene from its camera until every
/// model has loaded and every pipeline compiled, then captures it.
fn work(world: &mut World) {
    let busy = world.resource::<Compiling>().0.load(Ordering::Relaxed) > 0 || loading(world);
    let mut shots = world.resource_mut::<Shots>();
    if shots.job.is_none() {
        let Some((path, file, shot)) = shots.queue.pop_front() else {
            return;
        };
        let image = Image::new_target_texture(
            shot.size.x,
            shot.size.y,
            TextureFormat::Rgba8UnormSrgb,
            None,
        );
        let image = world.resource_mut::<Assets<Image>>().add(image);
        let camera = world
            .spawn((
                Name::new("Screenshot camera"),
                EditorOnly,
                Camera3d::default(),
                Camera {
                    order: -90,
                    ..default()
                },
                RenderTarget::Image(image.into()),
                shot.projection,
                shot.transform,
            ))
            .id();
        if shot.markers {
            world
                .entity_mut(camera)
                .insert(RenderLayers::default().with(MARKER_LAYER));
        }
        world.resource_mut::<Shots>().job = Some(Job {
            path,
            file,
            camera,
            frames: 0,
            idle: 0,
            capturing: false,
        });
        return;
    }
    let job = shots.job.as_mut().unwrap();
    job.frames += 1;
    job.idle = if busy { 0 } else { job.idle + 1 };
    let ready = job.frames >= RENDER_FRAMES && job.idle >= IDLE_FRAMES;
    if job.capturing || !(ready || job.frames >= LOAD_FRAMES) {
        return;
    }
    job.capturing = true;
    let camera = job.camera;
    let target = world.get::<RenderTarget>(camera).unwrap().clone();
    world.spawn(Screenshot(target)).observe(captured);
}

/// Placed models and prefabs still loading or spawning, which a picture
/// would miss. One that failed to load is not waited for.
fn loading(world: &mut World) -> bool {
    let mut roots = world.query_filtered::<(
        Option<&WorldAssetRoot>,
        Option<&DynamicWorldRoot>,
        Option<&WorldInstance>,
    ), Or<(With<WorldAssetRoot>, With<DynamicWorldRoot>)>>();
    let server = world.resource::<AssetServer>();
    let spawner = world.resource::<WorldInstanceSpawner>();
    roots.iter(world).any(|(model, prefab, instance)| {
        let id: UntypedAssetId = match (model, prefab) {
            (Some(model), _) => model.0.id().untyped(),
            (None, Some(prefab)) => prefab.0.id().untyped(),
            (None, None) => return false,
        };
        !matches!(server.load_state(id), LoadState::Failed(_))
            && (!server.is_loaded_with_dependencies(id)
                || instance.is_none_or(|instance| !spawner.instance_is_ready(**instance)))
    })
}

/// Writes the picture, drops its camera, whose image goes with it, and
/// reports it.
fn captured(capture: On<ScreenshotCaptured>, mut commands: Commands) {
    let image = capture.image.clone();
    commands.queue(move |world: &mut World| {
        let Some(job) = world.resource_mut::<Shots>().job.take() else {
            return;
        };
        world.despawn(job.camera);
        let error = save(&image, &job.file, false).err();
        match &error {
            Some(error) => warn!("Could not save {}: {error}", job.path),
            None => info!("Saved {}", job.path),
        }
        send_report(&Report::Screenshot {
            path: job.path,
            error,
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_down_with_north_up_from_above() {
        let shot = Shot::new(Vec3::new(10.0, 0.0, -5.0), None, Some(200.0), None, None, false).unwrap();
        assert_eq!(shot.size, UVec2::splat(1024));
        assert!(shot.transform.forward().abs_diff_eq(Vec3::NEG_Y, 1e-5));
        assert!(shot.transform.up().abs_diff_eq(Vec3::NEG_Z, 1e-5));
        assert!(shot.transform.right().abs_diff_eq(Vec3::X, 1e-5));
        assert!(
            shot.transform
                .translation
                .xz()
                .abs_diff_eq(Vec2::new(10.0, -5.0), 1e-5)
        );
        let Projection::Orthographic(projection) = shot.projection else {
            panic!("a view from above is orthographic");
        };
        assert!(matches!(
            projection.scaling_mode,
            ScalingMode::FixedHorizontal {
                viewport_width: 200.0
            }
        ));
    }

    #[test]
    fn looks_from_the_eye_at_the_target() {
        let eye = Vec3::new(0.0, 30.0, 40.0);
        let shot = Shot::new(Vec3::ZERO, Some(eye), None, None, None, false).unwrap();
        assert_eq!(shot.size, UVec2::new(1024, 768));
        assert!(
            shot.transform
                .forward()
                .abs_diff_eq((-eye).normalize(), 1e-5)
        );
        // Straight down still makes a picture.
        let down = Shot::new(Vec3::ZERO, Some(Vec3::Y * 50.0), None, Some(64), Some(32), false).unwrap();
        assert!(down.transform.up().abs_diff_eq(Vec3::NEG_Z, 1e-5));
        assert_eq!(down.size, UVec2::new(64, 32));

        assert!(Shot::new(Vec3::ZERO, None, None, None, None, false).is_err());
        assert!(Shot::new(Vec3::ZERO, Some(Vec3::ZERO), None, None, None, false).is_err());
        assert!(Shot::new(Vec3::ZERO, None, Some(0.0), None, None, false).is_err());
        assert!(Shot::new(Vec3::ZERO, Some(eye), None, Some(5000), None, false).is_err());
    }
}
