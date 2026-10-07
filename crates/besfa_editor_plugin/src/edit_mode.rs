//! Holds the game still until the editor says play, and takes the editor's
//! other commands.
//!
//! The editor sets `BESFA_EDIT_MODE` and the game starts with virtual time
//! paused: Startup systems build the scene, `Update` sees no time pass and
//! `FixedUpdate` does not run. The editor writes one command per line to
//! stdin: `play` unpauses once, `select <id>` picks the entity that `inspect`
//! reports, `select` alone clears it and `save` writes the scene file. The
//! editor ends a play session by killing the process, which resets the scene.

use std::sync::{Mutex, PoisonError, mpsc};

use bevy::prelude::*;

use crate::{
    inspect::Selected,
    scene::{SCENE_PATH, save, scene_file},
};

pub(crate) fn requested() -> bool {
    std::env::var_os("BESFA_EDIT_MODE").is_some()
}

// ponytail: pauses virtual time only, so systems that ignore `Time` or read
// `Time<Real>` keep running. Gate gameplay on an edit/play `State` if that
// starts to matter.
pub(crate) struct EditModePlugin;

/// Lines the editor wrote to stdin, one command each.
#[derive(Resource)]
pub(crate) struct EditorCommands(pub(crate) Mutex<mpsc::Receiver<String>>);

impl Plugin for EditModePlugin {
    fn build(&self, app: &mut App) {
        let (sender, receiver) = mpsc::channel();
        // ponytail: the editor talks over the game's stdin and stdout (see
        // `inspect` for the replies). A besfa_protocol crate can take over
        // once the editor and the game need shared message types.
        std::thread::spawn(move || {
            for line in std::io::stdin().lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    return;
                }
            }
        });

        app.insert_resource(EditorCommands(Mutex::new(receiver)))
            .init_resource::<Selected>()
            .add_systems(PreStartup, pause)
            .add_systems(PreUpdate, run_commands);
    }
}

fn pause(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

/// Exclusive, because saving reads the whole world.
fn run_commands(world: &mut World) {
    let lines: Vec<String> = {
        let commands = world.resource::<EditorCommands>();
        let receiver = commands.0.lock().unwrap_or_else(PoisonError::into_inner);
        receiver.try_iter().collect()
    };
    for line in lines {
        let line = line.trim();
        let (command, argument) = line.split_once(' ').unwrap_or((line, ""));
        match command {
            // Unpauses once per request, so a game that pauses itself stays paused.
            "play" => {
                world.resource_mut::<Time<Virtual>>().unpause();
                info!("Playing.");
            }
            // An id the editor got from `inspect`; anything else clears.
            "select" => {
                world.resource_mut::<Selected>().0 =
                    argument.trim().parse().ok().and_then(Entity::try_from_bits);
            }
            "save" => match save(world, &scene_file()) {
                Ok(()) => info!("Saved {SCENE_PATH}"),
                Err(error) => error!("Could not save {SCENE_PATH}: {error}"),
            },
            _ => warn!("Unknown editor command: {line}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::time::TimePlugin;

    use super::*;

    fn editor_app() -> (App, mpsc::Sender<String>) {
        let mut app = App::new();
        app.add_plugins((TimePlugin, EditModePlugin));
        // Replaces stdin with the test's channel.
        let (sender, receiver) = mpsc::channel();
        app.insert_resource(EditorCommands(Mutex::new(receiver)));
        (app, sender)
    }

    fn paused(app: &App) -> bool {
        app.world().resource::<Time<Virtual>>().is_paused()
    }

    #[test]
    fn starts_paused_and_plays_once_asked() {
        let (mut app, editor) = editor_app();
        app.update();
        assert!(paused(&app));

        editor.send("play".into()).unwrap();
        app.update();
        assert!(!paused(&app));

        // The request is used up: the game's own pause sticks.
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        app.update();
        assert!(paused(&app));
    }

    #[test]
    fn selects_an_entity_by_id_and_clears_on_a_bare_select() {
        let (mut app, editor) = editor_app();
        let cube = app.world_mut().spawn_empty().id();

        editor.send(format!("select {}", cube.to_bits())).unwrap();
        app.update();
        assert_eq!(app.world().resource::<Selected>().0, Some(cube));

        editor.send("select".into()).unwrap();
        app.update();
        assert_eq!(app.world().resource::<Selected>().0, None);
    }
}
