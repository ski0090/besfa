use std::path::Path;

use serde_json::json;

use crate::{cli::OutputFormat, project_creation::CreateError};

pub(crate) fn print_create_success(output: Option<OutputFormat>, project_path: &Path) {
    match output {
        Some(OutputFormat::Json) => println!(
            "{}",
            json!({ "status": "success", "project_path": project_path })
        ),
        None => println!("Created project at '{}'.", project_path.display()),
    }
}

pub(crate) fn print_create_error(output: Option<OutputFormat>, error: &CreateError) {
    if let Some(diagnostic) = &error.diagnostic {
        eprint!("{diagnostic}");
        if !diagnostic.ends_with('\n') {
            eprintln!();
        }
    }

    match output {
        Some(OutputFormat::Json) => println!(
            "{}",
            json!({
                "status": "error",
                "code": error.code,
                "message": error.message,
            })
        ),
        None => eprintln!("{}", error.message),
    }
}
