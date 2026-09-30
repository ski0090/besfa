use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
pub(crate) struct CreateError {
    pub(crate) exit_code: u8,
    pub(crate) code: &'static str,
    pub(crate) message: String,
    pub(crate) diagnostic: Option<String>,
}

impl CreateError {
    fn new(exit_code: u8, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            code,
            message: message.into(),
            diagnostic: None,
        }
    }

    fn with_diagnostic(mut self, diagnostic: String) -> Self {
        self.diagnostic = Some(diagnostic);
        self
    }
}

pub(crate) fn create_project(directory: &Path) -> Result<PathBuf, CreateError> {
    create_project_with(directory, shared_dir().as_deref())
}

/// Machine-wide Besfa data: `%LOCALAPPDATA%\Besfa`.
fn shared_dir() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("Besfa"))
}

fn create_project_with(directory: &Path, shared: Option<&Path>) -> Result<PathBuf, CreateError> {
    let destination = absolute_path(directory)?;

    if destination.exists() {
        return Err(CreateError::new(
            10,
            "destination_exists",
            "The destination directory already exists.",
        ));
    }

    let project_name = destination
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            CreateError::new(
                11,
                "invalid_project_name",
                "The destination must include a project directory name.",
            )
        })?;
    let parent = destination.parent().ok_or_else(|| {
        CreateError::new(
            11,
            "invalid_project_name",
            "The destination must include a parent directory.",
        )
    })?;
    let staging_dir = create_staging_dir(parent)?;
    let staged_project = staging_dir.join(project_name);

    let cargo_output = Command::new("cargo")
        .arg("new")
        .arg("--bin")
        .arg(&staged_project)
        .output()
        .map_err(|error| {
            CreateError::new(
                30,
                "cargo_execution_failed",
                "Failed to start Cargo while creating the project.",
            )
            .with_diagnostic(error.to_string())
        })?;

    if !cargo_output.status.success() {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(CreateError::new(
            11,
            "invalid_project_name",
            "Cargo rejected the project creation request.",
        )
        .with_diagnostic(String::from_utf8_lossy(&cargo_output.stderr).into_owned()));
    }

    if !staged_project.is_dir() {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(CreateError::new(
            30,
            "internal_error",
            "Cargo completed without creating the expected project directory.",
        ));
    }

    if let Err(error) = write_bevy_template(&staged_project, shared) {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(error);
    }

    match fs::rename(&staged_project, &destination) {
        Ok(()) => {
            let _ = fs::remove_dir(&staging_dir);
            Ok(destination)
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging_dir);
            let (exit_code, code, message) = if destination.exists() {
                (
                    10,
                    "destination_exists",
                    "The destination directory already exists.",
                )
            } else {
                (
                    20,
                    "filesystem_error",
                    "Failed to move the created project into place.",
                )
            };
            Err(CreateError::new(exit_code, code, message).with_diagnostic(error.to_string()))
        }
    }
}

const TEMPLATE_MAIN: &str = include_str!("../template/main.rs");

/// Appended to the `[dependencies]` table that ends Cargo's generated manifest.
const TEMPLATE_DEPENDENCIES: &str = r#"bevy = "0.19.1"
besfa_editor_plugin = { git = "https://github.com/ski0090/besfa" }

# Bevy is slow unoptimized: optimize dependencies, keep the game quick to rebuild.
[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3
"#;

/// Turns Cargo's hello-world project into a Bevy game with a cube scene.
///
/// With a machine-wide `shared` directory, the game also builds into
/// `<shared>/target` and starts from `<shared>/prebuild/Cargo.lock`, so every
/// Besfa game reuses one Bevy build instead of compiling its own.
fn write_bevy_template(project: &Path, shared: Option<&Path>) -> Result<(), CreateError> {
    let filesystem_error = |message: &str, error: io::Error| {
        CreateError::new(20, "filesystem_error", message).with_diagnostic(error.to_string())
    };

    let manifest_path = project.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|error| filesystem_error("Failed to read the generated Cargo.toml.", error))?;
    if !manifest.trim_end().ends_with("[dependencies]") {
        return Err(CreateError::new(
            30,
            "internal_error",
            "Cargo generated a manifest that does not end with [dependencies].",
        ));
    }

    fs::write(
        &manifest_path,
        format!("{}\n{TEMPLATE_DEPENDENCIES}", manifest.trim_end()),
    )
    .map_err(|error| filesystem_error("Failed to write Cargo.toml.", error))?;
    fs::write(project.join("src").join("main.rs"), TEMPLATE_MAIN)
        .map_err(|error| filesystem_error("Failed to write src/main.rs.", error))?;

    let Some(shared) = shared else {
        return Ok(());
    };

    // Same dependency versions as the prebuild, or Cargo rebuilds everything.
    let prebuild_lock = shared.join("prebuild").join("Cargo.lock");
    if prebuild_lock.is_file() {
        fs::copy(&prebuild_lock, project.join("Cargo.lock"))
            .map_err(|error| filesystem_error("Failed to copy the prebuild Cargo.lock.", error))?;
    }

    let cargo_dir = project.join(".cargo");
    fs::create_dir_all(&cargo_dir)
        .map_err(|error| filesystem_error("Failed to create .cargo.", error))?;
    fs::write(
        cargo_dir.join("config.toml"),
        cargo_config(&shared.join("target")),
    )
    .map_err(|error| filesystem_error("Failed to write .cargo/config.toml.", error))
}

