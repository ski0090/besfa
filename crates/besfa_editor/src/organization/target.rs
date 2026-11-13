//! A target folder of its own for each agent that works in a worktree.
//!
//! Every checkout of a game gives its packages the same build hashes, so
//! two checkouts building in one target folder overwrite each other's game,
//! and cargo can run the other's without compiling its own. Each worktree
//! agent builds in `target` inside its worktree instead, filled first from
//! the project's target folder so it compiles only the repository's own
//! packages, not Bevy again.

use std::{
    collections::BTreeSet,
    env, fs, io,
    path::{Path, PathBuf},
};

/// The target folder cargo builds `project` in: the one the editor's
/// environment names, else `build.target-dir` from the nearest
/// `.cargo/config.toml` above the project or in cargo's home, else
/// `target` in the project.
pub(super) fn project_target(project: &Path) -> PathBuf {
    for name in ["CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR"] {
        if let Some(dir) = env::var_os(name).filter(|dir| !dir.is_empty()) {
            return project.join(dir);
        }
    }
    configured_target(project)
}

/// The target folder cargo's config files set for `project`, else `target`
/// in the project.
fn configured_target(project: &Path) -> PathBuf {
    let home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(|home| Path::new(&home).join(".cargo")));
    let configs = project
        .ancestors()
        .map(|dir| dir.join(".cargo"))
        .chain(home);
    for dot_cargo in configs {
        for file in ["config.toml", "config"] {
            let Some(dir) = fs::read_to_string(dot_cargo.join(file))
                .ok()
                .and_then(|text| target_dir(&text))
            else {
                continue;
            };
            // A relative one is from the folder that holds `.cargo`.
            return dot_cargo.parent().unwrap_or(project).join(dir);
        }
    }
    project.join("target")
}

/// `build.target-dir` in a cargo config file, if it sets one.
fn target_dir(config: &str) -> Option<String> {
    let mut section = String::new();
    for line in config.lines() {
        let line = line.trim();
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            section = name.trim().into();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = match section.as_str() {
            "" => key.trim().to_string(),
            section => format!("{section}.{}", key.trim()),
        };
        if key != "build.target-dir" {
            continue;
        }
        let value = value.trim();
        // A literal string as it is; a basic one with its `\\` unescaped.
        return Some(match value.strip_prefix('\'') {
            Some(literal) => literal.split('\'').next().unwrap_or_default().into(),
            None => value
                .trim_start_matches('"')
                .split('"')
                .next()
                .unwrap_or_default()
                .replace("\\\\", "\\"),
        });
    }
    None
}

