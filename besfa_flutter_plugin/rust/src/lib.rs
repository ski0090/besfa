use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
};

const RUNTIME_COMMAND_OK: i32 = 0;
const RUNTIME_COMMAND_ALREADY_RUNNING: i32 = 1;
const RUNTIME_COMMAND_NOT_RUNNING: i32 = 2;
const RUNTIME_COMMAND_FAILED: i32 = -1;

static RUNTIME_PROCESS: Mutex<Option<Child>> = Mutex::new(None);

#[unsafe(no_mangle)]
pub extern "C" fn besfa_flutter_plugin_abi_version() -> u32 {
    besfa_core::ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn besfa_flutter_plugin_add(left: i32, right: i32) -> i32 {
    left + right
}

#[unsafe(no_mangle)]
pub extern "C" fn besfa_runtime_start() -> i32 {
    let mut runtime_process = match RUNTIME_PROCESS.lock() {
        Ok(runtime_process) => runtime_process,
        Err(_) => return RUNTIME_COMMAND_FAILED,
    };

    if let Some(child) = runtime_process.as_mut() {
        match child.try_wait() {
            Ok(Some(_)) => {
                *runtime_process = None;
            }
            Ok(None) => return RUNTIME_COMMAND_ALREADY_RUNNING,
            Err(_) => return RUNTIME_COMMAND_FAILED,
        }
    }

    let Some(runtime_path) = find_runtime_executable() else {
        return RUNTIME_COMMAND_FAILED;
    };

    match Command::new(runtime_path)
        .current_dir(workspace_root())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            *runtime_process = Some(child);
            RUNTIME_COMMAND_OK
        }
        Err(_) => RUNTIME_COMMAND_FAILED,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn besfa_runtime_stop() -> i32 {
    let mut runtime_process = match RUNTIME_PROCESS.lock() {
        Ok(runtime_process) => runtime_process,
        Err(_) => return RUNTIME_COMMAND_FAILED,
    };

    let Some(child) = runtime_process.as_mut() else {
        return RUNTIME_COMMAND_NOT_RUNNING;
    };

    match child.try_wait() {
        Ok(Some(_)) => {
            *runtime_process = None;
            RUNTIME_COMMAND_NOT_RUNNING
        }
        Ok(None) => match child.kill().and_then(|_| child.wait()) {
            Ok(_) => {
                *runtime_process = None;
                RUNTIME_COMMAND_OK
            }
            Err(_) => RUNTIME_COMMAND_FAILED,
        },
        Err(_) => RUNTIME_COMMAND_FAILED,
    }
}

fn find_runtime_executable() -> Option<PathBuf> {
    if let Ok(runtime_path) = std::env::var("BESFA_RUNTIME_PATH") {
        let runtime_path = PathBuf::from(runtime_path);
        if runtime_path.is_file() {
            return Some(runtime_path);
        }
    }

    runtime_executable_candidates()
        .into_iter()
        .find(|path| path.is_file())
}

fn runtime_executable_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let executable_name = runtime_executable_name();

    if let Ok(current_exe) = std::env::current_exe()
        && let Some(exe_dir) = current_exe.parent()
    {
        candidates.push(exe_dir.join(executable_name));
    }

    let root = workspace_root();
    candidates.push(root.join("target").join("debug").join(executable_name));
    candidates.push(root.join("target").join("release").join(executable_name));
    candidates
}

fn runtime_executable_name() -> &'static str {
    if cfg!(windows) {
        "besfa_runtime.exe"
    } else {
        "besfa_runtime"
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_abi_version() {
        assert_eq!(
            crate::besfa_flutter_plugin_abi_version(),
            besfa_core::ABI_VERSION
        );
    }
}
