//! Hands the viewport's pointer and keys to the game while it plays. The
//! game renders into the editor's viewport and has no window of its own, so
//! the editor sends them as commands and they become Bevy's input messages:
//! `ButtonInput<KeyCode>`, `ButtonInput<Key>`, `ButtonInput<MouseButton>`,
//! `AccumulatedMouseMotion` and `AccumulatedMouseScroll` work as they do
//! with a window.
// ponytail: no window entity, so `Window::cursor_position` stays empty;
// `CursorMoved` carries the viewport position. Give the game a headless
// window if games need the cursor from it.

use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardFocusLost, KeyboardInput, NativeKey},
        mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel},
        touch::TouchPhase,
    },
    prelude::*,
    reflect::{
        TypeInfo, Typed,
        enums::{DynamicEnum, DynamicVariant, VariantInfo},
    },
    window::CursorMoved,
};

use crate::scene_view::{Button, PointerEvent};

/// The messages name a window; there is none.
const NO_WINDOW: Entity = Entity::PLACEHOLDER;

/// Where the pointer was last, for the motion to the next position.
#[derive(Resource, Default)]
struct LastPointer(Option<Vec2>);

/// One viewport pointer event, in viewport pixels, as the game's input.
pub(crate) fn pointer(world: &mut World, event: PointerEvent) {
    let button = |button, state| MouseButtonInput {
        button: match button {
            Button::Left => MouseButton::Left,
            Button::Middle => MouseButton::Middle,
            Button::Right => MouseButton::Right,
        },
        state,
        window: NO_WINDOW,
    };
    match event {
        PointerEvent::Down { x, y, button: down } => {
            moved(world, Vec2::new(x, y));
            world.write_message(button(down, ButtonState::Pressed));
        }
        PointerEvent::Move { x, y } => moved(world, Vec2::new(x, y)),
        PointerEvent::Up { x, y, button: up } => {
            moved(world, Vec2::new(x, y));
            world.write_message(button(up, ButtonState::Released));
        }
        // The editor's scroll is positive downwards, Bevy's upwards.
        PointerEvent::Scroll { delta } => {
            world.write_message(MouseWheel {
                unit: MouseScrollUnit::Pixel,
                x: 0.0,
                y: -delta,
                window: NO_WINDOW,
                phase: TouchPhase::Moved,
            });
        }
    }
}

fn moved(world: &mut World, position: Vec2) {
    let last = world
        .get_resource_or_init::<LastPointer>()
        .0
        .replace(position);
    let delta = last.map(|last| position - last);
    if let Some(delta) = delta.filter(|delta| *delta != Vec2::ZERO) {
        world.write_message(MouseMotion { delta });
    }
    world.write_message(CursorMoved {
        window: NO_WINDOW,
        position,
        delta,
    });
}

/// A key pressed or released in the viewport. `code` is the W3C name of
/// the physical key, which is how Bevy names its `KeyCode`s, like `KeyW`;
/// `text` is what the key typed, if anything.
pub(crate) fn key(
    world: &mut World,
    code: &str,
    pressed: bool,
    text: Option<String>,
) -> Result<(), String> {
    let key_code = unit_variant::<KeyCode>(code).ok_or_else(|| format!("{code} is not a key"))?;
    // Named keys share their code's name, like `Enter`; others are the text.
    let logical_key = unit_variant::<Key>(code)
        .or_else(|| text.clone().map(|text| Key::Character(text.into())))
        .unwrap_or(Key::Unidentified(NativeKey::Unidentified));
    world.write_message(KeyboardInput {
        key_code,
        logical_key,
        state: if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
        text: text.filter(|_| pressed).map(Into::into),
        repeat: false,
        window: NO_WINDOW,
    });
    Ok(())
}

/// The viewport lost the keyboard: whatever is held is let go.
pub(crate) fn focus_lost(world: &mut World) {
    world.write_message(KeyboardFocusLost);
}

/// The variant of `T` named `name` that holds no data.
fn unit_variant<T: FromReflect + Typed>(name: &str) -> Option<T> {
    let TypeInfo::Enum(info) = T::type_info() else {
        return None;
    };
    matches!(info.variant(name)?, VariantInfo::Unit(_))
        .then(|| T::from_reflect(&DynamicEnum::new(name, DynamicVariant::Unit)))?
}

#[cfg(test)]
mod tests {
    use bevy::input::{
        InputPlugin,
        mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    };

    use super::*;

    #[test]
    fn names_keys_the_way_bevy_does() {
        assert_eq!(unit_variant::<KeyCode>("KeyW"), Some(KeyCode::KeyW));
        assert_eq!(
            unit_variant::<KeyCode>("ShiftLeft"),
            Some(KeyCode::ShiftLeft)
        );
        assert_eq!(unit_variant::<Key>("Enter"), Some(Key::Enter));
        assert_eq!(unit_variant::<KeyCode>("Unidentified"), None);
        assert_eq!(unit_variant::<KeyCode>("Nope"), None);
    }

    #[test]
    fn turns_viewport_events_into_input() {
        let mut app = App::new();
        app.add_plugins(InputPlugin).add_message::<CursorMoved>();
        let world = app.world_mut();

        key(world, "KeyW", true, Some("w".into())).unwrap();
        key(world, "Space", true, Some(" ".into())).unwrap();
        pointer(
            world,
            PointerEvent::Down {
                x: 10.0,
                y: 10.0,
                button: Button::Left,
            },
        );
        pointer(world, PointerEvent::Move { x: 13.0, y: 14.0 });
        pointer(world, PointerEvent::Scroll { delta: 120.0 });
        assert!(key(world, "Nope", true, None).is_err());
        app.update();

        let world = app.world();
        let keys = world.resource::<ButtonInput<KeyCode>>();
        assert!(keys.just_pressed(KeyCode::KeyW) && keys.pressed(KeyCode::Space));
        let logical = world.resource::<ButtonInput<Key>>();
        assert!(logical.pressed(Key::Character("w".into())) && logical.pressed(Key::Space));
        assert!(
            world
                .resource::<ButtonInput<MouseButton>>()
                .just_pressed(MouseButton::Left)
        );
        assert_eq!(
            world.resource::<AccumulatedMouseMotion>().delta,
            Vec2::new(3.0, 4.0)
        );
        assert_eq!(world.resource::<AccumulatedMouseScroll>().delta.y, -120.0);

        focus_lost(app.world_mut());
        app.update();
        let keys = app.world().resource::<ButtonInput<KeyCode>>();
        assert!(!keys.pressed(KeyCode::KeyW), "focus loss lets keys go");
    }
}
