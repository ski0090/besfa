//! Holds the game still until the editor says play.
//!
//! The editor sets `BESFA_EDIT_MODE` and the game starts with virtual time
//! paused: Startup systems build the scene, `Update` sees no time pass and
//! `FixedUpdate` does not run. The line `play` on stdin unpauses it once. The
//! editor ends a play session by killing the process, which resets the scene.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use bevy::prelude::*;

pub(crate) fn requested() -> bool {
    std::env::var_os("BESFA_EDIT_MODE").is_some()
}

// ponytail: pauses virtual time only, so systems that ignore `Time` or read
// `Time<Real>` keep running. Gate gameplay on an edit/play `State` if that
// starts to matter.
pub(crate) struct EditModePlugin;

/// Set by the stdin reader when the editor sends `play`.
#[derive(Resource)]
struct PlayRequested(Arc<AtomicBool>);

impl Plugin for EditModePlugin {
    fn build(&self, app: &mut App) {
        let requested = Arc::new(AtomicBool::new(false));
        let reader = requested.clone();
        // ponytail: stdin is the editor's only channel into the game; move to
        // besfa_protocol IPC once the editor sends more than `play`.
        std::thread::spawn(move || {
            for line in std::io::stdin().lines().map_while(Result::ok) {
                if line.trim() == "play" {
                    reader.store(true, Ordering::Relaxed);
                }
            }
        });

        app.insert_resource(PlayRequested(requested))
            .add_systems(PreStartup, pause)
            .add_systems(PreUpdate, play_when_requested);
    }
}

fn pause(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

/// Unpauses once per request, so a game that pauses itself stays paused.
fn play_when_requested(requested: Res<PlayRequested>, mut time: ResMut<Time<Virtual>>) {
    if requested.0.swap(false, Ordering::Relaxed) {
        time.unpause();
        info!("Playing.");
    }
}

#[cfg(test)]
mod tests {
    use bevy::time::TimePlugin;

    use super::*;

    fn paused(app: &App) -> bool {
        app.world().resource::<Time<Virtual>>().is_paused()
    }

    #[test]
    fn starts_paused_and_plays_once_asked() {
        let mut app = App::new();
        app.add_plugins((TimePlugin, EditModePlugin));
        app.update();
        assert!(paused(&app));

        app.world()
            .resource::<PlayRequested>()
            .0
            .store(true, Ordering::Relaxed);
        app.update();
        assert!(!paused(&app));

        // The request is used up: the game's own pause sticks.
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        app.update();
        assert!(paused(&app));
    }
}
