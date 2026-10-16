//! Game projects: the recent list, creating, opening, revealing and deleting
//! them, and the `besfa_editor_plugin` revision they pin.

use std::{
    ffi::OsStr,
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
};

use crate::process::command;

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub path: String,
}

impl Project {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    pub fn name(&self) -> &str {
        Path::new(&self.path)
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or(&self.path)
    }

    pub fn file(&self, name: &str) -> PathBuf {
        Path::new(&self.path).join(name)
    }

    /// Whether it is (still) a Cargo project.
    // ponytail: Cargo.toml check only, switch to the .besfa marker once
    // DATA_FORMAT.md is settled.
    pub fn exists(&self) -> bool {
        self.file("Cargo.toml").is_file()
    }
}

/// Recently opened projects, newest first, stored as a JSON list of paths.
pub struct RecentProjects {
    file: PathBuf,
}

impl RecentProjects {
    const LIMIT: usize = 10;

    pub fn standard() -> Self {
        let base = std::env::var("APPDATA").or_else(|_| std::env::var("HOME"));
        Self {
            file: Path::new(&base.unwrap_or_else(|_| ".".into()))
                .join("Besfa/recent_projects.json"),
        }
    }

    pub fn load(&self) -> Vec<Project> {
        fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
            .unwrap_or_default()
            .into_iter()
            .map(Project::new)
            .collect()
    }

    /// Moves `project` to the top of the list and returns the new list.
    pub fn add(&self, project: &Project) -> Vec<Project> {
        let mut projects = vec![project.clone()];
        projects.extend(self.without(project));
        projects.truncate(Self::LIMIT);
        self.save(projects)
    }

    pub fn remove(&self, project: &Project) -> Vec<Project> {
        self.save(self.without(project))
    }

    // Windows paths are case-insensitive.
    fn without(&self, project: &Project) -> Vec<Project> {
        let mut projects = self.load();
        projects.retain(|other| !other.path.eq_ignore_ascii_case(&project.path));
        projects
    }

    fn save(&self, projects: Vec<Project>) -> Vec<Project> {
        let paths: Vec<&str> = projects
            .iter()
            .map(|project| project.path.as_str())
            .collect();
        // The list is a convenience; failing to save must not block opening.
        if let Some(parent) = self.file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&self.file, serde_json::to_string(&paths).unwrap());
        projects
    }
}

/// Creates a project in `directory` with `besfa new` and returns its path,
/// or what went wrong.
pub fn create(directory: &str) -> Result<String, String> {
    let output = command("besfa")
        .args(["new", directory, "--output", "json"])
        .output()
        .map_err(|error| format!("Could not start the Besfa CLI.\n{error}"))?;
    creation_result(
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
    )
}

fn creation_result(success: bool, stdout: &str, stderr: &str) -> Result<String, String> {
    let Ok(serde_json::Value::Object(output)) = serde_json::from_str(stdout) else {
        return Err("The Besfa CLI returned an invalid response.".into());
    };
    if success
        && output.get("status").and_then(|s| s.as_str()) == Some("success")
        && let Some(path) = output.get("project_path").and_then(|p| p.as_str())
    {
        return Ok(path.into());
    }
    let message = output.get("message").and_then(|m| m.as_str());
    let message = message.unwrap_or("Failed to create the project.");
    Err(match stderr.trim() {
        "" => message.into(),
        diagnostic => format!("{message}\n{diagnostic}"),
    })
}

/// Moves `directory` to the Recycle Bin. Returns false if it failed.
pub fn move_to_recycle_bin(directory: &str) -> bool {
    // The path goes through an env var so quotes in it cannot break the script.
    command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$ErrorActionPreference = 'Stop'; \
             Add-Type -AssemblyName Microsoft.VisualBasic; \
             [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory(\
             $env:BESFA_DELETE_PATH, 'OnlyErrorDialogs', 'SendToRecycleBin')",
        ])
        .env("BESFA_DELETE_PATH", directory)
        .status()
        .is_ok_and(|status| status.success())
}

/// Opens `directory` in Explorer.
pub fn reveal(directory: &str) {
    // Explorer needs backslashes; with '/' it silently opens Documents.
    open_url(&directory.replace('/', "\\"));
}

/// The community's plugins, assets, templates and tools for Bevy, which is
/// what Bevy has for Unity's Asset Store.
pub const BEVY_ASSETS: &str = "https://bevy.org/assets/";

