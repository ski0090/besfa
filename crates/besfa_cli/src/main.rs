use std::{io, path::PathBuf};

use clap::{CommandFactory, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "besfa",
    version,
    about = "Command-line tools for Besfa projects"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create a project from a Besfa template.
    New {
        /// Directory in which to create the project.
        #[arg(value_name = "DIRECTORY")]
        directory: PathBuf,

        /// Template used as the starting point.
        #[arg(long, default_value = "basic_3d")]
        template: String,
    },
    /// Validate a Besfa project directory.
    Validate {
        /// Project directory to validate.
        #[arg(default_value = ".", value_name = "DIRECTORY")]
        directory: PathBuf,
    },
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::New {
            directory,
            template,
        }) => {
            println!(
                "Creating a project in '{}' from the '{}' template is not implemented yet.",
                directory.display(),
                template,
            );
        }
        Some(Commands::Validate { directory }) => {
            println!(
                "Validating '{}' is not implemented yet.",
                directory.display(),
            );
        }
        None => {
            let mut command = Cli::command();
            command.print_help()?;
            println!();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_new_project_arguments() {
        let cli = Cli::try_parse_from(["besfa", "new", "projects/demo", "--template", "basic_3d"])
            .expect("the new command should parse");

        match cli.command {
            Some(Commands::New {
                directory,
                template,
            }) => {
                assert_eq!(directory, PathBuf::from("projects/demo"));
                assert_eq!(template, "basic_3d");
            }
            _ => panic!("expected the new command"),
        }
    }
}
