mod cli;
mod output;
mod project_creation;

use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use crate::{
    cli::{Cli, Commands},
    output::{print_create_error, print_create_success},
    project_creation::create_project,
};

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::New { directory, output }) => match create_project(&directory) {
            Ok(project_path) => {
                print_create_success(output, &project_path);
                ExitCode::SUCCESS
            }
            Err(error) => {
                print_create_error(output, &error);
                ExitCode::from(error.exit_code)
            }
        },
        Some(Commands::Validate { directory }) => {
            eprintln!(
                "Validating '{}' is not implemented yet.",
                directory.display()
            );
            ExitCode::from(30)
        }
        None => {
            let mut command = Cli::command();
            command.print_help().expect("printing help should succeed");
            println!();
            ExitCode::SUCCESS
        }
    }
}
