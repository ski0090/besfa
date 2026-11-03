use std::{
    env, fs,
    io::{self, Write},
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

/// `ccgs` also adds Claude Code Game Studios, see [`add_ccgs`].
pub(crate) fn create_project(directory: &Path, ccgs: bool) -> Result<PathBuf, CreateError> {
    create_project_with(directory, shared_dir().as_deref(), ccgs)
}

/// Builds Bevy into the shared target directory the way the editor's Run
/// does, so the first run of each game only compiles the game itself.
pub(crate) fn prebuild() -> Result<(), CreateError> {
    let shared = shared_dir()
        .ok_or_else(|| CreateError::new(20, "filesystem_error", "LOCALAPPDATA is not set."))?;
    let project = shared.join("prebuild");
    write_prebuild_project(&project, &shared)?;

    // New games copy this lock, so this keeps them on the latest editor
    // plugin. Offline, the pinned version still builds. Cargo holds the
    // package cache lock while it fetches, which would stall a Run in the
    // editor, so a bad network is given up on quickly instead of retried.
    let updated = Command::new("cargo")
        .args(["update", "-p", "besfa_editor_plugin"])
        .args(["--config", "net.retry=0", "--config", "http.timeout=10"])
        .current_dir(&project)
        .status();
    if !updated.is_ok_and(|status| status.success()) {
        eprintln!("Could not update besfa_editor_plugin, building the pinned version.");
    }

    // Same features as the editor's Run of a new game, or the build is not
    // reused.
    let status = Command::new("cargo")
        .args(["build", "--features", "bevy/dynamic_linking,besfa_editor"])
        .current_dir(&project)
        .status()
        .map_err(cargo_start_error)?;
    if !status.success() {
        return Err(CreateError::new(
            30,
            "prebuild_failed",
            "Cargo failed to build the prebuild project.",
        ));
    }
    Ok(())
}

/// Cargo's manifest header for the prebuild game; the template dependencies
/// follow it, as in a new game.
const PREBUILD_MANIFEST: &str = "[package]\n\
                                 name = \"prebuild\"\n\
                                 version = \"0.1.0\"\n\
                                 edition = \"2024\"\n\
                                 \n\
                                 [dependencies]";

/// A game made of the template files, so it pulls in exactly the
/// dependencies, features and profile a new game does. Files are rewritten
/// only when the template changed, so an unchanged prebuild is not recompiled.
fn write_prebuild_project(project: &Path, shared: &Path) -> Result<(), CreateError> {
    let manifest = format!("{PREBUILD_MANIFEST}\n{TEMPLATE_DEPENDENCIES}");
    write_if_changed(&project.join("Cargo.toml"), &manifest)?;
    write_if_changed(&project.join("src").join("main.rs"), TEMPLATE_MAIN)?;
    write_if_changed(
        &project.join(".cargo").join("config.toml"),
        &cargo_config(&shared.join("target")),
    )
}

fn write_if_changed(path: &Path, contents: &str) -> Result<(), CreateError> {
    if fs::read_to_string(path).is_ok_and(|current| current == contents) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            filesystem_error(format!("Failed to create {}.", parent.display()), error)
        })?;
    }
    fs::write(path, contents)
        .map_err(|error| filesystem_error(format!("Failed to write {}.", path.display()), error))
}

fn filesystem_error(message: impl Into<String>, error: io::Error) -> CreateError {
    CreateError::new(20, "filesystem_error", message).with_diagnostic(error.to_string())
}

fn cargo_start_error(error: io::Error) -> CreateError {
    CreateError::new(30, "cargo_execution_failed", "Failed to start Cargo.")
        .with_diagnostic(error.to_string())
}

/// Machine-wide Besfa data: `%LOCALAPPDATA%\Besfa`.
fn shared_dir() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("Besfa"))
}

