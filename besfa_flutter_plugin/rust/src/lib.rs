use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
};

const RUNTIME_COMMAND_OK: i32 = 0;
const RUNTIME_COMMAND_ALREADY_RUNNING: i32 = 1;
const RUNTIME_COMMAND_NOT_RUNNING: i32 = 2;
const RUNTIME_COMMAND_FAILED: i32 = -1;

const RUNTIME_STATUS_STOPPED: i32 = 0;
const RUNTIME_STATUS_RUNNING: i32 = 1;
const RUNTIME_STATUS_EXITED: i32 = 2;
const RUNTIME_STATUS_FAILED: i32 = -1;

const RUNTIME_ERROR_NONE: i32 = 0;
const RUNTIME_ERROR_LOCK_POISONED: i32 = 1;
const RUNTIME_ERROR_EXECUTABLE_NOT_FOUND: i32 = 2;
const RUNTIME_ERROR_SPAWN_FAILED: i32 = 3;
const RUNTIME_ERROR_STATUS_FAILED: i32 = 4;
const RUNTIME_ERROR_STOP_FAILED: i32 = 5;

static RUNTIME_PROCESS: Mutex<Option<Child>> = Mutex::new(None);
static RUNTIME_LAST_ERROR: Mutex<i32> = Mutex::new(RUNTIME_ERROR_NONE);

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
        Err(_) => {
            set_last_error(RUNTIME_ERROR_LOCK_POISONED);
            return RUNTIME_COMMAND_FAILED;
        }
    };

    if let Some(child) = runtime_process.as_mut() {
        match child.try_wait() {
            Ok(Some(_)) => {
                *runtime_process = None;
            }
            Ok(None) => {
                set_last_error(RUNTIME_ERROR_NONE);
                return RUNTIME_COMMAND_ALREADY_RUNNING;
            }
            Err(_) => {
                set_last_error(RUNTIME_ERROR_STATUS_FAILED);
                return RUNTIME_COMMAND_FAILED;
            }
        }
    }

    let Some(runtime_path) = find_runtime_executable() else {
        set_last_error(RUNTIME_ERROR_EXECUTABLE_NOT_FOUND);
        return RUNTIME_COMMAND_FAILED;
    };
    let working_dir = runtime_working_dir(&runtime_path);

    match Command::new(runtime_path)
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            *runtime_process = Some(child);
            set_last_error(RUNTIME_ERROR_NONE);
            RUNTIME_COMMAND_OK
        }
        Err(_) => {
            set_last_error(RUNTIME_ERROR_SPAWN_FAILED);
            RUNTIME_COMMAND_FAILED
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn besfa_runtime_stop() -> i32 {
    let mut runtime_process = match RUNTIME_PROCESS.lock() {
        Ok(runtime_process) => runtime_process,
        Err(_) => {
            set_last_error(RUNTIME_ERROR_LOCK_POISONED);
            return RUNTIME_COMMAND_FAILED;
        }
    };

    let Some(child) = runtime_process.as_mut() else {
        set_last_error(RUNTIME_ERROR_NONE);
        return RUNTIME_COMMAND_NOT_RUNNING;
    };

    match child.try_wait() {
        Ok(Some(_)) => {
            *runtime_process = None;
            set_last_error(RUNTIME_ERROR_NONE);
            RUNTIME_COMMAND_NOT_RUNNING
        }
        Ok(None) => match child.kill().and_then(|_| child.wait()) {
            Ok(_) => {
                *runtime_process = None;
                set_last_error(RUNTIME_ERROR_NONE);
                RUNTIME_COMMAND_OK
            }
            Err(_) => {
                set_last_error(RUNTIME_ERROR_STOP_FAILED);
                RUNTIME_COMMAND_FAILED
            }
        },
        Err(_) => {
            set_last_error(RUNTIME_ERROR_STATUS_FAILED);
            RUNTIME_COMMAND_FAILED
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn besfa_runtime_status() -> i32 {
    let mut runtime_process = match RUNTIME_PROCESS.lock() {
        Ok(runtime_process) => runtime_process,
        Err(_) => {
            set_last_error(RUNTIME_ERROR_LOCK_POISONED);
            return RUNTIME_STATUS_FAILED;
        }
    };

    let Some(child) = runtime_process.as_mut() else {
        return RUNTIME_STATUS_STOPPED;
    };

    match child.try_wait() {
        Ok(Some(_)) => {
            *runtime_process = None;
            set_last_error(RUNTIME_ERROR_NONE);
            RUNTIME_STATUS_EXITED
        }
        Ok(None) => RUNTIME_STATUS_RUNNING,
        Err(_) => {
            set_last_error(RUNTIME_ERROR_STATUS_FAILED);
            RUNTIME_STATUS_FAILED
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn besfa_runtime_last_error_code() -> i32 {
    match RUNTIME_LAST_ERROR.lock() {
        Ok(last_error) => *last_error,
        Err(_) => RUNTIME_ERROR_LOCK_POISONED,
    }
}

fn set_last_error(code: i32) {
    if let Ok(mut last_error) = RUNTIME_LAST_ERROR.lock() {
        *last_error = code;
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

fn runtime_working_dir(runtime_path: &Path) -> PathBuf {
    if let Ok(working_dir) = std::env::var("BESFA_RUNTIME_WORKING_DIR") {
        let working_dir = PathBuf::from(working_dir);
        if working_dir.is_dir() {
            return working_dir;
        }
    }

    let root = workspace_root();
    if root.is_dir() {
        return root;
    }

    runtime_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
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
