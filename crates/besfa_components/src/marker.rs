//! Icons for what the scene view would not show otherwise: an empty that
//! marks a spot, like where enemies come from, and how far something
//! reaches. The editor draws them in edit mode and a click on the icon
//! selects the entity; the game never draws them. A game gives its own
//! component an icon by requiring a [`Marker`]:
//!
//! ```
//! # use bevy::prelude::*;
//! # use besfa_components::{Marker, MarkerIcon};
//! #[derive(Component)]
//! #[require(Marker = Marker::new(MarkerIcon::Flag, Color::srgb(0.9, 0.2, 0.2)))]
//! struct SpawnPoint;
//! ```

use bevy::prelude::*;

/// An icon at the entity's place, and a ring on the ground around it when
/// `radius` is above zero.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::marker"]
#[reflect(Component, Default)]
pub struct Marker {
    pub icon: MarkerIcon,
    pub color: Color,
    /// Meters the ring reaches, as a tower's range; none at zero.
    pub radius: f32,
}

#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[type_path = "besfa_editor_plugin::marker"]
pub enum MarkerIcon {
    /// A flag on a pole, for a spot on the ground.
    #[default]
    Flag,
    /// A diamond, for a point in the air.
    Diamond,
}

impl Default for Marker {
    fn default() -> Self {
        Self::new(MarkerIcon::Flag, Color::srgb(0.95, 0.75, 0.2))
    }
}

impl Marker {
    pub const fn new(icon: MarkerIcon, color: Color) -> Self {
        Self {
            icon,
            color,
            radius: 0.0,
        }
    }

    pub const fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

/// How tall an icon stands, in meters.
pub const ICON_HEIGHT: f32 = 1.6;

impl Marker {
    /// Lines that draw the icon standing at `at`, as pairs of points.
    pub fn lines(&self, at: Vec3) -> Vec<(Vec3, Vec3)> {
        match self.icon {
            MarkerIcon::Flag => {
                let top = at + Vec3::Y * ICON_HEIGHT;
                let tip = top + Vec3::new(0.7, -0.25, 0.0);
                let low = top - Vec3::Y * 0.5;
                vec![(at, top), (top, tip), (tip, low)]
            }
            MarkerIcon::Diamond => {
                let center = at + Vec3::Y * ICON_HEIGHT / 2.0;
                let (up, down) = (center + Vec3::Y * 0.4, center - Vec3::Y * 0.4);
                let sides = [Vec3::X, Vec3::Z, Vec3::NEG_X, Vec3::NEG_Z].map(|side| center + side * 0.3);
                let mut lines = Vec::new();
                for (index, side) in sides.iter().enumerate() {
                    lines.push((up, *side));
                    lines.push((down, *side));
                    lines.push((*side, sides[(index + 1) % 4]));
                }
                lines
            }
        }
    }

    /// The icon's middle, where a click picks it.
    pub fn center(at: Vec3) -> Vec3 {
        at + Vec3::Y * ICON_HEIGHT / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component)]
    #[require(Marker = Marker::new(MarkerIcon::Diamond, Color::WHITE).with_radius(12.0))]
    struct Tower;

    #[test]
    fn a_game_component_brings_its_icon() {
        let mut world = World::new();
        let tower = world.spawn(Tower).id();
        let marker = world.get::<Marker>(tower).unwrap();
        assert_eq!((marker.icon, marker.radius), (MarkerIcon::Diamond, 12.0));
        // A flag stands on the ground: its pole starts at the entity's place.
        let lines = Marker::default().lines(Vec3::ONE);
        assert_eq!(lines[0].0, Vec3::ONE);
        assert!(lines.iter().all(|(a, b)| a.y >= 1.0 && b.y >= 1.0));
    }
}
