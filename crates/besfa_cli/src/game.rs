//! `besfa build`, `test` and `run`: cargo in the current folder with the
//! features the editor runs the game with, so they reuse its Bevy build in
//! the shared target instead of compiling another.

use std::{fs, process::Command};

/// Exit code of a game that did not build, as `besfa prebuild`'s.
const BUILD_FAILED: i32 = 30;

/// Builds the game, then for `test` and `run` runs its tests or itself
/// with `args`, and returns the exit code. Cargo's output passes through
/// unchanged, since agents read its `test result:` and `error[E..]` lines.
pub(crate) fn cargo(command: &str, args: &[String]) -> i32 {
    let manifest = fs::read_to_string("Cargo.toml").unwrap_or_default();
    let features = besfa_mcp::run_features(&manifest);
    // Built on its own first, so a compile error exits apart from a test
    // that fails, which cargo gives the same code.
    let (build, then) = match command {
        "test" => (["test", "--no-run"].as_slice(), Some("test")),
        "run" => (["build"].as_slice(), Some("run")),
        _ => (["build"].as_slice(), None),
    };
    // What follows `--` is for the tests or the game, which a build refuses.
    let cargo_args = args.split(|arg| arg == "--").next().unwrap_or_default();
    let built = run(build, &features, cargo_args);
    if built != 0 {
        return BUILD_FAILED;
    }
    match then {
        Some(then) => run(&[then], &features, args),
        None => 0,
    }
}

/// Runs `cargo <command> --features <features> <args>` and returns its exit
/// code, or [`BUILD_FAILED`] when cargo could not start.
fn run(command: &[&str], features: &str, args: &[String]) -> i32 {
    let status = Command::new("cargo")
        .args(command)
        .args(["--features", features])
        .args(args)
        .status();
    match status {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("Failed to start Cargo: {error}");
            BUILD_FAILED
        }
    }
}