/// The names of the packages in the repository at `top` and of their
/// targets, with `-` as `_`. Their builds in the project's target folder
/// are of the project's sources, so none goes into a worktree's.
/// ponytail: by name, so a dependency named like one of them is built again
/// in the worktree, and so is everything above it.
pub(super) fn own_names(top: &Path) -> BTreeSet<String> {
    let listed = super::git(
        top,
        &["ls-files", "--recurse-submodules", "--", "*Cargo.toml"],
    )
    .or_else(|_| super::git(top, &["ls-files", "--", "*Cargo.toml"]))
    .unwrap_or_default();
    listed
        .lines()
        .filter(|path| path == &"Cargo.toml" || path.ends_with("/Cargo.toml"))
        .filter_map(|path| fs::read_to_string(top.join(path)).ok())
        .flat_map(|manifest| {
            manifest
                .lines()
                .filter_map(|line| {
                    let (key, value) = line.split_once('=')?;
                    (key.trim() == "name").then(|| quoted(value).replace('-', "_"))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The string a TOML value starts with, without a comment after it.
fn quoted(value: &str) -> &str {
    let value = value.trim();
    match value.chars().next() {
        Some(quote @ ('"' | '\'')) => value[1..].split(quote).next().unwrap_or_default(),
        _ => value.split('#').next().unwrap_or_default().trim(),
    }
}

/// `name` without the `-<hash>` cargo puts after a package or crate name.
fn without_hash(name: &str) -> &str {
    match name.rsplit_once('-') {
        Some((base, hash)) if hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()) => {
            base
        }
        _ => name,
    }
}

/// Whether `file`, in `deps`, is a build of one of `own`.
fn own_file(file: &str, own: &BTreeSet<String>) -> bool {
    let stem = without_hash(file.split('.').next().unwrap_or(file)).replace('-', "_");
    own.contains(&stem)
        || stem
            .strip_prefix("lib")
            .is_some_and(|name| own.contains(name))
}

/// Fills `to`, the target folder of an agent's worktree, from `from`, the
/// project's, so the worktree's builds reuse what the project built and
/// compile only the repository's own packages, `own`. Libraries are
/// hardlinked, since rustc writes a new file in place of an old one. What
/// cargo, rustc or the linker write over in place, fingerprints, dep-info,
/// DLLs and build-script outputs, is copied, so a rebuild in the worktree
/// never changes the project's folder. Debug info, leftover objects and
/// executables stay behind. A folder that is there already, a stopped
/// run's, is left as it is; with no `from` it is made empty. On another
/// drive than `from` nothing can be linked: the folder is left empty, and
/// the error says so.
pub(super) fn seed(from: Option<&Path>, to: &Path, own: &BTreeSet<String>) -> Result<(), String> {
    if to.exists() {
        return Ok(());
    }
    let Some(from) = from else {
        return ignored(to).map_err(|error| error.to_string());
    };
    // Whole or not at all: a folder cut short would read as filled. Each
    // filling has its own, so one started again meanwhile, from a project
    // reopened say, leaves this one alone; whichever ends first is kept.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let part = to.with_file_name(format!("target.part-{unique}"));
    let filled = ignored(&part).and_then(|()| fill(&from.join("debug"), &part.join("debug"), own));
    let done = filled.and_then(|()| match fs::rename(&part, to) {
        Err(_) if to.exists() => fs::remove_dir_all(&part),
        renamed => renamed,
    });
    done.map_err(|error| {
        let _ = fs::remove_dir_all(&part);
        let _ = ignored(to);
        if error.raw_os_error() == Some(17) {
            // ERROR_NOT_SAME_DEVICE
            format!(
                "{} and {} are on different drives, so nothing the project built could be shared.",
                to.display(),
                from.display()
            )
        } else {
            format!(
                "Could not fill {} from {}: {error}",
                to.display(),
                from.display()
            )
        }
    })
}

/// Makes `dir` with a `.gitignore` that keeps all of it out of git, so the
/// editor's `git add -A` in the worktree never takes it in.
fn ignored(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(".gitignore"), "*\n")
}

fn fill(from: &Path, to: &Path, own: &BTreeSet<String>) -> io::Result<()> {
    for folder in ["deps", "build", ".fingerprint"] {
        fs::create_dir_all(to.join(folder))?;
    }
    let Ok(deps) = fs::read_dir(from.join("deps")) else {
        // Nothing to fill from: the worktree builds everything, and says so.
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the project's target folder has no build yet",
        ));
    };
    for entry in deps.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !entry.path().is_file() || own_file(&name, own) {
            continue;
        }
        let extension = name.rsplit('.').next().unwrap_or_default();
        let target = to.join("deps").join(&*name);
        let made = match extension {
            "rlib" | "rmeta" => fs::hard_link(entry.path(), &target),
            "pdb" | "o" | "exe" => continue,
            _ => fs::copy(entry.path(), &target).map(|_| ()),
        };
        skip_failed(made)?;
    }
    for folder in ["build", ".fingerprint"] {
        for entry in fs::read_dir(from.join(folder))
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if own.contains(&without_hash(&name).replace('-', "_")) {
                continue;
            }
            skip_failed(copy_folder(&entry.path(), &to.join(folder).join(&*name)))?;
        }
    }
    Ok(())
}

/// Copies `from` into `to`, all but debug info.
fn copy_folder(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)?.flatten() {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_folder(&path, &target)?;
        } else if path.extension().is_none_or(|extension| extension != "pdb") {
            skip_failed(fs::copy(&path, &target).map(|_| ()))?;
        }
    }
    Ok(())
}

