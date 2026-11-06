//! Pruning the shared build directory: every Bevy bump and every plugin
//! update leaves the units of the old version behind, gigabytes nobody
//! builds against any more.

use std::{
    collections::HashSet,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

/// The dependency versions still in use, from the Cargo.lock files of the
/// prebuild and of the games: registry packages as their
/// `<name>-<version>` source directory names, git packages as revisions.
#[derive(Default)]
pub(crate) struct Locked {
    registry: HashSet<String>,
    git_revisions: Vec<String>,
}

impl Locked {
    /// Missing locks are skipped: a project in the recent list may be gone.
    pub(crate) fn from_locks<'a>(locks: impl IntoIterator<Item = &'a Path>) -> Self {
        let mut locked = Self::default();
        for lock in locks {
            if let Ok(text) = fs::read_to_string(lock) {
                locked.add(&text);
            }
        }
        locked
    }

    /// Reads the `[[package]]` entries, whose `name`, `version` and
    /// `source` lines come in that order.
    fn add(&mut self, lock: &str) {
        let mut name = "";
        for line in lock.lines().map(str::trim_end) {
            if let Some(value) = line.strip_prefix("name = ") {
                name = value.trim_matches('"');
            } else if let Some(version) = line.strip_prefix("version = ") {
                let version = version.trim_matches('"');
                self.registry.insert(format!("{name}-{version}"));
                // Built for `bevy/dynamic_linking` and never in a lock.
                if name == "bevy" {
                    self.registry.insert(format!("bevy_dylib-{version}"));
                }
            } else if let Some(revision) = line
                .strip_prefix("source = \"git+")
                .and_then(|source| source.split_once('#'))
                .map(|(_, revision)| revision.trim_matches('"'))
            {
                self.git_revisions.push(revision.into());
            }
        }
    }

    /// Whether the dependency whose crate root is `source` is still in
    /// use. Anything outside the registry and the git checkouts (the games
    /// themselves, path dependencies) is.
    fn in_use(&self, source: &str) -> bool {
        let source = source.replace('\\', "/");
        let parts: Vec<&str> = source.split('/').collect();
        for window in parts.windows(4) {
            match window {
                // registry/src/<index>/<name>-<version>/...
                ["registry", "src", _, package] => return self.registry.contains(*package),
                // git/checkouts/<repository>-<hash>/<short revision>/...
                ["git", "checkouts", _, revision] => {
                    return self
                        .git_revisions
                        .iter()
                        .any(|locked| locked.starts_with(revision));
                }
                _ => {}
            }
        }
        true
    }
}

/// What `prune` removed.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Pruned {
    pub(crate) units: usize,
    pub(crate) bytes: u64,
}

/// Deletes every unit under `target/debug` whose dependency version none of
/// `locked` pins: its files in `deps` and its fingerprint, which Cargo
/// names by the same hash. A unit is told from its dep-info file (`.d`),
/// whose first source path is the crate root; units without one are left
/// alone, as are `build` and `incremental` (small next to `deps`).
///
/// Fails with `WouldBlock` while another Cargo holds the build directory.
pub(crate) fn prune(target: &Path, locked: &Locked) -> io::Result<Pruned> {
    let debug = target.join("debug");
    // Cargo's own build directory lock, so no build runs over the deletion
    // and none starts until it is done.
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(debug.join(".cargo-lock"))?;
    lock.try_lock().map_err(|error| match error {
        std::fs::TryLockError::WouldBlock => io::Error::from(io::ErrorKind::WouldBlock),
        std::fs::TryLockError::Error(error) => error,
    })?;

    let mut fingerprints: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(debug.join(".fingerprint"))? {
        fingerprints.push(entry?.path());
    }
    let deps = debug.join("deps");
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&deps)? {
        files.push(entry?.path());
    }

    let mut pruned = Pruned::default();
    for dep_info in &files {
        let Some(stem) = dep_info
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".d"))
        else {
            continue;
        };
        // `<output>: <crate root> <other sources>`
        let Some(source) = fs::read_to_string(dep_info).ok().and_then(|text| {
            text.lines()
                .next()?
                .split_once(": ")
                .map(|(_, s)| s.to_owned())
        }) else {
            continue;
        };
        if locked.in_use(&source) {
            continue;
        }
        // ponytail: units of one version under several hashes (features,
        // profiles) all stay; the lock cannot tell them apart.
        let hash = stem.rsplit('-').next().unwrap_or(stem);
        // The fingerprint goes first: without it Cargo rebuilds the unit
        // even if a file below fails to go.
        for fingerprint in &fingerprints {
            if fingerprint.to_string_lossy().ends_with(&format!("-{hash}")) {
                let bytes = dir_size(fingerprint);
                if fs::remove_dir_all(fingerprint).is_ok() {
                    pruned.bytes += bytes;
                }
            }
        }
        for file in &files {
            let name = file.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with(&format!("{stem}.")) || name.starts_with(&format!("lib{stem}.")) {
                let bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
                // A DLL a running game still maps is left for the next time.
                if fs::remove_file(file).is_ok() {
                    pruned.bytes += bytes;
                }
            }
        }
        pruned.units += 1;
    }
    Ok(pruned)
}

