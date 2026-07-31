use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "besfa",
    version,
    about = "Command-line tools for Besfa projects"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,
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
            Some(Commands::New { directory, output }) => {
                assert_eq!(directory, PathBuf::from("projects/demo"));
                assert!(matches!(output, Some(OutputFormat::Json)));
            }
            _ => panic!("expected the new command"),
        }
    }
}
