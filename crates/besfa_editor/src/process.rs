//! Command-line processes the editor starts, such as a game's `cargo run`,
//! with their output reported line by line, and the background Bevy
//! prebuild.

use std::{
    io::{self, Read, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use eframe::egui;

pub enum Output {
    Line(String),
    /// Always the last message: every line comes before it.
    Exit(i32),
}

pub struct CliProcess {
    pid: u32,
    stdin: Option<ChildStdin>,
    output: Receiver<Output>,
}

/// Starts console programs without a console window of their own.
pub fn command(program: &str) -> Command {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new(on_path(program));
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// `program` as found on PATH outside the editor's own folder, or as given
/// when it is not there. A workspace build leaves a `besfa.exe` next to the
/// editor that may be older than the one on PATH, and both Windows and
/// `cargo run`, which puts `target\debug` on PATH, would find that one first.
fn on_path(program: &str) -> PathBuf {
    let paths = std::env::var_os("PATH").unwrap_or_default();
    let exe = std::env::current_exe().ok();
    let own = exe.as_deref().and_then(Path::parent);
    find_program(program, std::env::split_paths(&paths), own).unwrap_or_else(|| program.into())
}

/// The first `program` in `dirs`, skipping `own` and the folders in it.
fn find_program(
    program: &str,
    dirs: impl IntoIterator<Item = PathBuf>,
    own: Option<&Path>,
) -> Option<PathBuf> {
    let file = format!("{program}{}", std::env::consts::EXE_SUFFIX);
    // Windows paths ignore case.
    let lower = |path: &Path| PathBuf::from(path.to_string_lossy().to_lowercase());
    dirs.into_iter()
        .filter(|dir| !own.is_some_and(|own| lower(dir).starts_with(lower(own))))
        .map(|dir| dir.join(&file))
        .find(|path| path.is_file())
}

impl CliProcess {
    /// Starts `program` in `directory` and reports its stdout and stderr
    /// lines; cargo writes its build progress to stderr. Each message wakes
    /// the UI.
    pub fn start(
        directory: &Path,
        program: &str,
        args: &[&str],
        env: &[(&str, &str)],
        ctx: &egui::Context,
    ) -> io::Result<Self> {
        let mut child = command(program)
            .args(args)
            .current_dir(directory)
            // The log panel shows plain text, not ANSI colors.
            .env("NO_COLOR", "1")
            .envs(env.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let (pid, stdin) = (child.id(), child.stdin.take());
        let (sender, output) = mpsc::channel();
        let readers = [
            reader(child.stdout.take().unwrap(), sender.clone(), ctx.clone()),
            reader(child.stderr.take().unwrap(), sender.clone(), ctx.clone()),
        ];
        let ctx = ctx.clone();
        // ponytail: a child that inherits the pipes (a game left behind when
        // cargo is killed from outside without /T) keeps the readers open;
        // fall back to wait() after a grace period if that ever shows up.
        thread::spawn(move || {
            for reader in readers {
                let _ = reader.join();
            }
            let code = child.wait().ok().and_then(|status| status.code());
            let _ = sender.send(Output::Exit(code.unwrap_or(-1)));
            ctx.request_repaint();
        });
        Ok(Self { pid, stdin, output })
    }

    /// Writes `line` to the process's stdin. A child that inherits it, like
    /// the game under `cargo run`, reads it once it starts. Lines sent after
    /// the process exited fail on the closed pipe and are dropped.
    pub fn send(&mut self, line: &str) {
        if let Some(stdin) = &mut self.stdin {
            let _ = stdin.write_all(format!("{line}\n").as_bytes());
        }
    }

    /// Stops the process and everything it launched. Killing only cargo
    /// would leave the game it launched as an orphan.
    pub fn stop(&self) {
        let pid = self.pid.to_string();
        // The exit arrives through poll() once the tree is gone.
        let _ = command("taskkill").args(["/PID", &pid, "/T", "/F"]).spawn();
    }

    /// What arrived since the last call.
    pub fn poll(&self) -> Vec<Output> {
        self.output.try_iter().collect()
    }
}

fn reader(
    pipe: impl Read + Send + 'static,
    sender: Sender<Output>,
    ctx: egui::Context,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        split_lines(pipe, |line| {
            let _ = sender.send(Output::Line(line));
            ctx.request_repaint();
        });
    })
}

/// Reports each line of `pipe` as it arrives. A line ends at `\n`, `\r\n`
/// or a bare `\r`, which cargo's progress bar ends each redraw with.
fn split_lines(mut pipe: impl Read, mut emit: impl FnMut(String)) {
    let mut line = Vec::new();
    let mut buffer = [0; 4096];
    let mut after_cr = false;
    while let Ok(read @ 1..) = pipe.read(&mut buffer) {
        for &byte in &buffer[..read] {
            match byte {
                b'\n' if after_cr => {}
                b'\r' | b'\n' => emit(String::from_utf8_lossy(&std::mem::take(&mut line)).into()),
                _ => line.push(byte),
            }
            after_cr = byte == b'\r';
        }
    }
    if !line.is_empty() {
        emit(String::from_utf8_lossy(&line).into());
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum PrebuildPhase {
    Running,
    Ready,
    Failed,
}

/// Runs `besfa prebuild` once and keeps its progress for the UI: cargo's
/// latest output line and share of finished units while it runs, then
/// whether Bevy is ready.
pub struct Prebuild {
    pub phase: PrebuildPhase,
    pub message: String,
    /// Share of cargo's build units finished, 0 to 1; None until cargo
    /// reports it, and when nothing needs building.
    pub progress: Option<f32>,
    process: Option<CliProcess>,
}

impl Prebuild {
    pub fn start(ctx: &egui::Context) -> Self {
        Self::start_with(ctx, "besfa", &["prebuild"])
    }

    fn start_with(ctx: &egui::Context, program: &str, args: &[&str]) -> Self {
        // Cargo draws its progress bar only on a terminal unless forced, and
        // a forced bar needs a width.
        let env = [
            ("CARGO_TERM_PROGRESS_WHEN", "always"),
            ("CARGO_TERM_PROGRESS_WIDTH", "80"),
        ];
        match CliProcess::start(Path::new("."), program, args, &env, ctx) {
            Ok(process) => Self {
                phase: PrebuildPhase::Running,
                message: "Preparing Bevy…".into(),
                progress: None,
                process: Some(process),
            },
            // The game still runs without it; it just builds Bevy itself.
            Err(error) => Self {
                phase: PrebuildPhase::Failed,
                message: format!("Could not start {program}: {error}"),
                progress: None,
                process: None,
            },
        }
    }

    /// The prebuild holds the shared build directory, so a game started now
    /// would only wait for it.
    pub fn running(&self) -> bool {
        self.phase == PrebuildPhase::Running
    }

    pub fn poll(&mut self) {
        let Some(process) = &self.process else {
            return;
        };
        for output in process.poll() {
            match output {
                Output::Line(line) => {
                    let line = line.trim();
                    if let Some(progress) = build_progress(line) {
                        self.progress = Some(progress);
                    } else if !line.is_empty() {
                        self.message = line.into();
                    }
                }
                Output::Exit(code) => {
                    self.process = None;
                    self.progress = None;
                    (self.phase, self.message) = if code == 0 {
                        (PrebuildPhase::Ready, "Bevy is ready".into())
                    } else {
                        (
                            PrebuildPhase::Failed,
                            format!("Bevy prebuild failed (exit {code})"),
                        )
                    };
                }
            }
        }
    }
}

#[cfg(test)]
impl Prebuild {
    /// A prebuild in `phase` with no process behind it.
    pub fn with_phase(phase: PrebuildPhase, message: &str) -> Self {
        Self {
            phase,
            message: message.into(),
            progress: None,
            process: None,
        }
    }
}

/// The share of finished units in cargo's progress bar, as in
/// `Building [==>   ] 210/498: bevy_render…`.
fn build_progress(line: &str) -> Option<f32> {
    let rest = line.strip_prefix("Building [")?;
    let (done, rest) = rest[rest.find(']')? + 1..].trim_start().split_once('/')?;
    let total: String = rest.chars().take_while(char::is_ascii_digit).collect();
    Some(done.parse::<f32>().ok()? / total.parse::<f32>().ok()?)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn lines(bytes: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        split_lines(bytes, |line| lines.push(line));
        lines
    }

    #[test]
    fn splits_lines_on_newlines_and_bare_carriage_returns() {
        assert_eq!(lines(b"a\r\nb\rc\n\nd"), ["a", "b", "c", "", "d"]);
        assert_eq!(lines("한글\n".as_bytes()), ["한글"]);
    }

    #[test]
    fn reads_the_share_of_finished_units() {
        assert_eq!(build_progress("Building [==   ] 1/4: bevy"), Some(0.25));
        assert_eq!(build_progress("Building [=====] 4/4"), Some(1.0));
        assert_eq!(build_progress("Compiling bevy v0.19.1"), None);
    }

    fn wait_for_exit(prebuild: &mut Prebuild, statuses: &mut Vec<(String, Option<f32>)>) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while prebuild.running() && Instant::now() < deadline {
            let before = (prebuild.message.clone(), prebuild.progress);
            prebuild.poll();
            let after = (prebuild.message.clone(), prebuild.progress);
            if after != before {
                statuses.push(after);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn follows_cargos_progress_bar_then_the_result() {
        let mut prebuild = Prebuild::start_with(
            &egui::Context::default(),
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                r#"[Console]::Error.Write("   Compiling bevy v0.19.1`n    Building [==   ] 1/4: bevy`r    Building [====  ] 3/4: bevy`r   Compiling game v0.1.0`n"); exit 3"#,
            ],
        );
        let mut statuses = Vec::new();
        wait_for_exit(&mut prebuild, &mut statuses);
        let expected = [
            ("Compiling bevy v0.19.1", None),
            ("Compiling bevy v0.19.1", Some(0.25)),
            ("Compiling bevy v0.19.1", Some(0.75)),
            ("Compiling game v0.1.0", Some(0.75)),
            ("Bevy prebuild failed (exit 3)", None),
        ];
        let statuses: Vec<_> = statuses.iter().map(|(m, p)| (m.as_str(), *p)).collect();
        // Polling may merge updates that arrive together; none may be wrong.
        assert!(
            statuses.iter().all(|status| expected.contains(status)),
            "{statuses:?}"
        );
        assert_eq!(statuses.last(), expected.last());
    }

    #[test]
    fn finds_programs_on_path_outside_the_editor() {
        assert!(on_path("cmd").is_absolute(), "{:?}", on_path("cmd"));
        let dir = std::env::temp_dir().join(format!("besfa_path_{}", std::process::id()));
        let (own, other) = (dir.join("Target/debug"), dir.join("bin"));
        for folder in [own.join("deps"), own.clone(), other.clone()] {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                folder.join(format!("foo{}", std::env::consts::EXE_SUFFIX)),
                "",
            )
            .unwrap();
        }
        let dirs = || vec![own.join("deps"), own.clone(), other.clone()];
        let found = find_program("foo", dirs(), Some(&dir.join("target/DEBUG"))).unwrap();
        assert!(found.starts_with(&other), "{found:?}");
        assert!(
            find_program("foo", dirs(), None)
                .unwrap()
                .starts_with(own.join("deps"))
        );
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!(
            on_path("besfa-missing-cli"),
            PathBuf::from("besfa-missing-cli")
        );
    }

    #[test]
    fn reports_a_cli_that_cannot_start() {
        let prebuild = Prebuild::start_with(&egui::Context::default(), "besfa-missing-cli", &[]);
        assert!(prebuild.phase == PrebuildPhase::Failed);
        assert!(
            prebuild
                .message
                .starts_with("Could not start besfa-missing-cli")
        );
    }

    #[test]
    fn stop_ends_the_whole_process_tree() {
        let ctx = egui::Context::default();
        let process = CliProcess::start(
            Path::new("."),
            "cmd",
            &["/c", "ping -n 30 127.0.0.1 > nul"],
            &[],
            &ctx,
        )
        .unwrap();
        process.stop();
        // taskkill alone takes seconds, longer while other tests run.
        let exit = process.output.recv_timeout(Duration::from_secs(15));
        assert!(matches!(exit, Ok(Output::Exit(code)) if code != 0));
    }
}