fn dir_size(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: &str = "[[package]]\nname = \"bevy\"\nversion = \"0.20.0\"\n\
                        source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n\
                        [[package]]\nname = \"besfa_editor_plugin\"\nversion = \"0.1.0\"\n\
                        source = \"git+https://github.com/ski0090/besfa#c981c78b2f182b154865d41d09269062dbc8ba1b\"\n\n\
                        [[package]]\nname = \"xml-rs\"\nversion = \"0.8.29\"\n\
                        source = \"registry+https://github.com/rust-lang/crates.io-index\"\n";

    fn unit(debug: &Path, package: &str, lib: &str, hash: &str, root: &str) {
        let deps = debug.join("deps");
        fs::write(
            deps.join(format!("{lib}-{hash}.d")),
            format!("{}\\deps\\{lib}-{hash}.d: {root} {root}\n", debug.display()),
        )
        .unwrap();
        fs::write(deps.join(format!("lib{lib}-{hash}.rlib")), "rlib").unwrap();
        fs::write(deps.join(format!("{lib}-{hash}.dll")), "dll").unwrap();
        let fingerprint = debug.join(".fingerprint").join(format!("{package}-{hash}"));
        fs::create_dir_all(&fingerprint).unwrap();
        fs::write(fingerprint.join(format!("lib-{lib}.json")), "{}").unwrap();
    }

    #[test]
    fn removes_units_of_versions_no_lock_pins() {
        let dir = std::env::temp_dir().join(format!("besfa-cli-prune-{}", std::process::id()));
        let debug = dir.join("target/debug");
        fs::create_dir_all(debug.join("deps")).unwrap();
        fs::create_dir_all(debug.join(".fingerprint")).unwrap();
        let registry = r"C:\Users\me\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f";
        let git = r"C:\Users\me\.cargo\git\checkouts\besfa-06b8ef651c685e8b";
        // The lib name is not the package name: keyed on the path, not the stem.
        unit(
            &debug,
            "xml-rs",
            "xml",
            "1111111111111111",
            &format!(r"{registry}\xml-rs-0.8.29\src\lib.rs"),
        );
        unit(
            &debug,
            "xml-rs",
            "xml",
            "2222222222222222",
            &format!(r"{registry}\xml-rs-0.8.0\src\lib.rs"),
        );
        unit(
            &debug,
            "bevy_dylib",
            "bevy_dylib",
            "3333333333333333",
            &format!(r"{registry}\bevy_dylib-0.20.0\src\lib.rs"),
        );
        unit(
            &debug,
            "bevy_dylib",
            "bevy_dylib",
            "4444444444444444",
            &format!(r"{registry}\bevy_dylib-0.19.1\src\lib.rs"),
        );
        unit(
            &debug,
            "besfa_editor_plugin",
            "besfa_editor_plugin",
            "5555555555555555",
            &format!(r"{git}\c981c78\crates\besfa_editor_plugin\src\lib.rs"),
        );
        unit(
            &debug,
            "besfa_editor_plugin",
            "besfa_editor_plugin",
            "6666666666666666",
            &format!(r"{git}\0c918f8\crates\besfa_editor_plugin\src\lib.rs"),
        );
        unit(
            &debug,
            "my_game",
            "my_game",
            "7777777777777777",
            r"C:\games\my game\src\main.rs",
        );
        fs::write(debug.join("deps/std-8888888888888888.dll"), "no dep-info").unwrap();

        let lock = dir.join("Cargo.lock");
        fs::write(&lock, LOCK).unwrap();
        let locked = Locked::from_locks([lock.as_path(), Path::new("missing/Cargo.lock")]);
        let pruned = prune(&dir.join("target"), &locked).unwrap();
        assert_eq!(pruned.units, 3);
        // 3 units × (dep-info 4 + rlib 4 + dll 3 + fingerprint 2 bytes), dep-info lengths vary.
        assert!(pruned.bytes > 27, "{pruned:?}");

        let mut names: Vec<String> = fs::read_dir(debug.join("deps"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let kept = |hash: &str| names.iter().filter(|n| n.contains(hash)).count();
        assert_eq!(kept("1111111111111111"), 3, "{names:?}");
        assert_eq!(kept("2222222222222222"), 0, "{names:?}");
        assert_eq!(kept("3333333333333333"), 3);
        assert_eq!(kept("4444444444444444"), 0);
        assert_eq!(kept("5555555555555555"), 3);
        assert_eq!(kept("6666666666666666"), 0);
        assert_eq!(kept("7777777777777777"), 3);
        assert_eq!(kept("8888888888888888"), 1);
        let fingerprints = fs::read_dir(debug.join(".fingerprint")).unwrap().count();
        assert_eq!(fingerprints, 4);

        // Another Cargo in the build directory: nothing is touched.
        let held = File::open(debug.join(".cargo-lock")).unwrap();
        held.lock().unwrap();
        let error = prune(&dir.join("target"), &locked).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        drop(held);
        fs::remove_dir_all(dir).unwrap();
    }
}
