//! UI kept over a place in the world: a damage number over what was hit, a
//! name over a character, a warning over a building. Put [`WorldLabel`] on
//! a UI node with absolute position; each frame it moves to where its
//! place shows on the screen of its `UiTargetCamera`, or the only camera,
//! and hides while the place is behind it.

use bevy::{prelude::*, ui::UiSystems};

/// Moves [`WorldLabel`] nodes once the world has moved and before the UI
/// lays them out.
pub struct WorldLabelPlugin;

impl Plugin for WorldLabelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            follow
                .after(TransformSystems::Propagate)
                .before(UiSystems::Layout),
        );
    }
}

/// On an absolutely placed UI node: the place it is kept over, `offset`
/// from `entity`'s, or from the world's origin with no entity.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::world_label"]
#[reflect(Component, Default)]
pub struct WorldLabel {
    pub entity: Option<Entity>,
    pub offset: Vec3,
    /// Pixels from the node's top left corner to the place: half its size
    /// centers it.
    pub anchor: Vec2,
    /// Kept this many pixels inside the screen's top edge when its place is
    /// above it, so that a warning over something tall is still seen.
    pub top_margin: Option<f32>,
}

impl WorldLabel {
    /// Over `entity`, raised by `height`.
    pub fn over(entity: Entity, height: f32) -> Self {
        Self {
            entity: Some(entity),
            offset: Vec3::Y * height,
            ..default()
        }
    }

    /// At a place in the world.
    pub fn at(place: Vec3) -> Self {
        Self {
            offset: place,
            ..default()
        }
    }

    /// With `anchor` pixels from the node's corner to its place.
    pub fn anchored(mut self, anchor: Vec2) -> Self {
        self.anchor = anchor;
        self
    }

    /// Kept `margin` pixels inside the screen's top edge.
    pub fn kept_below_top(mut self, margin: f32) -> Self {
        self.top_margin = Some(margin);
        self
    }
}

/// Where `label` puts its node's top left corner for its place showing at
/// `pixel` of the screen.
fn corner(label: &WorldLabel, pixel: Vec2) -> Vec2 {
    let corner = pixel - label.anchor;
    match label.top_margin {
        Some(margin) => corner.with_y(corner.y.max(margin)),
        None => corner,
    }
}

#[allow(clippy::type_complexity, reason = "a system's parameters")]
fn follow(
    mut labels: Query<(&WorldLabel, &mut Node, Option<&UiTargetCamera>)>,
    places: Query<&GlobalTransform>,
    cameras: Query<(Entity, &Camera, &GlobalTransform)>,
) {
    let only = cameras.single().ok().map(|(camera, ..)| camera);
    for (label, mut node, target) in &mut labels {
        let place = match label.entity {
            Some(entity) => match places.get(entity) {
                Ok(at) => at.translation() + label.offset,
                Err(_) => {
                    node.display = Display::None;
                    continue;
                }
            },
            None => label.offset,
        };
        let shown = target
            .map(|target| target.entity())
            .or(only)
            .and_then(|camera| cameras.get(camera).ok())
            .and_then(|(_, camera, eye)| camera.world_to_viewport(eye, place).ok())
            .map(|pixel| corner(label, pixel));
        match shown {
            Some(pixel) => {
                node.left = Val::Px(pixel.x);
                node.top = Val::Px(pixel.y);
                node.display = Display::Flex;
            }
            None => node.display = Display::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_follows_its_entity_and_hides_once_it_is_gone() {
        let mut app = App::new();
        app.add_systems(Update, follow);
        let camera = app
            .world_mut()
            .spawn((Camera::default(), Transform::default(), GlobalTransform::default()))
            .id();
        let thing = app.world_mut().spawn(GlobalTransform::from_xyz(0.0, 0.0, -5.0)).id();
        let label = app
            .world_mut()
            .spawn((Node::default(), WorldLabel::over(thing, 1.0), UiTargetCamera(camera)))
            .id();
        app.update();
        // Without a window the camera has no viewport: the label hides.
        assert_eq!(app.world().get::<Node>(label).unwrap().display, Display::None);
        app.world_mut().despawn(thing);
        app.update();
        assert_eq!(app.world().get::<Node>(label).unwrap().display, Display::None);
    }

    #[test]
    fn a_label_kept_below_the_top_does_not_go_above_it() {
        let label = WorldLabel::at(Vec3::ZERO).anchored(Vec2::new(10.0, 5.0));
        assert_eq!(corner(&label, Vec2::new(100.0, 50.0)), Vec2::new(90.0, 45.0));
        assert_eq!(corner(&label, Vec2::new(100.0, -30.0)), Vec2::new(90.0, -35.0));
        let kept = label.kept_below_top(4.0);
        assert_eq!(corner(&kept, Vec2::new(100.0, -30.0)), Vec2::new(90.0, 4.0));
    }
}
