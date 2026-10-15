//! An MCP server over stdio that runs a Besfa game in edit mode, the way
//! the editor's Run does, and passes it `besfa_protocol` commands, so an
//! agent can read and change the scene without driving the editor's UI.
//!
//! Tools: `start` a game, `send` it a command, list its `entities` or one
//! entity's components, and `stop` it. The game is stopped when the client
//! closes stdin.

use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command as Process, Stdio};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use besfa_protocol::{Command, EntityInfo, Report, Request};
use serde_json::{Value, json};

/// How much of the game's log a failure shows.
const LOG_LINES: usize = 40;

fn main() {
    let mut game = None;
    let mut stdout = io::stdout();
    // Stdout carries only JSON-RPC lines; the game's output is piped.
    for line in io::stdin().lock().lines().map_while(Result::ok) {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        // Notifications have no id and get no answer.
        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        let reply = match handle(&mut game, &message) {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        };
        let _ = writeln!(stdout, "{reply}").and_then(|()| stdout.flush());
    }
}

/// The result of the JSON-RPC request `message`, or its error object.
fn handle(game: &mut Option<Game>, message: &Value) -> Result<Value, Value> {
    let params = &message["params"];
    match message["method"].as_str().unwrap_or_default() {
        "initialize" => Ok(json!({
            // ponytail: agrees to whatever version the client asks for, since
            // tools/list and tools/call have not changed across versions.
            // Negotiate properly if a client ever needs it.
            "protocolVersion": params["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "besfa", "version": env!("CARGO_PKG_VERSION")},
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or_default();
            // A failed tool is a result the model reads, not a protocol error.
            let (text, error) = match call(game, name, &params["arguments"]) {
                Ok(text) => (text, false),
                Err(text) => (text, true),
            };
            Ok(json!({"content": [{"type": "text", "text": text}], "isError": error}))
        }
        method => Err(json!({"code": -32601, "message": format!("Unknown method {method}")})),
    }
}

fn tools() -> Value {
    let timeout = json!({
        "type": "number",
        "description": "Seconds to wait for the game, 30 by default. The first start after a code change builds the game, which can take minutes.",
    });
    json!([
        {
            "name": "start",
            "description": "Builds and runs the Besfa game project at `project` in edit mode, paused, as the editor's Run does (`cargo run --features bevy/dynamic_linking`). Returns once cargo starts; the other tools wait for the game. The game opens its own window. One game at a time; do not run one on a project the editor has open, since both would write its scene file.",
            "inputSchema": {
                "type": "object",
                "properties": {"project": {"type": "string", "description": "Absolute path of the game project, the folder with its Cargo.toml."}},
                "required": ["project"],
            },
        },
        {
            "name": "send",
            "description": concat!(
                "Sends one editor command to the running game and returns the reports it wrote until it ran the command, one JSON object per line. ",
                "Entities are the ids `entities` lists; components are named by type path, as `entities` with an id shows them. Commands:\n",
                "{\"command\":\"spawn\",\"kind\":\"empty|cube|sphere|plane|light|camera\"} reports `spawned` with the new id\n",
                "{\"command\":\"instantiate\",\"path\":\"models/tree.glb\"} places an asset, relative to assets/\n",
                "{\"command\":\"set\",\"id\":ID,\"component\":PATH,\"value\":{...}} replaces the whole value; left-out fields take defaults\n",
                "{\"command\":\"insert\",\"id\":ID,\"component\":PATH} / {\"command\":\"remove\",...}\n",
                "{\"command\":\"duplicate\",\"id\":ID} / {\"command\":\"delete\",\"id\":ID} hides it until {\"command\":\"restore\",\"id\":ID}\n",
                "{\"command\":\"save\"} writes the scene file; {\"command\":\"save_prefab\",\"id\":ID,\"path\":\"prefabs/x.scn.ron\"}\n",
                "{\"command\":\"play\"} unpauses the game\n",
                "{\"command\":\"thumbnail\",\"path\":\"models/x.glb\",\"file\":\"thumb.png\"} renders that model on its own to a PNG relative to the project, not the scene; pass wait_for \"thumbnail\"\n",
                "An unknown command is refused with the list of valid ones.",
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": {"type": "object", "description": "The command, tagged by its `command` field."},
                    "wait_for": {"type": "string", "description": "Also wait for a report of this type, for commands answered later, like `thumbnail`, `screenshot` or `reimported`."},
                    "timeout_secs": timeout,
                },
                "required": ["command"],
            },
        },
        {
            "name": "entities",
            "description": "Without `id`, the scene's entities: id, name, parent, whether the scene file keeps it, and the asset it shows. With `id`, selects that entity in the game and returns its components with their values. Right after start the first list can come before the scene file has loaded, so `scene` reads false; ask again.",
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "integer"}, "timeout_secs": timeout},
            },
        },
        {
            "name": "stop",
            "description": "Stops the game and cargo. Unsaved changes are lost.",
            "inputSchema": {"type": "object", "properties": {}},
        },
    ])
}