/// Lets a file that could not be linked or copied go, one another build
/// removed or holds, say: cargo builds that unit again. Only another drive
/// ends the filling, since nothing could be linked from there.
fn skip_failed(made: io::Result<()>) -> io::Result<()> {
    match made {
        // ERROR_NOT_SAME_DEVICE
        Err(error) if error.raw_os_error() == Some(17) => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, process::Command, time::SystemTime};

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("besfa-target-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        super::super::git(dir, args).unwrap()
    }

    #[test]
    fn finds_the_target_folder_cargo_builds_the_project_in() {
        let shared = "[build]\ntarget-dir = \"C:/Shared/target\"\n";
        assert_eq!(target_dir(shared).as_deref(), Some("C:/Shared/target"));
        assert_eq!(
            target_dir("build.target-dir = 'D:\\t'\n").as_deref(),
            Some("D:\\t")
        );
        let escaped = "[build]\ntarget-dir = \"C:\\\\a\\\\b\"";
        assert_eq!(target_dir(escaped).as_deref(), Some("C:\\a\\b"));
        assert_eq!(target_dir("[env]\ntarget-dir = \"x\"\n"), None);
        let root = scratch("config");
        let project = root.join("game");
        fs::create_dir_all(project.join(".cargo")).unwrap();
        // Relative to the folder that holds `.cargo`, here above the game.
        fs::create_dir_all(root.join(".cargo")).unwrap();
        fs::write(
            root.join(".cargo/config.toml"),
            "[build]\ntarget-dir = \"t\"\n",
        )
        .unwrap();
        assert_eq!(configured_target(&project), root.join("t"));
        // The nearest one wins.
        fs::write(project.join(".cargo/config.toml"), shared).unwrap();
        assert_eq!(
            configured_target(&project),
            PathBuf::from("C:/Shared/target")
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tells_the_repositorys_own_builds_from_its_dependencies() {
        // As a manifest writes them, comments and all.
        assert_eq!(quoted(" \"my-game\" # the game"), "my-game");
        assert_eq!(quoted(" 'tool'"), "tool");
        assert_eq!(quoted(" bare # x"), "bare");
        let own: BTreeSet<String> = ["my_game".to_string(), "tool".into()].into();
        for file in [
            "libmy_game-0123456789abcdef.rlib",
            "my_game.exe",
            "my_game-0123456789abcdef.d",
            "tool.pdb",
        ] {
            assert!(own_file(file, &own), "{file}");
        }
        for file in [
            "libitoa-0123456789abcdef.rlib",
            "serde_derive-0123456789abcdef.dll",
            "my_game_extra-0123456789abcdef.d",
        ] {
            assert!(!own_file(file, &own), "{file}");
        }
        assert_eq!(without_hash("proc-macro2-0123456789abcdef"), "proc-macro2");
        assert_eq!(without_hash("my-game"), "my-game");
    }

    /// Writes `files` under `dir`, each holding its own name.
    fn write(dir: &Path, files: &[&str]) {
        for file in files {
            let path = dir.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, file).unwrap();
        }
    }

    #[test]
    fn links_libraries_and_copies_what_builds_write_over() {
        let root = scratch("seed");
        let from = root.join("shared");
        let to = root.join("worktree/target");
        let hash = "-0123456789abcdef";
        let files = [
            "deps/libitoa{h}.rlib",
            "deps/libitoa{h}.rmeta",
            "deps/itoa{h}.d",
            "deps/serde_derive{h}.dll",
            "deps/serde_derive{h}.dll.lib",
            "deps/serde_derive{h}.pdb",
            "deps/leftover.rcgu.o",
            "deps/libmy_game{h}.rlib",
            "deps/my_game.exe",
            ".fingerprint/itoa{h}/lib-itoa",
            ".fingerprint/my-game{h}/bin-my_game",
            "build/rustversion{h}/output",
            "build/rustversion{h}/out/version.expr",
            "build/rustversion{h}/build_script_build.pdb",
            "build/my-game{h}/output",
        ]
        .map(|file| file.replace("{h}", hash));
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        write(&from.join("debug"), &files);
        let own: BTreeSet<String> = ["my_game".to_string()].into();
        seed(Some(&from), &to, &own).unwrap();
        let debug = to.join("debug");
        let shared = |file: &str| fs::read_to_string(from.join("debug").join(file)).unwrap();
        // A write to the worktree's file reaches the project's only where
        // the two are one file.
        for (file, linked) in [
            (files[0], true),
            (files[1], true),
            (files[2], false),
            (files[3], false),
            (files[4], false),
            (files[9], false),
            (files[12], false),
        ] {
            fs::write(debug.join(file), "rebuilt").unwrap();
            assert_eq!(shared(file) == "rebuilt", linked, "{file}");
        }
        // A copy keeps its time, which cargo compares.
        let time = |path: PathBuf| fs::metadata(path).unwrap().modified().unwrap();
        assert_eq!(
            time(debug.join(files[11])),
            time(from.join("debug").join(files[11]))
        );
        // Debug info, leftovers, executables, and the game's own builds.
        let own_fingerprint = format!(".fingerprint/my-game{hash}");
        let own_build = format!("build/my-game{hash}");
        for left in [
            files[5],
            files[6],
            files[7],
            files[8],
            files[13],
            &own_fingerprint,
            &own_build,
        ] {
            assert!(!debug.join(left).exists(), "{left}");
        }
        assert_eq!(fs::read_to_string(to.join(".gitignore")).unwrap(), "*\n");
        // Nothing half-made is left beside it.
        let left: Vec<_> = fs::read_dir(root.join("worktree"))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(left.len(), 1, "{left:?}");
        // A stopped run's folder keeps what its agent built.
        let built = debug.join(format!("deps/new{hash}.d"));
        fs::write(&built, "").unwrap();
        seed(Some(&from), &to, &own).unwrap();
        assert!(built.exists());
        // A project target with no build fills nothing, and says so.
        let bare = root.join("bare/target");
        let why = seed(Some(&root.join("never-built")), &bare, &own).unwrap_err();
        assert!(why.contains("no build"), "{why}");
        assert_eq!(fs::read_to_string(bare.join(".gitignore")).unwrap(), "*\n");
        // With nothing to fill from, an empty folder git still ignores.
        let empty = root.join("none/target");
        seed(None, &empty, &own).unwrap();
        assert_eq!(fs::read_to_string(empty.join(".gitignore")).unwrap(), "*\n");
        assert!(!empty.join("debug").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    /// Every file under `dir`, with its length, time and contents.
    fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (u64, SystemTime, Vec<u8>)> {
        let mut files = BTreeMap::new();
        let mut folders = vec![dir.to_path_buf()];
        while let Some(folder) = folders.pop() {
            for entry in fs::read_dir(&folder).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    folders.push(path);
                } else {
                    let meta = fs::metadata(&path).unwrap();
                    let bytes = fs::read(&path).unwrap();
                    files.insert(path, (meta.len(), meta.modified().unwrap(), bytes));
                }
            }
        }
        files
    }

    /// The crates `cargo build` compiled in `project` with `target`.
    fn compiled(project: &Path, target: &Path) -> Vec<String> {
        let output = Command::new("cargo")
            .args(["build", "--offline"])
            .current_dir(project)
            .env("CARGO_TARGET_DIR", target)
            .env_remove("RUSTUP_TOOLCHAIN")
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{log}");
        log.lines()
            .filter_map(|line| line.trim().strip_prefix("Compiling "))
            .map(|rest| rest.split(' ').next().unwrap().to_string())
            .collect()
    }

    /// Needs cargo with itoa, rustversion and serde with derive in its
    /// registry cache, since it builds offline:
    /// `cargo test -p besfa_editor seeded_worktree -- --ignored`.
    #[test]
    #[ignore]
    fn a_seeded_worktree_builds_only_its_game_and_never_writes_the_project_target() {
        let root = scratch("cargo");
        let project = root.join("game");
        let shared = root.join("shared");
        fs::create_dir_all(project.join("src")).unwrap();
        let manifest = "[package]\nname = \"my-game\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nitoa = \"1\"\nrustversion = \"1\"\nserde = { version = \"1\", features = [\"derive\"] }\n";
        fs::write(project.join("Cargo.toml"), manifest).unwrap();
        let main = |said: &str| {
            format!(
                "#[derive(serde::Serialize)]\nstruct S;\n\n#[rustversion::any(stable, beta, nightly)]\nfn main() {{\n    let _ = S;\n    println!(\"{said} {{}}\", itoa::Buffer::new().format(1));\n}}\n"
            )
        };
        fs::write(project.join("src/main.rs"), main("project")).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "Test"],
            &["config", "user.email", "test@example.com"],
        ] {
            git(&project, args);
        }
        let locked = Command::new("cargo")
            .args(["generate-lockfile", "--offline"])
            .current_dir(&project)
            .status()
            .unwrap();
        assert!(locked.success());
        git(&project, &["add", "-A"]);
        git(&project, &["commit", "-q", "-m", "Start"]);
        compiled(&project, &shared);

        let worktree = root.join("game-wt");
        let at = worktree.to_string_lossy().to_string();
        git(
            &project,
            &["worktree", "add", "-q", "--detach", &at, "HEAD"],
        );
        fs::write(worktree.join("src/main.rs"), main("worktree")).unwrap();
        let target = worktree.join("target");
        seed(Some(&shared), &target, &own_names(&worktree)).unwrap();
        let before = snapshot(&shared.join("debug"));

        assert_eq!(compiled(&worktree, &target), ["my-game"]);
        let ran = Command::new(target.join("debug/my-game.exe"))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&ran.stdout).trim(), "worktree 1");
        // Dependencies the worktree builds again, a library, a proc macro
        // and one with a build script, leave the project's folder alone.
        let rebuild = ["itoa-", "serde_derive-", "rustversion-"];
        for entry in fs::read_dir(target.join("debug/.fingerprint"))
            .unwrap()
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if rebuild.iter().any(|crate_| name.starts_with(crate_)) {
                fs::remove_dir_all(entry.path()).unwrap();
            }
        }
        let again = compiled(&worktree, &target);
        for crate_ in ["itoa", "serde_derive", "rustversion"] {
            assert!(again.iter().any(|built| built == crate_), "{again:?}");
        }
        assert!(
            before == snapshot(&shared.join("debug")),
            "the project's target changed"
        );
        // Git sees none of it.
        assert_eq!(
            git(
                &worktree,
                &["status", "--porcelain", "-uall", "--", "target"]
            ),
            ""
        );
        let _ = super::super::git(&project, &["worktree", "remove", "--force", &at]);
        let _ = fs::remove_dir_all(&root);
    }
}
