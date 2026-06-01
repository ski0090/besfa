use std::{
    ffi::{CString, c_char},
    fs::{File, OpenOptions, create_dir_all},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    ptr,
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
const RUNTIME_ERROR_INVALID_ARGUMENT: i32 = 6;

static RUNTIME_PROCESS: Mutex<Option<Child>> = Mutex::new(None);
static RUNTIME_LAST_ERROR: Mutex<i32> = Mutex::new(RUNTIME_ERROR_NONE);
static RUNTIME_LOG_PATH: Mutex<Option<CString>> = Mutex::new(None);

#[derive(Debug, Clone, Copy)]
struct RuntimeIpcLaunch {
    port: u16,
    token: u64,
}

/// Returns the ABI version exposed by the native bridge.
#[unsafe(no_mangle)]
pub extern "C" fn besfa_flutter_plugin_abi_version() -> u32 {
    besfa_core::ABI_VERSION
}

/// Native smoke-test function used by Dart FFI tests.
#[unsafe(no_mangle)]
pub extern "C" fn besfa_flutter_plugin_add(left: i32, right: i32) -> i32 {
    left + right
}

/// Starts the preview runtime without IPC.
#[unsafe(no_mangle)]
pub extern "C" fn besfa_runtime_start() -> i32 {
    start_runtime(None)
}

/// Starts the preview runtime with localhost IPC launch arguments.
#[unsafe(no_mangle)]
pub extern "C" fn besfa_runtime_start_with_ipc(port: i32, token: u64) -> i32 {
    let Ok(port) = u16::try_from(port) else {
        set_last_error(RUNTIME_ERROR_INVALID_ARGUMENT);
        return RUNTIME_COMMAND_FAILED;
    };

    if port == 0 || token == 0 {
        set_last_error(RUNTIME_ERROR_INVALID_ARGUMENT);
        return RUNTIME_COMMAND_FAILED;
    }

    start_runtime(Some(RuntimeIpcLaunch { port, token }))
}

fn start_runtime(ipc: Option<RuntimeIpcLaunch>) -> i32 {
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

    let mut command = Command::new(runtime_path);
    command.current_dir(&working_dir).stdin(Stdio::null());
    configure_runtime_stdio(&mut command, &working_dir);

    if let Some(ipc) = ipc {
        command
            .arg("--ipc-port")
            .arg(ipc.port.to_string())
            .arg("--ipc-token")
            .arg(ipc.token.to_string());
    }

    match command.spawn() {
        Ok(child) => {
            *runtime_process = Some(child);
            set_last_error(RUNTIME_ERROR_NONE);
            RUNTIME_COMMAND_OK
        }
        Err(_) => {
            set_runtime_log_path(None);
            set_last_error(RUNTIME_ERROR_SPAWN_FAILED);
            RUNTIME_COMMAND_FAILED
        }
    }
}

/// Returns the current runtime stdout/stderr log file path as UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn besfa_runtime_log_path() -> *const c_char {
    let Ok(log_path) = RUNTIME_LOG_PATH.lock() else {
        return ptr::null();
    };

    log_path
        .as_ref()
        .map(|path| path.as_ptr())
        .unwrap_or(ptr::null())
}

/// Stops the tracked preview runtime process.
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

/// Returns the tracked preview runtime process status.
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

/// Returns the last native runtime bridge error code.
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

fn configure_runtime_stdio(command: &mut Command, working_dir: &Path) {
    let log_path = runtime_log_path(working_dir);
    match open_runtime_log_file(&log_path) {
        Ok(stdout) => match stdout.try_clone() {
            Ok(stderr) => {
                command
                    .stdout(Stdio::from(stdout))
                    .stderr(Stdio::from(stderr));
                set_runtime_log_path(Some(log_path));
            }
            Err(_) => {
                command.stdout(Stdio::null()).stderr(Stdio::null());
                set_runtime_log_path(None);
            }
        },
        Err(_) => {
            command.stdout(Stdio::null()).stderr(Stdio::null());
            set_runtime_log_path(None);
        }
    }
}

fn open_runtime_log_file(log_path: &Path) -> std::io::Result<File> {
    if let Some(parent) = log_path.parent() {
        create_dir_all(parent)?;
    }

    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(log_path)
}

fn set_runtime_log_path(path: Option<PathBuf>) {
    if let Ok(mut runtime_log_path) = RUNTIME_LOG_PATH.lock() {
        *runtime_log_path = path.and_then(|path| {
            let path = path.to_string_lossy().into_owned();
            CString::new(path).ok()
        });
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

fn runtime_log_path(working_dir: &Path) -> PathBuf {
    let root = workspace_root();
    if root.is_dir() {
        return root.join("target").join("besfa_runtime.log");
    }

    working_dir.join("besfa_runtime.log")
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

    #[test]
    fn runtime_log_path_is_null_before_launch() {
        assert!(crate::besfa_runtime_log_path().is_null());
    }
}