/// Runs the tool `name`; the text is what the model reads either way.
fn call(game: &mut Option<Game>, name: &str, args: &Value) -> Result<String, String> {
    let timeout = Duration::from_secs_f64(args["timeout_secs"].as_f64().unwrap_or(30.0));
    match name {
        "start" => {
            if game.is_some() {
                return Err("A game is running; stop it first.".into());
            }
            let project = args["project"].as_str().ok_or("`project` is required.")?;
            let started = Game::start(Path::new(project))
                .map_err(|error| format!("Could not start cargo in {project}: {error}"))?;
            *game = Some(started);
            Ok(
                "Started `cargo run` in edit mode; the game opens a window once it is built."
                    .into(),
            )
        }
        "send" => {
            // Checked here, since the game ignores a line it cannot read and
            // never answers it.
            let command = serde_json::from_value::<Command>(args["command"].clone())
                .map_err(|error| format!("Not a command: {error}"))?;
            let reports = running(game)?.send(command, args["wait_for"].as_str(), timeout)?;
            let failed = reports.iter().any(|report| !report["error"].is_null());
            let text = lines(&reports);
            if failed { Err(text) } else { Ok(text) }
        }
        "entities" => {
            let game = running(game)?;
            match args["id"].as_u64() {
                None => {
                    let inbox = game.wait(timeout, |inbox| inbox.entities.is_some())?;
                    Ok(serde_json::to_string(&inbox.entities).unwrap())
                }
                Some(id) => {
                    game.send(Command::Select { id: Some(id) }, None, timeout)?;
                    // Reported after the command ran, or earlier if it was
                    // selected already, since the game repeats no report.
                    let inbox = game.wait(timeout, |inbox| {
                        inbox
                            .entity
                            .as_ref()
                            .is_some_and(|entity| entity["id"] == id)
                    })?;
                    Ok(inbox.entity.as_ref().unwrap().to_string())
                }
            }
        }
        "stop" => Ok(match game.take() {
            Some(_) => "Stopped.".into(),
            None => "No game was running.".into(),
        }),
        _ => Err(format!("Unknown tool {name}.")),
    }
}

fn running(game: &mut Option<Game>) -> Result<&mut Game, String> {
    game.as_mut()
        .ok_or_else(|| "No game is running; start one first.".into())
}

fn lines(values: &[Value]) -> String {
    let lines: Vec<String> = values.iter().map(Value::to_string).collect();
    lines.join("\n")
}

/// What the game wrote so far.
#[derive(Default)]
struct Inbox {
    hello: bool,
    /// Reports `send` has not returned yet. The entity list is left out,
    /// since `entities` has it, and so is the hello.
    reports: Vec<Value>,
    entities: Option<Vec<EntityInfo>>,
    /// The latest report of the selection's components.
    entity: Option<Value>,
    logs: VecDeque<String>,
    /// The game's outputs still open; none once it exited.
    open: u8,
}

impl Inbox {
    fn receive(&mut self, report: Report) {
        match report {
            Report::Hello { .. } => self.hello = true,
            Report::Entities { entities } => self.entities = Some(entities),
            // Long lists the editor shows in its panels, written once.
            Report::Systems { .. } | Report::Components { .. } => {}
            report => {
                let value = serde_json::to_value(&report).unwrap();
                if let Report::Entity { .. } = report {
                    self.entity = Some(value.clone());
                }
                self.reports.push(value);
            }
        }
    }

    fn log(&mut self, line: String) {
        self.logs.push_back(line);
        if self.logs.len() > LOG_LINES {
            self.logs.pop_front();
        }
    }
}

type Shared = Arc<(Mutex<Inbox>, Condvar)>;

/// A game cargo runs, and what it reported.
struct Game {
    cargo: Child,
    stdin: ChildStdin,
    /// Numbers requests, so each `done` report answers one.
    seq: u64,
    inbox: Shared,
}

impl Game {
    /// Starts the game the way the editor does, minus its viewport, so the
    /// shared Bevy build is reused.
    fn start(project: &Path) -> io::Result<Game> {
        let mut cargo = quiet("cargo")
            .args(["run", "--features", "bevy/dynamic_linking"])
            .current_dir(project)
            // Read by besfa_editor_plugin: start paused and take commands.
            .env("BESFA_EDIT_MODE", "1")
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let inbox: Shared = Arc::new((
            Mutex::new(Inbox {
                open: 2,
                ..Inbox::default()
            }),
            Condvar::new(),
        ));
        read(cargo.stdout.take().unwrap(), inbox.clone());
        read(cargo.stderr.take().unwrap(), inbox.clone());
        let stdin = cargo.stdin.take().unwrap();
        Ok(Game {
            cargo,
            stdin,
            seq: 0,
            inbox,
        })
    }

