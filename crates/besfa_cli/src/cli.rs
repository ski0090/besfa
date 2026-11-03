use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

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

        /// Also add Claude Code Game Studios: agents, skills and hooks for
        /// making the game with Claude Code. Needs Git and the network.
        #[arg(long)]
        ccgs: bool,

        /// Format used to print the result.
        #[arg(long, value_enum)]
        output: Option<OutputFormat>,
    },
    /// Build Bevy into the shared target directory, so games start fast.
    Prebuild,
    /// Run the game in the current folder without a window, as `mcp` does,
    /// play it and save what the player sees, its cameras and UI, to a PNG.
    /// Exits 30 when no picture was saved.
    Screenshot {
        /// The .png file to write, inside the project.
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Seconds the game plays before the picture.
        #[arg(long, default_value_t = 0.0, value_name = "SECONDS")]
        wait: f64,
        /// After `--`: the game's own launch arguments.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Serve the editor's commands to an agent, an MCP server on stdin and
    /// stdout that runs the game in the current folder.
    Mcp,
    /// Validate a Besfa project directory.
    Validate {
        /// Project directory to validate.
        #[arg(default_value = ".", value_name = "DIRECTORY")]
        directory: PathBuf,
    },
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
            Commands::New {
                directory,
                ccgs,
                output,
            } => {
                assert_eq!(directory, PathBuf::from("projects/demo"));
                assert!(!ccgs, "CCGS is opt-in");
                assert!(matches!(output, Some(OutputFormat::Json)));
            }
            _ => panic!("expected the new command"),
        }

        let cli =
            Cli::try_parse_from(["besfa", "new", "demo", "--ccgs"]).expect("--ccgs should parse");
        assert!(matches!(cli.command, Commands::New { ccgs: true, .. }));
    }

    #[test]
    fn parses_mcp() {
        let cli = Cli::try_parse_from(["besfa", "mcp"]).expect("mcp should parse");

        assert!(matches!(cli.command, Commands::Mcp));
    }

    #[test]
    fn parses_screenshot_with_the_game_arguments() {
        let line = [
            "besfa",
            "screenshot",
            "shots/a.png",
            "--wait",
            "2",
            "--",
            "--level",
            "shop",
        ];
        match Cli::try_parse_from(line).map(|cli| cli.command) {
            Ok(Commands::Screenshot { file, wait, args }) => {
                assert_eq!(file, PathBuf::from("shots/a.png"));
                assert_eq!(wait, 2.0);
                assert_eq!(args, ["--level", "shop"]);
            }
            other => panic!("parsed as {other:?}"),
        }
    }

    #[test]
    fn parses_prebuild() {
        let cli = Cli::try_parse_from(["besfa", "prebuild"]).expect("prebuild should parse");

        assert!(matches!(cli.command, Commands::Prebuild));
    }
}
