//! The sun's lens flare: soft rings of the sun's color strung along the
//! line from the sun through the middle of the picture, as a camera lens
//! would show them. They are UI circles over each camera that shows the
//! sky, so they follow the camera without touching the scene, and they go
//! when the sun is off the picture, below the horizon or behind something.

use bevy::{
    picking::{
        Pickable,
        mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings},
    },
    prelude::*,
    ui::UiTargetCamera,
};

use crate::{
    EditorOnly,
    sky::{Sky, SkyApplied, SkyMode},
};

/// Each ring: how far along the line from the sun (0) through the middle
/// of the picture (1) it sits, its size against the picture's height, and
/// how strong it is at full flare.
const GHOSTS: [(f32, f32, f32); 7] = [
    (0.0, 0.32, 0.30),
    (0.35, 0.04, 0.35),
    (0.55, 0.09, 0.18),
    (0.8, 0.025, 0.40),
    (1.2, 0.14, 0.10),
    (1.45, 0.05, 0.25),
    (1.8, 0.11, 0.12),
];

pub struct LensFlarePlugin;

impl Plugin for LensFlarePlugin {
    fn build(&self, app: &mut App) {
        // After transforms settle, so the rings sit where the sun shows.
        app.add_systems(
            PostUpdate,
            (keep_ghosts, place_ghosts)
                .chain()
                .after(TransformSystems::Propagate),
        );
    }
}

/// One ring of the flare over `camera`.
#[derive(Component)]
pub struct Ghost {
    camera: Entity,
    index: usize,
}

/// Gives each camera that shows the sky its rings, and takes them away
/// from cameras that no longer do.
fn keep_ghosts(
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera3d>, With<SkyApplied>)>,
    ghosts: Query<(Entity, &Ghost)>,
) {
    for (entity, ghost) in &ghosts {
        if !cameras.contains(ghost.camera) {
            commands.entity(entity).despawn();
        }
    }
    for camera in &cameras {
        if ghosts.iter().any(|(_, ghost)| ghost.camera == camera) {
            continue;
        }
        for index in 0..GHOSTS.len() {
            commands.spawn((
                Name::new("Lens flare"),
                Ghost { camera, index },
                EditorOnly,
                Node {
                    display: Display::None,
                    position_type: PositionType::Absolute,
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(Color::NONE),
                UiTargetCamera(camera),
                // Under the game's own UI.
                GlobalZIndex(-1000),
                Pickable::IGNORE,
            ));
        }
    }
}

/// Places the rings where the sun shows, or hides them.
fn place_ghosts(
    skies: Query<&Sky>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut ghosts: Query<(&Ghost, &mut Node, &mut BackgroundColor)>,
    mut ray_cast: MeshRayCast,
) {
    let sky = skies.iter().next();
    let mut seen = Vec::new();
    for (ghost, mut node, mut background) in &mut ghosts {
        let found = seen
            .iter()
            .find(|(camera, _)| *camera == ghost.camera)
            .map(|(_, shown)| *shown);
        let shown = found.unwrap_or_else(|| {
            let shown = sky.and_then(|sky| {
                let (camera, transform) = cameras.get(ghost.camera).ok()?;
                flare(sky, camera, transform, &mut ray_cast)
            });
            seen.push((ghost.camera, shown));
            shown
        });
        let Some((sun, size, strength)) = shown else {
            node.display = Display::None;
            continue;
        };
        let (along, across, alpha) = GHOSTS[ghost.index];
        let middle = size / 2.0;
        let center = sun + (middle - sun) * along;
        let diameter = size.y * across;
        node.display = Display::Flex;
        node.left = Val::Px(center.x - diameter / 2.0);
        node.top = Val::Px(center.y - diameter / 2.0);
        node.width = Val::Px(diameter);
        node.height = Val::Px(diameter);
        let color = sky.unwrap().sun.color.with_alpha(alpha * strength);
        if background.0 != color {
            background.0 = color;
        }
    }
}

/// Where the sun shows on `camera`'s picture in logical pixels, the
/// picture's size, and how strong the flare is there; None while it does
/// not show.
fn flare(
    sky: &Sky,
    camera: &Camera,
    transform: &GlobalTransform,
    ray_cast: &mut MeshRayCast,
) -> Option<(Vec2, Vec2, f32)> {
    let sun = &sky.sun;
    if !camera.is_active || sky.mode != SkyMode::Gradient || !sun.show || sun.flare <= 0.0 {
        return None;
    }
    let direction = Dir3::new(sun.direction).ok()?;
    if direction.y <= 0.0 || transform.forward().dot(*direction) <= 0.0 {
        return None;
    }
    let size = camera.logical_viewport_size()?;
    let at = camera
        .world_to_viewport(transform, transform.translation() + *direction)
        .ok()?;
    let strength = sun.flare.min(1.0) * fade(at, size);
    if strength <= 0.0 {
        return None;
    }
    let ray = Ray3d::new(transform.translation(), direction);
    let hidden = !ray_cast
        .cast_ray(ray, &MeshRayCastSettings::default())
        .is_empty();
    (!hidden).then_some((at, size, strength))
}

/// 1 with the sun in the middle of the picture, fading to 0 just past
/// its edges.
fn fade(at: Vec2, size: Vec2) -> f32 {
    let off = ((at - size / 2.0) / (size / 2.0)).abs().max_element();
    (1.0 - (off - 0.6) / 0.6).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fades_as_the_sun_nears_the_edge_and_goes_past_it() {
        let size = Vec2::new(1280.0, 720.0);
        assert_eq!(fade(size / 2.0, size), 1.0);
        let near_edge = fade(Vec2::new(1200.0, 360.0), size);
        assert!(near_edge > 0.0 && near_edge < 1.0);
        assert_eq!(fade(Vec2::new(1700.0, 360.0), size), 0.0);
    }

    #[test]
    fn rings_cameras_that_show_the_sky_and_only_while_they_do() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<Mesh>()
            .add_plugins((crate::sky::SkyPlugin, LensFlarePlugin));
        let camera = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        let rings = |app: &mut App| {
            app.world_mut()
                .query::<&Ghost>()
                .iter(app.world())
                .filter(|ghost| ghost.camera == camera)
                .count()
        };
        assert_eq!(rings(&mut app), 0, "no sky");

        let sky = app.world_mut().spawn(Sky::default()).id();
        app.update();
        app.update();
        assert_eq!(rings(&mut app), GHOSTS.len());
        app.update();
        assert_eq!(rings(&mut app), GHOSTS.len(), "once");

        app.world_mut().entity_mut(sky).despawn();
        app.update();
        app.update();
        assert_eq!(rings(&mut app), 0);
    }
}
