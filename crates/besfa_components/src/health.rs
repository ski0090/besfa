//! Health that harm takes and rest fills. Trigger [`Damage`] on an entity
//! with [`Health`] to harm it; once it reaches zero, [`Died`] is triggered
//! on it once, and the game decides what dying means. It fills by itself
//! after a while unharmed, and not while it is at zero. While `immune`
//! lasts, harm does not take from it; `armor` keeps off its part of harm.

use bevy::prelude::*;

/// In [`crate::default_plugins`]; add it on its own to a game that runs
/// without them.
pub struct HealthPlugin;

impl Plugin for HealthPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, regenerate).add_observer(harm);
    }
}

#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::health"]
#[reflect(Component, Default)]
pub struct Health {
    pub value: f32,
    pub max: f32,
    /// Filled each second once `rest` seconds went by unharmed.
    pub regen: f32,
    pub rest: f32,
    /// Seconds since it was last harmed.
    pub unharmed: f32,
    /// Seconds left that harm does not take from it, as while rolling.
    pub immune: f32,
    /// The part of each harm kept off, 0 to 1, as worn armor keeps it.
    #[reflect(default)]
    pub armor: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self::new(100.0)
    }
}

impl Health {
    /// Full at `max`, never filling by itself.
    pub fn new(max: f32) -> Self {
        Self {
            value: max,
            max,
            regen: 0.0,
            rest: 5.0,
            unharmed: 0.0,
            immune: 0.0,
            armor: 0.0,
        }
    }

    pub fn dead(&self) -> bool {
        self.value <= 0.0
    }

    /// How full it is, 0 to 1.
    pub fn fraction(&self) -> f32 {
        if self.max > 0.0 {
            (self.value / self.max).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Fills it by `amount`, up to `max`; the dead stay dead.
    pub fn heal(&mut self, amount: f32) {
        if !self.dead() {
            self.value = (self.value + amount).min(self.max);
        }
    }
}

/// Harm to an entity's [`Health`], from `source` if something dealt it.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct Damage {
    pub entity: Entity,
    pub amount: f32,
    pub source: Option<Entity>,
}

/// An entity's [`Health`] reached zero.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct Died {
    pub entity: Entity,
    /// What dealt the last harm.
    pub source: Option<Entity>,
}

fn harm(damage: On<Damage>, mut commands: Commands, mut healths: Query<&mut Health>) {
    let Ok(mut health) = healths.get_mut(damage.entity) else {
        return;
    };
    if health.dead() || damage.amount <= 0.0 || health.immune > 0.0 {
        return;
    }
    let taken = damage.amount * (1.0 - health.armor.clamp(0.0, 1.0));
    health.value = (health.value - taken).max(0.0);
    health.unharmed = 0.0;
    if health.dead() {
        commands.trigger(Died {
            entity: damage.entity,
            source: damage.source,
        });
    }
}

fn regenerate(time: Res<Time>, mut healths: Query<&mut Health>) {
    let delta = time.delta_secs();
    if delta == 0.0 {
        return;
    }
    for mut health in &mut healths {
        // Only on change, so a full one does not tell the Inspector.
        let mut next = *health;
        next.unharmed += delta;
        next.immune = (next.immune - delta).max(0.0);
        if next.unharmed >= next.rest {
            next.heal(next.regen * delta);
        }
        // Counting time alone is no change worth telling.
        if next.value != health.value {
            *health = next;
        } else {
            let quiet = health.bypass_change_detection();
            quiet.unharmed = next.unharmed;
            quiet.immune = next.immune;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[derive(Resource, Default)]
    struct Deaths(u32);

    #[test]
    fn harm_takes_it_to_zero_once_and_rest_fills_it_again() {
        let mut app = App::new();
        app.add_plugins(HealthPlugin)
            .init_resource::<Time>()
            .init_resource::<Deaths>()
            .add_observer(|_: On<Died>, mut deaths: ResMut<Deaths>| deaths.0 += 1);
        let entity = app
            .world_mut()
            .spawn(Health {
                regen: 10.0,
                rest: 1.0,
                ..Health::new(50.0)
            })
            .id();
        let health = |app: &App| *app.world().get::<Health>(entity).unwrap();
        let hit = |app: &mut App, amount| {
            app.world_mut().trigger(Damage {
                entity,
                amount,
                source: None,
            });
            app.world_mut().flush();
        };
        let step = |app: &mut App, secs: f32| {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs_f32(secs));
            app.update();
        };

        hit(&mut app, 20.0);
        assert_eq!(health(&app).value, 30.0);
        // Resting a second, then filling 10 a second.
        step(&mut app, 0.5);
        assert_eq!(health(&app).value, 30.0);
        step(&mut app, 0.5);
        step(&mut app, 0.5);
        assert!(health(&app).value > 30.0, "{:?}", health(&app));

        // Immune for a moment: harm then takes nothing.
        app.world_mut().get_mut::<Health>(entity).unwrap().immune = 0.4;
        let before = health(&app).value;
        hit(&mut app, 5.0);
        assert_eq!(health(&app).value, before);
        // Armor keeps off its part.
        let mut worn = app.world_mut().get_mut::<Health>(entity).unwrap();
        (worn.immune, worn.armor) = (0.0, 0.5);
        hit(&mut app, 10.0);
        assert_eq!(health(&app).value, before - 5.0);
        step(&mut app, 0.5);
        hit(&mut app, 100.0);
        hit(&mut app, 10.0);
        assert!(health(&app).dead());
        assert_eq!(app.world().resource::<Deaths>().0, 1);
        step(&mut app, 5.0);
        assert!(health(&app).dead(), "the dead do not fill");
    }

    #[test]
    fn reads_health_saved_before_it_had_armor() {
        use bevy::reflect::{FromReflect, TypeRegistry, serde::TypedReflectDeserializer};
        use serde::de::DeserializeSeed;

        let mut registry = TypeRegistry::default();
        registry.register::<Health>();
        let registration = registry.get(std::any::TypeId::of::<Health>()).unwrap();
        let saved = "(value: 5.0, max: 10.0, regen: 0.0, rest: 5.0, unharmed: 0.0, immune: 0.0)";
        let mut ron = ron::Deserializer::from_str(saved).unwrap();
        let read = TypedReflectDeserializer::new(registration, &registry)
            .deserialize(&mut ron)
            .unwrap();
        let health = Health::from_reflect(read.as_partial_reflect()).unwrap();
        assert_eq!((health.value, health.armor), (5.0, 0.0));
    }
}