fn cargo_config(target_dir: &Path) -> String {
    // Forward slashes need no escaping in a TOML string and work on Windows.
    let target_dir = target_dir.to_string_lossy().replace('\\', "/");
    format!(
        "# Shared by every Besfa game on this machine, so Bevy is compiled once.\n\
         [build]\n\
         target-dir = \"{target_dir}\"\n"
    )
}

fn absolute_path(path: &Path) -> Result<PathBuf, CreateError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        env::current_dir()
            .map(|current_dir| current_dir.join(path))
            .map_err(|error| {
                CreateError::new(
                    20,
                    "filesystem_error",
                    "Failed to determine the current working directory.",
                )
                .with_diagnostic(error.to_string())
            })
    }
}

fn create_staging_dir(parent: &Path) -> Result<PathBuf, CreateError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();

    for attempt in 0..32 {
        let path = parent.join(format!(
            ".besfa-new-{}-{timestamp}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(CreateError::new(
                    20,
                    "filesystem_error",
                    "Failed to create a temporary project directory.",
                )
                .with_diagnostic(error.to_string()));
            }
        }
    }

    Err(CreateError::new(
        20,
        "filesystem_error",
        "Failed to allocate a unique temporary project directory.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_cargo_binary_project_in_the_requested_directory() {
        let parent = test_directory("create");
        fs::create_dir(&parent).expect("test parent should be created");
        let destination = parent.join("demo_game");

        let result = create_project_with(&destination, None);
        let manifest = fs::read_to_string(destination.join("Cargo.toml"));
        let has_cargo_config = destination.join(".cargo").exists();
        let main = fs::read_to_string(destination.join("src").join("main.rs"));
        fs::remove_dir_all(&parent).expect("test directory should be removed");

        assert_eq!(
            result.expect("project creation should succeed"),
            destination
        );
        let manifest = manifest.expect("Cargo.toml should be created");
        assert!(manifest.contains("name = \"demo_game\""));
        assert!(manifest.contains("[dependencies]\nbevy = \"0.19.1\""));
        assert!(manifest.contains("besfa_editor_plugin = { git ="));
        assert_eq!(main.expect("main.rs should be created"), TEMPLATE_MAIN);
        assert!(!has_cargo_config, "no shared directory means no config");
    }

    #[test]
    fn shares_the_target_dir_and_prebuild_lock() {
        let parent = test_directory("shared");
        let shared = parent.join("Besfa");
        fs::create_dir_all(shared.join("prebuild")).expect("prebuild dir should be created");
        fs::write(shared.join("prebuild").join("Cargo.lock"), "# pinned\n")
            .expect("prebuild lock should be written");
        let destination = parent.join("demo_game");

        let result = create_project_with(&destination, Some(&shared));
        let config = fs::read_to_string(destination.join(".cargo").join("config.toml"));
        let lock = fs::read_to_string(destination.join("Cargo.lock"));
        fs::remove_dir_all(&parent).expect("test directory should be removed");

        result.expect("project creation should succeed");
        let target_dir = shared.join("target").to_string_lossy().replace('\\', "/");
        assert!(
            config
                .expect("config.toml should be created")
                .contains(&format!("target-dir = \"{target_dir}\"")),
        );
        assert_eq!(lock.expect("Cargo.lock should be copied"), "# pinned\n");
    }

    #[test]
    fn rejects_an_existing_destination() {
        let parent = test_directory("existing");
        let destination = parent.join("demo_game");
        fs::create_dir_all(&destination).expect("test destination should be created");

        let error = create_project(&destination).expect_err("existing destination should fail");
        fs::remove_dir_all(&parent).expect("test directory should be removed");

        assert_eq!(error.exit_code, 10);
        assert_eq!(error.code, "destination_exists");
    }

    fn test_directory(label: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "besfa-cli-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after the Unix epoch")
                .as_nanos()
        ))
    }
}
