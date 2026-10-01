mod cli;
mod output;
mod project_creation;

use std::process::ExitCode;

use clap::Parser;

use crate::{
    cli::{Cli, Commands},
    output::{print_create_error, print_create_success},
    project_creation::{create_project, prebuild},
};

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { directory, output } => match create_project(&directory) {
            Ok(project_path) => {
                print_create_success(output, &project_path);
                ExitCode::SUCCESS
            }
            Err(error) => {
                print_create_error(output, &error);
                ExitCode::from(error.exit_code)
            }
        },
        Commands::Prebuild => match prebuild() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                print_create_error(None, &error);
                ExitCode::from(error.exit_code)
            }
        },
        Commands::Validate { directory } => {
            eprintln!(
                "Validating '{}' is not implemented yet.",
                directory.display()
            );
            ExitCode::from(30)
        }
    }
}
