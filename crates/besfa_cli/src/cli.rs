use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "besfa",
    version,
    about = "Command-line tools for Besfa projects",
    arg_required_else_help = true
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Create a binary Rust project with Cargo.
    New {
        /// Directory in which to create the project.
        #[arg(value_name = "DIRECTORY")]
        directory: PathBuf,

        /// Format used to print the result.
        #[arg(long, value_enum)]
        output: Option<OutputFormat>,
    },
    /// Build Bevy into the shared target directory, so games start fast.
    Prebuild,
    /// Serve the editor's commands to an agent, an MCP server on stdin and
    /// stdout that runs the game in the current folder.
    Mcp,
    /// Build the game in the current folder with the features the editor
    /// runs it with, so the shared Bevy build is reused. Exits 30 when it
    /// does not build.
    Build {
        #[command(flatten)]
        args: CargoArgs,
    },
    /// Build the game's tests as `build` does, then run them. Exits 30 when
    /// they do not build and with cargo's code, 101, when a test fails.
    Test {
        #[command(flatten)]
        args: CargoArgs,
    },
    /// Build the game as `build` does, then run it in a window, as a player
    /// would, not in edit mode. Exits 30 when it does not build, else with
    /// the game's code.
    Run {
        #[command(flatten)]
        args: CargoArgs,
    },
    /// Validate a Besfa project directory.
    Validate {
        /// Project directory to validate.
        #[arg(default_value = ".", value_name = "DIRECTORY")]
        directory: PathBuf,
    },
}

/// What `build`, `test` and `run` pass on to cargo.
#[derive(Debug, Args)]
pub(crate) struct CargoArgs {
    /// Passed on to cargo, as `--release` or a test's name.
    #[arg(allow_hyphen_values = true)]
    cargo: Vec<String>,
    /// After `--`: passed on to the tests or the game.
    #[arg(last = true)]
    then: Vec<String>,
}

impl CargoArgs {
    /// The arguments as cargo takes them, with `--` before the tests' or
    /// the game's.
    pub(crate) fn all(&self) -> Vec<String> {
        let mut all = self.cargo.clone();
        if !self.then.is_empty() {
            all.push("--".into());
            all.extend(self.then.iter().cloned());
        }
        all
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum OutputFormat {
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_new_project_arguments() {
        let cli = Cli::try_parse_from(["besfa", "new", "projects/demo", "--output", "json"])
            .expect("the new command should parse");

        match cli.command {
            Commands::New { directory, output } => {
                assert_eq!(directory, PathBuf::from("projects/demo"));
                assert!(matches!(output, Some(OutputFormat::Json)));
            }
            _ => panic!("expected the new command"),
        }
    }

    #[test]
    fn parses_mcp() {
        let cli = Cli::try_parse_from(["besfa", "mcp"]).expect("mcp should parse");

        assert!(matches!(cli.command, Commands::Mcp));
    }

    #[test]
    fn passes_cargo_and_game_arguments_through() {
        let args = |line: &[&str]| match Cli::try_parse_from(line).map(|cli| cli.command) {
            Ok(Commands::Build { args } | Commands::Test { args } | Commands::Run { args }) => {
                args.all()
            }
            other => panic!("{line:?} parsed as {other:?}"),
        };

        assert!(args(&["besfa", "build"]).is_empty());
        assert_eq!(args(&["besfa", "build", "--release"]), ["--release"]);
        assert_eq!(args(&["besfa", "test", "spawns"]), ["spawns"]);
        assert_eq!(
            args(&["besfa", "test", "--lib", "--", "--nocapture"]),
            ["--lib", "--", "--nocapture"]
        );
        assert_eq!(
            args(&["besfa", "run", "--", "--level", "shop"]),
            ["--", "--level", "shop"]
        );
    }

    #[test]
    fn parses_prebuild() {
        let cli = Cli::try_parse_from(["besfa", "prebuild"]).expect("prebuild should parse");

        assert!(matches!(cli.command, Commands::Prebuild));
    }
}
