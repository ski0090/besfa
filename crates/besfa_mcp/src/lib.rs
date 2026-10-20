//! An MCP server over stdio that runs a Besfa game in edit mode, the way
//! the editor's Run does, and passes it `besfa_protocol` commands, so an
//! agent can read and change the scene without driving the editor's UI.
//!
//! Tools: `start` a game, `send` it a command, list its `entities` or one
//! entity's components, and `stop` it. The game is stopped when the client
//! closes stdin.
//!
//! `besfa mcp` serves it, and so does this crate's own binary.

use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command as Process, Stdio};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

/// The features the editor runs a game with, for `besfa new`'s check.
pub use besfa_protocol::run_features;
use besfa_protocol::{Command, EntityInfo, Placement, Report, Request, SpawnKind};
use serde_json::{Value, json};

/// How much of the game's log a failure shows.
const LOG_LINES: usize = 40;

/// Answers MCP requests on stdin until the client closes it, then stops
/// the game it started.
pub fn serve() {
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
            "description": "Builds and runs the Besfa game project at `project` in edit mode, paused, as the editor's Run does (`cargo run --features bevy/dynamic_linking`, plus `besfa_editor` when the game declares it). Returns once cargo starts; the other tools wait for the game. The game runs without a window, so nothing on screen shows it; `thumbnail` still renders. One game at a time; do not run one on a project the editor has open, since both would write its scene file.",
            "inputSchema": {
                "type": "object",
                "properties": {"project": {"type": "string", "description": "Absolute path of the game project, the folder with its Cargo.toml; by default the folder the server runs in, which is the game's when its .mcp.json starts `besfa mcp`."}},
            },
        },
        {
            "name": "send",
            "description": concat!(
                "Sends editor commands to the running game, in order, and returns the reports it wrote until it ran them, one JSON object per line. ",
                "`spawned` reports come in the order of the commands that make entities; a failed command shows as {\"type\":\"done\",\"command\":INDEX,\"error\":...}. ",
                "Entities are the ids `entities` lists; components are named by type path, as `entities` with an id shows them. Commands:\n",
                "{\"command\":\"instantiate\",\"path\":\"models/oak-tree.glb\",\"translation\":[-55,0,10],\"yaw\":45,\"scale\":1.2,\"name\":\"Oak\",\"parent\":ID} places a model or prefab, relative to assets/; all but path optional; translation and yaw are in the world, yaw in degrees counterclockwise from above\n",
                "{\"command\":\"spawn\",\"kind\":\"empty|cube|sphere|plane|light|camera\"} takes the same placement; a named empty makes a group\n",
                "{\"command\":\"parent\",\"id\":ID,\"parent\":ID} moves it under another scene entity, or to the top without parent, staying where it is in the world\n",
                "{\"command\":\"set\",\"id\":ID,\"component\":PATH,\"value\":{...}} replaces the whole value; left-out fields take defaults\n",
                "{\"command\":\"insert\",\"id\":ID,\"component\":PATH} / {\"command\":\"remove\",...}\n",
                "{\"command\":\"duplicate\",\"id\":ID} / {\"command\":\"delete\",\"id\":ID} hides it until {\"command\":\"restore\",\"id\":ID}\n",
                "{\"command\":\"save\"} writes the scene file; {\"command\":\"save_prefab\",\"id\":ID,\"path\":\"prefabs/x.scn.ron\"}\n",
                "{\"command\":\"screenshot\",\"file\":\"shots/map.png\",\"target\":[0,0,0],\"eye\":[40,30,40]} or with \"top\":200 instead of eye, a view straight down over a square that many meters wide, north (-Z) up; renders the scene without editor overlays to a PNG relative to the project, once its models have loaded; read the file to see it\n",
                "{\"command\":\"thumbnail\",\"path\":\"models/x.glb\",\"file\":\"thumb.png\"} renders one model on its own\n",
                "Terrain: {\"command\":\"spawn\",\"kind\":\"terrain\",\"name\":\"Ground\"} makes flat ground 256 m a side centered on it, covered by its first layer; resize it by setting its besfa_editor_plugin::terrain::Terrain component to {\"size\":200}, which also takes \"layers\", up to four colors (by default grass, dirt, stone, sand); its heights go to a .terrain file of their own when the scene is saved\n",
                "Sky: {\"command\":\"spawn\",\"kind\":\"sky\"} adds the scene's sky, which every camera draws behind everything; set its besfa_editor_plugin::sky::Sky to {\"mode\":\"Gradient\",\"zenith\":{\"Srgba\":{\"red\":0.2,\"green\":0.42,\"blue\":0.78,\"alpha\":1.0}},\"horizon\":...,\"ground\":...,\"blend\":0.5,\"brightness\":1.0,\"rotation\":0.0,\"sun\":{\"show\":true,\"direction\":[0.4,0.6,-0.5],\"size\":3.0,\"color\":...,\"flare\":0.5},\"moon\":{\"show\":true,\"direction\":[-0.4,0.5,0.5],\"size\":4.0,\"color\":...,\"phase\":0.5}} (sun and moon are world directions toward them, size in degrees across, flare 0 to 1 a lens flare while the sun is in sight, phase 0 new, 0.25 first quarter, 0.5 full; drawn on the gradient only) or {\"mode\":\"Cubemap\",\"cubemap\":\"skies/night.png\"} (a .ktx2/.dds cubemap or six faces +X,-X,+Y,-Y,+Z,-Z stacked top to bottom)\n",
                "{\"command\":\"terrain_stroke\",\"id\":ID,\"tool\":\"raise|lower|smooth|flatten|paint\",\"points\":[[x,z],[x,z]],\"radius\":8,\"strength\":2,\"layer\":1} paints one stroke along the points, world X and Z, joined by straight lines; each place takes the brush where it reached most, so a line raises an even ridge; raise and lower take strength in meters at the brush middle, smooth, flatten (to the first point's height) and paint (layer needed) take 0 to 1; answers terrain_painted with the stroke number, and {\"command\":\"terrain_restore\",\"stroke\":N,\"before\":true} undoes it (false redoes it)\n",
                "{\"command\":\"terrain_fill\",\"id\":ID,\"height\":0,\"layer\":0} brings the whole terrain to a height in the world and covers it with a layer, or does one of the two; one stroke, undone with terrain_restore\n",
                "{\"command\":\"heights\",\"points\":[[x,z],...]} answers heights: the ground's height at each point, or null where no terrain is\n",
                "{\"command\":\"drop_to_surface\",\"ids\":[ID,...]} brings each down, or up out of the ground, onto the surface right below it, keeping its turn; a group (no mesh of its own) drops each thing in it; answers moved with each Transform before and after; \"No surface below NAME.\" fails one with nothing below\n",
                "{\"command\":\"scatter\",\"models\":[{\"path\":\"models/grass-clump.glb\",\"weight\":3},{\"path\":\"models/wildflowers.glb\",\"weight\":1}],\"center\":[0,0],\"radius\":25,\"count\":80,\"spacing\":1.5,\"scale\":[0.8,1.3],\"rotate\":true,\"align\":true,\"seed\":1} places up to count on the ground (terrain, else what is below, else height 0), at least spacing apart and from earlier scattered things, under the scene's Scatter group, with random turn and slope tilt when asked; the same seed on the same ground places the same; answers scattered with the added ids (fewer than count when the circle is full)\n",
                "{\"command\":\"scatter_erase\",\"center\":[0,0],\"radius\":10} deletes scattered things within the circle, never hand-placed ones; answers scattered with the removed ids\n",
                "Animation state machines: insert besfa_editor_plugin::animation::AnimationStateMachine on a placed glTF model's root and set it to {\"states\":[{\"name\":\"idle\",\"clip\":\"idle\"},{\"name\":\"walk\",\"clip\":\"walk\",\"speed\":1.2},{\"name\":\"jump\",\"clip\":\"jump\",\"once\":true}],\"transitions\":[{\"from\":\"idle\",\"to\":\"walk\",\"conditions\":[{\"param\":\"speed\",\"test\":\"Greater\",\"value\":0.1}],\"blend\":0.2},{\"from\":\"*\",\"to\":\"jump\",\"conditions\":[{\"param\":\"jump\",\"test\":\"Trigger\"}]},{\"from\":\"jump\",\"to\":\"idle\",\"exit_time\":1}]}; ",
                "the first state plays first; states play the model's named clips, looping unless once, at speed; a transition is taken, first in the list first, when every condition holds and exit_time of the from state's clip has played (1 once through, 0 any time), crossfading for blend seconds; from \"*\" is any state; tests are Greater and Less (float params, against value), True and False (bool params) and Trigger (let go once taken). ",
                "The game writes besfa_editor_plugin::animation::AnimationParams, {\"floats\":{\"speed\":2},\"bools\":{\"running\":true},\"triggers\":[\"jump\"],\"state\":\"walk\"}, where state is the state playing; set params to try transitions, or state to go there at once, sending the other params with it, since set replaces the whole value. `entities` with the root's id shows both, and AnimationClips, the model's clip names. Clips move only after play.\n",
                "{\"command\":\"key\",\"code\":\"KeyW\",\"pressed\":true} holds a key down in the playing game until the same with pressed false; codes are Bevy KeyCode names like KeyW, ShiftLeft, Space\n",
                "{\"command\":\"play\"} unpauses the game\n",
                "Screenshot, thumbnail and reimport are waited for until their answer comes. An unknown command is refused with the list of valid ones.",
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": {"type": "object", "description": "One command, tagged by its `command` field."},
                    "commands": {"type": "array", "items": {"type": "object"}, "description": "Several commands, sent together and run in order, instead of `command`."},
                    "timeout_secs": timeout,
                },
            },
        },
        {
            "name": "entities",
            "description": "Without `id`, the scene's entities, as JSON: id, name, parent, whether the scene file keeps it, and the asset it shows. The parts a placed model or prefab spawns under it are left out unless `parts` is true. `name` and `asset` keep those whose name or asset path contains the text, ignoring case, and `parent` those right under that entity. With `id`, selects that entity in the game and returns its components with their values. Right after start the first list can come before the scene file has loaded, so `scene` reads false; ask again.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                    "name": {"type": "string"},
                    "asset": {"type": "string"},
                    "parent": {"type": "integer"},
                    "parts": {"type": "boolean"},
                    "timeout_secs": timeout,
                },
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
            // The folder the server runs in, a game project's own when its
            // .mcp.json starts `besfa mcp`.
            let project = match args["project"].as_str() {
                Some(project) => PathBuf::from(project),
                None => std::env::current_dir()
                    .map_err(|error| format!("No project given or current: {error}"))?,
            };
            let started = Game::start(&project).map_err(|error| {
                format!("Could not start cargo in {}: {error}", project.display())
            })?;
            *game = Some(started);
            Ok("Started `cargo run` in edit mode, without a window.".into())
        }
        "send" => {
            let commands = match (&args["command"], &args["commands"]) {
                (Value::Null, Value::Array(commands)) => commands.clone(),
                (command @ Value::Object(_), Value::Null) => vec![command.clone()],
                _ => return Err("Give `command`, one object, or `commands`, a list.".into()),
            };
            // Checked here, since the game ignores a line it cannot read and
            // never answers it.
            let commands = commands
                .into_iter()
                .enumerate()
                .map(|(index, command)| {
                    serde_json::from_value::<Command>(command)
                        .map_err(|error| format!("Command {index} is not a command: {error}"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let reports = running(game)?.send(commands, timeout)?;
            let failed = reports.iter().any(|report| !report["error"].is_null());
            let text = lines(&reports);
            if failed { Err(text) } else { Ok(text) }
        }
        "entities" => {
            let game = running(game)?;
            match args["id"].as_u64() {
                None => {
                    let inbox = game.wait(timeout, |inbox| inbox.entities.is_some())?;
                    let entities = inbox.entities.as_deref().unwrap_or_default();
                    Ok(serde_json::to_string(&filter(entities, args)).unwrap())
                }
                Some(id) => {
                    game.send(vec![Command::Select { id: Some(id) }], timeout)?;
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

/// The entities `args` asks for: by name, asset and parent, and without
/// the parts placed models spawn unless `parts` is set.
fn filter<'a>(entities: &'a [EntityInfo], args: &Value) -> Vec<&'a EntityInfo> {
    let by_id: HashMap<u64, &EntityInfo> =
        entities.iter().map(|entity| (entity.id, entity)).collect();
    // Spawned by a model or prefab above it, so not saved with the scene.
    let part = |entity: &EntityInfo| {
        let mut parent = entity.parent;
        while let Some(above) = parent.and_then(|id| by_id.get(&id)) {
            if above.asset.is_some() {
                return !entity.scene;
            }
            parent = above.parent;
        }
        false
    };
    let contains = |text: Option<&str>, wanted: &Value| {
        wanted.as_str().is_none_or(|wanted| {
            text.is_some_and(|text| text.to_lowercase().contains(&wanted.to_lowercase()))
        })
    };
    entities
        .iter()
        .filter(|entity| {
            (args["parts"] == true || !part(entity))
                && contains(entity.name.as_deref(), &args["name"])
                && contains(entity.asset.as_deref(), &args["asset"])
                && args["parent"]
                    .as_u64()
                    .is_none_or(|parent| entity.parent == Some(parent))
        })
        .collect()
}

/// The report a command is answered with after it ran, and the path it
/// names, for `send` to wait for.
fn answer(command: &Command) -> Option<(&'static str, &str)> {
    match command {
        Command::Screenshot { file, .. } => Some(("screenshot", file)),
        Command::Thumbnail { path, .. } => Some(("thumbnail", path)),
        Command::Reimport { path } => Some(("reimported", path)),
        _ => None,
    }
}

/// The protocol version a game must speak to run `command`.
fn since(command: &Command) -> u32 {
    match command {
        Command::Snap { .. } | Command::Ghost { .. } => 11,
        Command::Spawn { place, .. } | Command::Instantiate { place, .. }
            if place.pointer.is_some() =>
        {
            11
        }
        Command::DropToSurface { .. } | Command::Scatter { .. } | Command::ScatterErase { .. } => 7,
        Command::Spawn {
            kind: SpawnKind::Sky,
            ..
        } => 10,
        Command::TerrainFill { .. } => 9,
        Command::ScatterBrush { .. } => 8,
        Command::TerrainBrush { .. } => 6,
        Command::Spawn {
            kind: SpawnKind::Terrain,
            ..
        }
        | Command::TerrainStroke { .. }
        | Command::TerrainRestore { .. }
        | Command::TerrainForget { .. }
        | Command::Heights { .. } => 5,
        Command::Screenshot { .. } | Command::Parent { .. } => 4,
        Command::Spawn { place, .. } | Command::Instantiate { place, .. }
            if *place != Placement::default() =>
        {
            4
        }
        _ => 0,
    }
}

fn running(game: &mut Option<Game>) -> Result<&mut Game, String> {
    game.as_mut()
        .ok_or_else(|| "No game is running; start one first.".into())
}

/// A command `send` wrote: its seq, and the report that answers it later
/// with the path that report names, if any.
type Asked = (u64, Option<(&'static str, String)>);

/// `reports` without the `done`s of commands that ran, and with a failed
/// one naming the index of its command among `asked` instead of its seq.
fn tidy(reports: Vec<Value>, asked: &[Asked]) -> Vec<Value> {
    reports
        .into_iter()
        .filter_map(|report| {
            if report["type"] != "done" {
                return Some(report);
            }
            if report["error"].is_null() {
                return None;
            }
            let index = asked.iter().position(|(seq, _)| report["seq"] == *seq);
            Some(json!({"type": "done", "command": index, "error": report["error"]}))
        })
        .collect()
}

fn lines(values: &[Value]) -> String {
    let lines: Vec<String> = values.iter().map(Value::to_string).collect();
    lines.join("\n")
}

/// What the game wrote so far.
#[derive(Default)]
struct Inbox {
    /// The protocol version the game speaks, once it said.
    hello: Option<u32>,
    /// Reports `send` has not returned yet. The entity list and the
    /// selection's components are left out, since `entities` has them, and
    /// so is the hello.
    reports: Vec<Value>,
    entities: Option<Vec<EntityInfo>>,
    /// The latest report of the selection's components, which each entity
    /// a command makes becomes.
    entity: Option<Value>,
    logs: VecDeque<String>,
    /// The game's outputs still open; none once it exited.
    open: u8,
}

impl Inbox {
    fn receive(&mut self, report: Report) {
        match report {
            Report::Hello { version } => self.hello = Some(version),
            Report::Entities { entities } => self.entities = Some(entities),
            // Long lists the editor shows in its panels, written once.
            Report::Systems { .. } | Report::Components { .. } => {}
            // Each entity a command makes is selected, which `spawned` says.
            Report::Selected { .. } => {}
            report @ Report::Entity { .. } => {
                self.entity = Some(serde_json::to_value(&report).unwrap());
            }
            report => self.reports.push(serde_json::to_value(&report).unwrap()),
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
    /// Starts the game the way the editor does, minus its viewport and any
    /// window, so the shared Bevy build is reused.
    fn start(project: &Path) -> io::Result<Game> {
        let manifest = std::fs::read_to_string(project.join("Cargo.toml")).unwrap_or_default();
        let mut cargo = quiet("cargo")
            .args([
                "run",
                "--features",
                &besfa_protocol::run_features(&manifest),
            ])
            .current_dir(project)
            // Read by besfa_editor_plugin: start paused and take commands.
            .env("BESFA_EDIT_MODE", "1")
            // Also read by it: open no window, which would take focus.
            .env("BESFA_HEADLESS", "1")
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

    /// Writes `commands` once the game is up and returns the reports written
    /// until it ran them, and for those answered later, until the answer
    /// came; tidied up by [`tidy`].
    fn send(&mut self, commands: Vec<Command>, timeout: Duration) -> Result<Vec<Value>, String> {
        // Before the hello the game may still be building, or have failed to.
        let (first, version) = {
            let inbox = self.wait(timeout, |inbox| inbox.hello.is_some())?;
            (inbox.reports.len(), inbox.hello.unwrap())
        };
        if let Some(index) = commands.iter().position(|command| since(command) > version) {
            return Err(format!(
                "Command {index} needs protocol {} and this game's besfa_editor_plugin speaks {version}; update it with `cargo update -p besfa_editor_plugin` in the game.",
                since(&commands[index])
            ));
        }
        let mut written = String::new();
        let mut asked = Vec::new();
        for command in commands {
            self.seq += 1;
            asked.push((
                self.seq,
                answer(&command).map(|(kind, path)| (kind, path.to_string())),
            ));
            let request = Request {
                seq: Some(self.seq),
                command,
            };
            written += &serde_json::to_string(&request).unwrap();
            written.push('\n');
        }
        self.stdin
            .write_all(written.as_bytes())
            .and_then(|()| self.stdin.flush())
            .map_err(|error| format!("Could not write to the game: {error}"))?;
        // Ran, and answered if it is answered later, unless it failed.
        let finished = |reports: &[Value], (seq, answer): &Asked| {
            let done = reports
                .iter()
                .find(|report| report["type"] == "done" && report["seq"] == *seq);
            done.is_some_and(|done| {
                !done["error"].is_null()
                    || answer.as_ref().is_none_or(|(kind, path)| {
                        reports.iter().any(|report| {
                            report["type"] == *kind && report["path"] == path.as_str()
                        })
                    })
            })
        };
        let result = self.wait(timeout, |inbox| {
            let new = &inbox.reports[first.min(inbox.reports.len())..];
            asked.iter().all(|asked| finished(new, asked))
        });
        match result {
            Ok(mut inbox) => Ok(tidy(std::mem::take(&mut inbox.reports), &asked)),
            // `wait` let go of the inbox, so it can be locked again.
            Err(error) => {
                let (lock, _) = &*self.inbox;
                let reports = std::mem::take(&mut lock.lock().unwrap().reports);
                Err(format!(
                    "{error}\nReports so far:\n{}",
                    lines(&tidy(reports, &asked))
                ))
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
        } else if inbox.hello.is_none() {
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
    fn filters_entities_and_leaves_out_the_parts_of_placed_models() {
        let info = |id, name: &str, parent, scene, asset: Option<&str>| EntityInfo {
            id,
            name: Some(name.into()),
            parent,
            scene,
            asset: asset.map(Into::into),
            sky: false,
        };
        let entities = [
            info(1, "Forest", None, true, None),
            info(2, "Oak", Some(1), true, Some("models/oak-tree.glb")),
            info(3, "Trunk", Some(2), false, None),
            info(4, "Leaves", Some(3), false, None),
            info(5, "Birch", Some(1), true, Some("models/birch-tree.glb")),
            // Put under a placed model in the editor, so the scene keeps it.
            info(6, "Sign", Some(2), true, None),
        ];
        let ids = |args: Value| -> Vec<u64> {
            filter(&entities, &args)
                .iter()
                .map(|entity| entity.id)
                .collect()
        };
        assert_eq!(ids(json!({})), [1, 2, 5, 6]);
        assert_eq!(ids(json!({"parts": true})).len(), 6);
        assert_eq!(ids(json!({"name": "OAK"})), [2]);
        assert_eq!(ids(json!({"asset": "tree"})), [2, 5]);
        assert_eq!(ids(json!({"parent": 1})), [2, 5]);
    }

    #[test]
    fn names_a_failed_command_by_its_index() {
        let asked: Vec<Asked> = vec![(7, None), (8, None)];
        let reports = vec![
            json!({"type": "spawned", "id": 3}),
            json!({"type": "done", "seq": 7, "error": null}),
            json!({"type": "done", "seq": 8, "error": "nope"}),
        ];
        assert_eq!(
            tidy(reports, &asked),
            [
                json!({"type": "spawned", "id": 3}),
                json!({"type": "done", "command": 1, "error": "nope"}),
            ]
        );
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