/// Hands `target`, a URL or a folder, to Explorer, which opens the default
/// browser or a window, outside the editor's job so it stays open when the
/// editor exits. Launched from a job that allows no breakaway, as under
/// `cargo run`, it stays in the job.
pub fn open_url(target: &str) {
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let mut command = std::process::Command::new("explorer");
    command.arg(target);
    if command
        .creation_flags(CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
        .is_err()
    {
        let _ = command.creation_flags(0).spawn();
    }
}

/// Asks for a folder with the system dialog; None if cancelled.
pub fn pick_folder(ok_label: &str) -> Option<String> {
    use windows::{
        Win32::{
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree,
            },
            UI::Shell::{FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH},
        },
        core::HSTRING,
    };
    // SAFETY: plain COM calls on the UI thread; the returned string is freed
    // once copied.
    unsafe {
        // Usually a no-op: winit has initialized OLE on this thread already.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        dialog
            .SetOptions(dialog.GetOptions().ok()? | FOS_PICKFOLDERS)
            .ok()?;
        dialog.SetOkButtonLabel(&HSTRING::from(ok_label)).ok()?;
        // Fails when cancelled.
        dialog.Show(None).ok()?;
        let name = dialog
            .GetResult()
            .ok()?
            .GetDisplayName(SIGDN_FILESYSPATH)
            .ok()?;
        let path = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as _));
        path
    }
}

/// The git revision a Cargo.lock pins `besfa_editor_plugin` to, or None when
/// there is no lock yet or the plugin is a path dependency.
pub fn plugin_revision(lock: &Path) -> Option<String> {
    lock_revision(&fs::read_to_string(lock).ok()?)
}

/// The revision in a lock entry for the plugin as a git dependency:
///
/// ```toml
/// [[package]]
/// name = "besfa_editor_plugin"
/// version = "0.1.0"
/// source = "git+https://github.com/ski0090/besfa#<revision>"
/// ```
fn lock_revision(lock: &str) -> Option<String> {
    let mut lines = lock
        .lines()
        .skip_while(|line| *line != r#"name = "besfa_editor_plugin""#);
    lines.next()?;
    lines.next().filter(|line| line.starts_with("version = "))?;
    let source = lines.next()?.strip_prefix(r#"source = "git+"#)?;
    let revision: String = source
        .split_once('#')?
        .1
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .collect();
    (!revision.is_empty()).then_some(revision)
}

/// The revision the shared Bevy prebuild was built with. `besfa prebuild`
/// updates it when the editor starts, so it is the latest the editor knows.
pub fn prebuild_plugin_revision() -> Option<String> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    plugin_revision(&Path::new(&local).join("Besfa/prebuild/Cargo.lock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_a_project_after_its_folder() {
        assert_eq!(Project::new(r"C:\games\my_game\").name(), "my_game");
        assert_eq!(Project::new("C:/games/other").name(), "other");
    }

    #[test]
    fn keeps_newest_first_dedupes_ignoring_case_and_caps_the_list() {
        let dir = std::env::temp_dir().join(format!("besfa_recent_{}", std::process::id()));
        let recent = RecentProjects {
            file: dir.join("nested/recent.json"),
        };
        let paths = || {
            recent
                .load()
                .into_iter()
                .map(|p| p.path)
                .collect::<Vec<_>>()
        };
        assert!(recent.load().is_empty());
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(&recent.file, "{not json").unwrap();
        assert!(recent.load().is_empty(), "survives a corrupt file");

        recent.add(&Project::new(r"C:\a"));
        recent.add(&Project::new(r"C:\b"));
        recent.add(&Project::new(r"c:\A"));
        assert_eq!(paths(), [r"c:\A", r"C:\b"]);
        for i in 0..RecentProjects::LIMIT + 5 {
            recent.add(&Project::new(format!(r"C:\p{i}")));
        }
        assert_eq!(paths().len(), RecentProjects::LIMIT);
        assert_eq!(paths()[0], format!(r"C:\p{}", RecentProjects::LIMIT + 4));
        recent.remove(&Project::new(paths()[0].clone()));
        assert_eq!(paths().len(), RecentProjects::LIMIT - 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_the_cli_result() {
        assert_eq!(
            creation_result(true, r#"{"status":"success","project_path":"C:/g"}"#, ""),
            Ok("C:/g".into())
        );
        assert_eq!(
            creation_result(
                false,
                r#"{"status":"error","message":"Exists."}"#,
                "detail\n"
            ),
            Err("Exists.\ndetail".into())
        );
        assert_eq!(
            creation_result(true, "oops", ""),
            Err("The Besfa CLI returned an invalid response.".into())
        );
    }

    #[test]
    fn reads_the_git_revision_the_lock_pins_the_plugin_to() {
        let lock = "[[package]]\r\nname = \"bevy\"\r\nversion = \"0.19.1\"\r\n\
                    source = \"registry+https://github.com/rust-lang/crates.io-index\"\r\n\r\n\
                    [[package]]\r\nname = \"besfa_editor_plugin\"\r\nversion = \"0.1.0\"\r\n\
                    source = \"git+https://github.com/ski0090/besfa#28b39927dda69b76\"\r\n\
                    dependencies = [\r\n";
        assert_eq!(lock_revision(lock).as_deref(), Some("28b39927dda69b76"));
        let path_dependency = "[[package]]\nname = \"besfa_editor_plugin\"\nversion = \"0.1.0\"\n\
                               dependencies = [\n";
        assert_eq!(lock_revision(path_dependency), None);
        assert_eq!(plugin_revision(Path::new("missing/Cargo.lock")), None);
    }
}