fn create_project_with(
    directory: &Path,
    shared: Option<&Path>,
    ccgs: bool,
) -> Result<PathBuf, CreateError> {
    let destination = std::path::absolute(directory).map_err(|error| {
        filesystem_error("Failed to determine the current working directory.", error)
    })?;

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
        .map_err(cargo_start_error)?;

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
    if ccgs && let Err(error) = add_ccgs(&staged_project, CCGS_REPOSITORY, CCGS_COMMIT) {
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

/// The cube, light and camera scene; `{{crate}}` becomes the game's crate.
const TEMPLATE_SCENE: &str = include_str!("../template/main.scn.ron");

/// Where the game and the editor read and write the scene.
const SCENE_FILE: &str = "assets/scenes/main.scn.ron";

/// Lets an agent like Claude Code in the game's folder run the game through
/// `besfa mcp`.
const MCP_CONFIG: &str = r#"{
  "mcpServers": {
    "besfa": {
      "command": "besfa",
      "args": ["mcp"]
    }
  }
}
"#;

/// Appended to the `[dependencies]` table that ends Cargo's generated manifest.
const TEMPLATE_DEPENDENCIES: &str = r#"bevy = "0.20.0"
# The scene file, terrain, sky and the rest of what the game ships with.
besfa_components = { git = "https://github.com/ski0090/besfa" }
# The Besfa editor's side, only while it runs the game: release builds
# leave it out.
besfa_editor_plugin = { git = "https://github.com/ski0090/besfa", optional = true }

[features]
besfa_editor = ["dep:besfa_editor_plugin"]

# Bevy is slow unoptimized: optimize dependencies, keep the game quick to rebuild.
[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3
"#;

/// Turns Cargo's hello-world project into a Bevy game that loads a cube
/// scene from `assets/scenes/main.scn.ron`.
///
/// With a machine-wide `shared` directory, the game also builds into
/// `<shared>/target` and starts from `<shared>/prebuild/Cargo.lock`, so every
/// Besfa game reuses one Bevy build instead of compiling its own.
fn write_bevy_template(project: &Path, shared: Option<&Path>) -> Result<(), CreateError> {
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
    fs::write(project.join(".mcp.json"), MCP_CONFIG)
        .map_err(|error| filesystem_error("Failed to write .mcp.json.", error))?;

    // The scene names the game's own components by crate: `my_game::Spin`.
    let crate_name = project
        .file_name()
        .map(|name| name.to_string_lossy().replace('-', "_"))
        .unwrap_or_default();
    let scene_file = project.join(SCENE_FILE);
    fs::create_dir_all(scene_file.parent().expect("the scene file has a directory"))
        .map_err(|error| filesystem_error("Failed to create assets/scenes.", error))?;
    fs::write(
        &scene_file,
        TEMPLATE_SCENE.replace("{{crate}}", &crate_name),
    )
    .map_err(|error| filesystem_error(format!("Failed to write {SCENE_FILE}."), error))?;

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

/// Claude Code Game Studios: agents, skills and hooks for making the game
/// with Claude Code, at the commit new games get.
const CCGS_REPOSITORY: &str = "https://github.com/ski0090/Claude-Code-Game-Studios.git";
// Bumping it: re-check `CCGS_PATHS`, `CCGS_GITIGNORE` and the CLAUDE.md
// import `add_ccgs` rewrites against that commit.
const CCGS_COMMIT: &str = "3a41fdc023077dceff0b05a5187ded7d38018b5a";

/// What a game takes from it: the framework, and of the engine references
/// only Bevy's. A trailing `/` marks a directory.
const CCGS_PATHS: &[&str] = &[
    ".claude/",
    ".gitattributes",
    ".worktreeinclude",
    "CLAUDE.md",
    "project.yaml",
    "design/",
    "production/",
    "src/CLAUDE.md",
    "docs/CLAUDE.md",
    "docs/COLLABORATIVE-DESIGN-PRINCIPLE.md",
    "docs/WORKFLOW-GUIDE.md",
    "docs/architecture/",
    "docs/registry/",
    "docs/engine-reference/README.md",
    "docs/engine-reference/bevy/",
];

/// Its per-developer and per-session files, kept out of the game's commits.
const CCGS_GITIGNORE: &str = "\n\
# Claude Code Game Studios (.claude/CCGS-LICENSE): per-developer config,\n\
# per-session state and agent worktrees.\n\
.claude/settings.local.json\n\
CLAUDE.local.md\n\
project.local.yaml\n\
production/session-logs/\n\
production/session-state/\n\
.scratch/\n\
.claude/agent-memory/\n\
.claude/worktrees/\n\
docs/consistency-failures.md\n";

/// Copies Claude Code Game Studios at `revision` of `repository` into
/// `project`, pointed at Bevy, with its MIT notice in `.claude/CCGS-LICENSE`.
fn add_ccgs(project: &Path, repository: &str, revision: &str) -> Result<(), CreateError> {
    // A throwaway repository beside the project, so only the files land in it.
    let git_dir = project.with_file_name(".ccgs.git");
    // Run from the editor, a login prompt or a stalled download would hang
    // it with no way to cancel, so both fail instead. Its paths run to 88
    // characters, which can pass Windows' 260 under a deep staging directory.
    let git = || {
        let mut command = Command::new("git");
        command.env("GIT_TERMINAL_PROMPT", "0").args([
            "-c",
            "credential.helper=",
            "-c",
            "http.lowSpeedLimit=1000",
            "-c",
            "http.lowSpeedTime=30",
            "-c",
            "core.longpaths=true",
        ]);
        command.arg("--git-dir").arg(&git_dir);
        command
    };
    run_git(git().args(["init", "--bare", "--quiet"]))?;
    run_git(git().args(["fetch", "--quiet", "--depth", "1", repository, revision]))?;
    // As committed: its shell scripts must stay LF under core.autocrlf.
    run_git(
        git()
            .args(["-c", "core.autocrlf=false", "--work-tree"])
            .arg(project)
            .args(["checkout", "FETCH_HEAD", "--"])
            .args(CCGS_PATHS),
    )?;
    let license = run_git(git().args(["show", "FETCH_HEAD:LICENSE"]))?;
    fs::remove_dir_all(&git_dir)
        .map_err(|error| filesystem_error("Failed to remove the CCGS download.", error))?;

    fs::write(project.join(".claude").join("CCGS-LICENSE"), license)
        .map_err(|error| filesystem_error("Failed to write .claude/CCGS-LICENSE.", error))?;
    // What /setup-engine would point it at; Godot's reference is not copied.
    let claude_md = project.join("CLAUDE.md");
    let instructions = fs::read_to_string(&claude_md)
        .map_err(|error| filesystem_error("Failed to read CLAUDE.md.", error))?;
    let godot = "@docs/engine-reference/godot/VERSION.md";
    if !instructions.contains(godot) {
        return Err(CreateError::new(
            30,
            "ccgs_failed",
            "CCGS's CLAUDE.md no longer imports Godot's engine reference.",
        ));
    }
    fs::write(
        &claude_md,
        instructions.replace(godot, "@docs/engine-reference/bevy/VERSION.md"),
    )
    .map_err(|error| filesystem_error("Failed to write CLAUDE.md.", error))?;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(project.join(".gitignore"))
        .and_then(|mut gitignore| gitignore.write_all(CCGS_GITIGNORE.as_bytes()))
        .map_err(|error| filesystem_error("Failed to write .gitignore.", error))
}

/// Runs `command` and returns its stdout.
fn run_git(command: &mut Command) -> Result<Vec<u8>, CreateError> {
    let error = |message: &str| CreateError::new(30, "ccgs_failed", message);
    let output = command
        .output()
        .map_err(|e| error("Failed to start Git.").with_diagnostic(e.to_string()))?;
    if !output.status.success() {
        return Err(error("Git could not get Claude Code Game Studios.")
            .with_diagnostic(String::from_utf8_lossy(&output.stderr).into_owned()));
    }
    Ok(output.stdout)
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

        let result = create_project_with(&destination, None, false);
        let manifest = fs::read_to_string(destination.join("Cargo.toml"));
        let has_cargo_config = destination.join(".cargo").exists();
        let main = fs::read_to_string(destination.join("src").join("main.rs"));
        let scene = fs::read_to_string(destination.join(SCENE_FILE));
        let mcp = fs::read_to_string(destination.join(".mcp.json"));
        fs::remove_dir_all(&parent).expect("test directory should be removed");

        assert_eq!(
            result.expect("project creation should succeed"),
            destination
        );
        let manifest = manifest.expect("Cargo.toml should be created");
        assert!(manifest.contains("name = \"demo_game\""));
        assert!(manifest.contains("[dependencies]\nbevy = \"0.20.0\""));
        assert!(manifest.contains("besfa_components = { git ="));
        assert!(manifest.contains("besfa_editor_plugin = { git ="));
        assert_eq!(
            besfa_mcp::run_features(&manifest),
            "bevy/dynamic_linking,besfa_editor",
            "the editor runs a new game with its plugin"
        );
        assert_eq!(main.expect("main.rs should be created"), TEMPLATE_MAIN);
        let scene = scene.expect("the scene file should be created");
        assert!(scene.contains("\"demo_game::Spin\": ()"), "{scene}");
        assert!(!scene.contains("{{crate}}"));
        let mcp: serde_json::Value =
            serde_json::from_str(&mcp.expect(".mcp.json should be created")).unwrap();
        assert_eq!(mcp["mcpServers"]["besfa"]["args"][0], "mcp");
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

        let result = create_project_with(&destination, Some(&shared), false);
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
    fn prebuild_project_is_the_template_and_is_written_once() {
        let shared = test_directory("prebuild");
        let project = shared.join("prebuild");
        let files = ["Cargo.toml", "src/main.rs", ".cargo/config.toml"];
        // Left behind by an older template; the lock must survive the rewrite.
        fs::create_dir_all(project.join("src")).expect("prebuild dir should be created");
        fs::write(project.join("Cargo.toml"), "[package]\n").expect("manifest should be written");
        fs::write(project.join("src/main.rs"), "fn main() {}\n").expect("main should be written");
        fs::write(project.join("Cargo.lock"), "# pinned\n").expect("lock should be written");
        let modified = || {
            files.map(|file| {
                fs::metadata(project.join(file))
                    .and_then(|meta| meta.modified())
                    .ok()
            })
        };

        let first = write_prebuild_project(&project, &shared);
        let written = modified();
        let second = write_prebuild_project(&project, &shared);
        let rewritten = modified();
        let [manifest, main, config] = files.map(|file| fs::read_to_string(project.join(file)));
        let lock = fs::read_to_string(project.join("Cargo.lock"));
        fs::remove_dir_all(&shared).expect("test directory should be removed");

        first.expect("the prebuild project should be written");
        second.expect("an up-to-date prebuild project should be accepted");
        assert_eq!(
            manifest.expect("Cargo.toml should exist"),
            format!("{PREBUILD_MANIFEST}\n{TEMPLATE_DEPENDENCIES}")
        );
        assert_eq!(main.expect("main.rs should exist"), TEMPLATE_MAIN);
        assert_eq!(
            config.expect("config.toml should exist"),
            cargo_config(&shared.join("target"))
        );
        assert_eq!(lock.expect("Cargo.lock should be kept"), "# pinned\n");
        assert_eq!(written, rewritten, "an up-to-date prebuild is left alone");
    }

    #[test]
    fn rejects_an_existing_destination() {
        let parent = test_directory("existing");
        let destination = parent.join("demo_game");
        fs::create_dir_all(&destination).expect("test destination should be created");

        let error =
            create_project(&destination, false).expect_err("existing destination should fail");
        fs::remove_dir_all(&parent).expect("test directory should be removed");

        assert_eq!(error.exit_code, 10);
        assert_eq!(error.code, "destination_exists");
    }

    #[test]
    fn adds_claude_code_game_studios_set_up_for_bevy() {
        let parent = test_directory("ccgs");
        let (source, project) = (parent.join("ccgs"), parent.join("demo_game"));
        // A stand-in for its repository: one LF file at each copied path.
        for path in CCGS_PATHS {
            let file = match path.strip_suffix('/') {
                Some(directory) => source.join(directory).join("hook.sh"),
                None => source.join(path),
            };
            fs::create_dir_all(file.parent().unwrap()).expect("source dir should be created");
            fs::write(&file, "line\nline\n").expect("source file should be written");
        }
        let claude_md = "@docs/engine-reference/godot/VERSION.md\n";
        fs::write(source.join("CLAUDE.md"), claude_md).expect("CLAUDE.md should be written");
        fs::write(source.join("LICENSE"), "MIT License\n").expect("LICENSE should be written");
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .arg("-C")
                .arg(&source)
                .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
                .args(["-c", "core.autocrlf=false"])
                .args(args)
                .status();
            assert!(
                status.is_ok_and(|s| s.success()),
                "git {args:?} should succeed"
            );
        };
        git(&["init", "--quiet"]);
        git(&["add", "."]);
        git(&["commit", "--quiet", "-m", "ccgs"]);
        // Its download repository, which `add_ccgs` reuses, set to check out
        // CRLF whatever this machine's Git does.
        git(&["init", "--bare", "--quiet", "../.ccgs.git"]);
        git(&["-C", "../.ccgs.git", "config", "core.autocrlf", "true"]);
        fs::create_dir_all(&project).expect("project should be created");
        fs::write(project.join(".gitignore"), "/target\n").expect(".gitignore should be written");

        let result = add_ccgs(&project, &source.to_string_lossy(), "HEAD");
        let read = |path: &str| fs::read_to_string(project.join(path));
        let claude_md = read("CLAUDE.md");
        let (hook, license, gitignore) = (
            read(".claude/hook.sh"),
            read(".claude/CCGS-LICENSE"),
            read(".gitignore"),
        );
        let mut leftovers: Vec<_> = fs::read_dir(&parent)
            .expect("test directory should be listed")
            .map(|entry| entry.expect("entry should be read").file_name())
            .collect();
        leftovers.sort();
        fs::remove_dir_all(&parent).expect("test directory should be removed");

        result.expect("CCGS should be added");
        assert_eq!(
            claude_md.expect("CLAUDE.md should be copied"),
            "@docs/engine-reference/bevy/VERSION.md\n"
        );
        assert_eq!(
            hook.expect("files should be copied"),
            "line\nline\n",
            "LF kept"
        );
        assert_eq!(
            license.expect("the license should be kept"),
            "MIT License\n"
        );
        let gitignore = gitignore.expect(".gitignore should be kept");
        assert!(gitignore.starts_with("/target\n"), "{gitignore}");
        assert!(gitignore.contains("\nproject.local.yaml\n"), "{gitignore}");
        assert_eq!(leftovers, ["ccgs", "demo_game"], "the download is removed");
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
