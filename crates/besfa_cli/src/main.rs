mod cli;
mod game;
mod output;
mod project_creation;
mod prune;

use std::{path::Path, process::ExitCode, time::Duration};

use clap::Parser;

use crate::{
    cli::{Cli, Commands},
    output::{print_create_error, print_create_success},
    project_creation::{create_project, prebuild},
};

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::New {
            directory,
            ccgs,
            output,
        } => match create_project(&directory, ccgs) {
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
        Commands::Screenshot { file, wait, args } => {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let wait = Duration::from_secs_f64(wait.max(0.0));
            match besfa_mcp::screenshot(Path::new("."), &file, wait, &args) {
                Ok(file) => {
                    println!("Screenshot saved to {}", file.display());
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::from(30)
                }
            }
        }
        // Stdout carries only the server's messages.
        Commands::Mcp => {
            besfa_mcp::serve();
            ExitCode::SUCCESS
        }
        // Exit codes on Windows go past 255, which ExitCode cannot carry.
        Commands::Build { args } => std::process::exit(game::cargo("build", &args.all())),
        Commands::Test { args } => std::process::exit(game::cargo("test", &args.all())),
        Commands::Run { args } => std::process::exit(game::cargo("run", &args.all())),
        Commands::Validate { directory } => {
            eprintln!(
                "Validating '{}' is not implemented yet.",
                directory.display()
            );
            ExitCode::from(30)
        }
    }
}