    /// Writes `command` once the game is up and returns the reports written
    /// until it ran, and until one of type `wait_for` came.
    fn send(
        &mut self,
        command: Command,
        wait_for: Option<&str>,
        timeout: Duration,
    ) -> Result<Vec<Value>, String> {
        // Before the hello the game may still be building, or have failed to.
        let first = self.wait(timeout, |inbox| inbox.hello)?.reports.len();
        self.seq += 1;
        let seq = self.seq;
        let line = serde_json::to_string(&Request {
            seq: Some(seq),
            command,
        })
        .unwrap();
        writeln!(self.stdin, "{line}")
            .and_then(|()| self.stdin.flush())
            .map_err(|error| format!("Could not write to the game: {error}"))?;
        let has = |reports: &[Value], kind: &str| {
            reports
                .iter()
                .any(|report| report["type"] == kind && (kind != "done" || report["seq"] == seq))
        };
        let result = self.wait(timeout, |inbox| {
            let new = &inbox.reports[first.min(inbox.reports.len())..];
            has(new, "done") && wait_for.is_none_or(|kind| has(new, kind))
        });
        match result {
            Ok(mut inbox) => Ok(std::mem::take(&mut inbox.reports)),
            // `wait` let go of the inbox, so it can be locked again.
            Err(error) => {
                let (lock, _) = &*self.inbox;
                let reports = std::mem::take(&mut lock.lock().unwrap().reports);
                Err(format!("{error}\nReports so far:\n{}", lines(&reports)))
            }
        }
    }

    /// The inbox once `ready` holds, or why it did not within `timeout`.
    fn wait(
        &self,
        timeout: Duration,
        mut ready: impl FnMut(&Inbox) -> bool,
    ) -> Result<MutexGuard<'_, Inbox>, String> {
        let (lock, wake) = &*self.inbox;
        let (inbox, _) = wake
            .wait_timeout_while(lock.lock().unwrap(), timeout, |inbox| {
                !ready(inbox) && inbox.open > 0
            })
            .unwrap();
        if ready(&inbox) {
            return Ok(inbox);
        }
        let why = if inbox.open == 0 {
            "The game exited; stop it, fix what the log shows, and start it again."
        } else if !inbox.hello {
            "The game is not up yet; cargo may still be building. Try again."
        } else {
            "The game did not answer in time."
        };
        let log: Vec<&str> = inbox.logs.iter().map(String::as_str).collect();
        Err(format!("{why}\nLog:\n{}", log.join("\n")))
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        // Killing cargo alone would leave the game running.
        let _ = quiet("taskkill")
            .args(["/PID", &self.cargo.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = self.cargo.wait();
    }
}

/// Files the game's report lines into the inbox and the rest into its log.
fn read(output: impl Read + Send + 'static, inbox: Shared) {
    thread::spawn(move || {
        let (lock, wake) = &*inbox;
        let mut output = BufReader::new(output);
        let mut bytes = Vec::new();
        // Lossy, since a linker may write in the console's code page.
        while output
            .read_until(b'\n', &mut bytes)
            .is_ok_and(|read| read > 0)
        {
            let line = String::from_utf8_lossy(&bytes).trim_end().to_string();
            bytes.clear();
            let mut inbox = lock.lock().unwrap();
            match Report::parse(&line) {
                Some(report) => inbox.receive(report),
                None => inbox.log(line),
            }
            wake.notify_all();
        }
        lock.lock().unwrap().open -= 1;
        wake.notify_all();
    });
}

/// A console program that opens no console window of its own.
fn quiet(program: &str) -> Process {
    let mut command = Process::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, params: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    }

    #[test]
    fn answers_the_handshake_and_lists_the_tools() {
        let mut game = None;
        let init = handle(
            &mut game,
            &request("initialize", json!({"protocolVersion": "2025-06-18"})),
        )
        .unwrap();
        assert_eq!(init["protocolVersion"], "2025-06-18");
        assert!(init["capabilities"]["tools"].is_object());

        let list = handle(&mut game, &request("tools/list", json!({}))).unwrap();
        let names: Vec<&str> = list["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["start", "send", "entities", "stop"]);

        assert!(handle(&mut game, &request("nope", json!({}))).is_err());
    }

    #[test]
    fn refuses_a_bad_command_before_needing_a_game() {
        let mut game = None;
        let call = |game: &mut Option<Game>, command: Value| {
            let params = json!({"name": "send", "arguments": {"command": command}});
            handle(game, &request("tools/call", params)).unwrap()
        };

        let bad = call(&mut game, json!({"command": "nope"}));
        assert_eq!(bad["isError"], true);
        assert!(
            bad["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("spawn")
        );

        let good = call(&mut game, json!({"command": "spawn", "kind": "cube"}));
        assert_eq!(good["isError"], true);
        assert!(
            good["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("No game is running")
        );
    }
}
