//! The AI agent the editor's prompts go to. Each provider's command-line tool
//! runs headless in the project, either logged in on its own or with an API
//! key the editor keeps in the Windows Credential Manager, never in a file.
//! The Agent tab picks the provider and model and sends the prompt.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::mpsc::{self, Receiver},
    thread,
    time::SystemTime,
};

use eframe::egui::{self, Color32, Key, RichText, Ui, collapsing_header::CollapsingState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use windows::{
    Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    },
    core::{HSTRING, PWSTR},
};

use crate::{
    i18n::t,
    organization::{self, OrgPanel},
    panels,
    process::{self, CliProcess, Output},
};

/// Tells Claude why the `besfa mcp` warning against running a game beside
/// the editor does not hold here. An installed `besfa` CLI keeps the
/// warning it was built with, so the editor says it outright.
const AGENT_WINDOW: &str = "You run headless from the Besfa editor's Agent window, which has this game project open. The editor started this task with its scene saved, and when the task ends it reloads the scene file unless someone changed the scene in the editor meanwhile. So where the besfa start tool says not to run a game on a project the editor has open, that does not apply to you: if you have the besfa tools, you may start the game with them and save the scene through it.";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Provider {
    #[default]
    Claude,
    OpenAi,
    Gemini,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Claude, Provider::OpenAi, Provider::Gemini];

    pub fn label(self) -> &'static str {
        match self {
            Provider::Claude => "Claude",
            Provider::OpenAi => "ChatGPT",
            Provider::Gemini => "Gemini",
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::OpenAi => "codex",
            Provider::Gemini => "gemini",
        }
    }

    /// The variable the CLI reads an API key from. `codex exec` ignores
    /// `OPENAI_API_KEY`.
    fn key_env(self) -> &'static str {
        match self {
            Provider::Claude => "ANTHROPIC_API_KEY",
            Provider::OpenAi => "CODEX_API_KEY",
            Provider::Gemini => "GEMINI_API_KEY",
        }
    }

    /// Models to offer besides the CLI's default. Claude's aliases follow
    /// its latest models; the others are left to the CLI's own setting.
    pub fn models(self) -> &'static [&'static str] {
        match self {
            Provider::Claude => &["fable", "opus", "sonnet", "haiku"],
            Provider::OpenAi | Provider::Gemini => &[],
        }
    }

    /// Headless, with the prompt on stdin. Codex and Gemini may edit files
    /// in the project without asking and do nothing more, since nobody can
    /// answer their permission prompts. Claude decides by the project's
    /// `Permission`, may also do what `invocation` allows from its
    /// `ProjectAccess`, and asks about the rest on stdout as JSON events,
    /// with the prompt and the answers going in on stdin (see `Ask`).
    fn args(self) -> &'static [&'static str] {
        match self {
            Provider::Claude => &[
                "-p",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
                "--permission-prompt-tool",
                "stdio",
                "--append-system-prompt",
                AGENT_WINDOW,
            ],
            Provider::OpenAi => &[
                "exec",
                "--sandbox",
                "workspace-write",
                "--skip-git-repo-check",
            ],
            // The prompt flag makes it headless; stdin comes before it.
            Provider::Gemini => &["--prompt", "", "--approval-mode", "auto_edit"],
        }
    }

    /// Whether the CLI is logged in on its own. Run off the UI thread.
    fn check(self) -> Cli {
        if process::find_program_on_path(self.command()).is_none() {
            return Cli::Missing;
        }
        let output = |args: &[&str]| {
            process::command(self.command())
                .args(args)
                .stdin(Stdio::null())
                .output()
                .ok()
        };
        let logged_in = match self {
            Provider::Claude => output(&["auth", "status"]).is_some_and(|output| {
                serde_json::from_slice::<Value>(&output.stdout)
                    .is_ok_and(|status| status["loggedIn"] == true)
            }),
            Provider::OpenAi => {
                output(&["login", "status"]).is_some_and(|output| output.status.success())
            }
            // ponytail: no status command; a saved Google login can still be
            // refused when the request goes out.
            Provider::Gemini => std::env::var("USERPROFILE")
                .is_ok_and(|home| Path::new(&home).join(".gemini/oauth_creds.json").is_file()),
        };
        if logged_in {
            Cli::LoggedIn
        } else {
            Cli::LoggedOut
        }
    }
}

/// Where a provider's command-line tool stands on this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cli {
    Checking,
    Missing,
    LoggedOut,
    LoggedIn,
}

/// How a prompt reaches the provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// The CLI's own login.
    Cli,
    /// The CLI with this API key.
    Key(String),
}

/// How the user chose to reach a provider.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Access {
    #[default]
    Cli,
    Key,
}

/// The route `access` takes, or what it still needs. A key goes through the
/// CLI too, so the CLI must be installed either way.
fn route(access: Access, cli: Cli, key: &str) -> Result<Route, &'static str> {
    match (access, cli) {
        (_, Cli::Checking) => Err(t("Checking the command-line tool.")),
        (_, Cli::Missing) => Err(t("Install its command-line tool first.")),
        (Access::Cli, Cli::LoggedIn) => Ok(Route::Cli),
        (Access::Cli, Cli::LoggedOut) => Err(t(
            "Log in to its command-line tool, or switch it to an API key in Settings > Agent Settings.",
        )),
        (Access::Key, _) if key.trim().is_empty() => {
            Err(t("Enter its API key in Settings > Agent Settings."))
        }
        (Access::Key, _) => Ok(Route::Key(key.trim().into())),
    }
}

/// The program, arguments and environment that send a prompt with `files`
/// attached, allowing what `access` holds.
pub fn invocation(
    provider: Provider,
    route: &Route,
    model: &str,
    files: &[PathBuf],
    access: &ProjectAccess,
) -> (String, Vec<String>, Vec<(String, String)>) {
    let mut args: Vec<String> = provider.args().iter().map(|arg| arg.to_string()).collect();
    let rules = access.rules();
    let mut env = match route {
        Route::Cli => Vec::new(),
        Route::Key(key) => vec![(provider.key_env().into(), key.clone())],
    };
    if provider == Provider::Claude {
        args.extend(["--permission-mode".into(), access.permission.mode().into()]);
    }
    if provider == Provider::Claude && access.shell == Shell::PowerShell {
        // Bash off and PowerShell on. Claude turns PowerShell off along
        // with Bash unless the variable is set.
        args.extend(["--disallowedTools".into(), "Bash".into()]);
        env.push(("CLAUDE_CODE_USE_POWERSHELL_TOOL".into(), "1".into()));
    }
    if provider == Provider::Claude && !rules.is_empty() {
        // The list takes every value up to the next flag.
        args.push("--allowedTools".into());
        args.extend(rules);
    }
    if !model.is_empty() {
        args.extend(["--model".into(), model.into()]);
    }
    // Claude and Gemini keep to the project unless told about the attached
    // files' folders. Codex reads anywhere, but sees an image only through
    // --image.
    let mut folders: Vec<&Path> = files.iter().filter_map(|file| file.parent()).collect();
    folders.sort();
    folders.dedup();
    for folder in folders {
        let flag = match provider {
            Provider::Claude => "--add-dir",
            Provider::Gemini => "--include-directories",
            Provider::OpenAi => continue,
        };
        args.extend([flag.into(), folder.display().to_string()]);
    }
    if provider == Provider::OpenAi {
        // Codex splits --image at commas; such a file goes by path alone.
        let images = files
            .iter()
            .filter(|file| is_image(file) && !file.to_string_lossy().contains(','));
        for file in images {
            args.push(format!("--image={}", file.display()));
        }
    }
    (provider.command().into(), args, env)
}

fn is_image(file: &Path) -> bool {
    file.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["png", "jpg", "jpeg", "gif", "webp"]
                .iter()
                .any(|image| extension.eq_ignore_ascii_case(image))
        })
}

/// What Claude may do in a project besides editing its files. The user's
/// settings keep it, not the project, so a copy of the project allows none
/// of it until its own user says so.
/// ponytail: Claude only; Codex's sandbox and Gemini's approval mode need
/// flags of their own.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct ProjectAccess {
    /// The .mcp.json servers whose tools it may call. Headless, Claude
    /// starts every server there unasked, but calls only these.
    pub servers: BTreeSet<String>,
    /// Whether it may run git and gh, to commit, push and open pull requests.
    pub github: bool,
    /// Whether Organization runs review and merge the pull requests they
    /// open, with `github` on. Only their prompt changes: `gh *` allows it.
    pub merge: bool,
    /// Commands it may run, one per line, as the shell's rules take them:
    /// `cargo *` is cargo with any arguments.
    pub commands: String,
    /// The shell tool Claude runs commands in. A project's CCGS roles that
    /// list Bash in their `tools:` need Bash.
    pub shell: Shell,
    /// How Claude decides about what the rules above do not allow.
    pub permission: Permission,
}

/// How Claude decides about a tool call its allow rules do not cover.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Permission {
    /// Claude's classifier lets safe calls through and asks about the rest.
    #[default]
    Auto,
    /// Edits files without asking; asks about commands and other tools.
    AcceptEdits,
    /// Asks before every edit, command and tool.
    Manual,
}

impl Permission {
    /// Claude's `--permission-mode` for it.
    fn mode(self) -> &'static str {
        match self {
            Permission::Auto => "auto",
            Permission::AcceptEdits => "acceptEdits",
            Permission::Manual => "manual",
        }
    }
}

/// The shell tool Claude runs commands in.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    PowerShell,
    Bash,
}

impl Default for Shell {
    /// PowerShell on Windows, where Bash is not the shell; Bash elsewhere.
    fn default() -> Self {
        if cfg!(windows) {
            Shell::PowerShell
        } else {
            Shell::Bash
        }
    }
}

impl Shell {
    /// The name of Claude's tool for this shell, as its rules write it.
    fn tool(self) -> &'static str {
        match self {
            Shell::PowerShell => "PowerShell",
            Shell::Bash => "Bash",
        }
    }
}

impl Default for ProjectAccess {
    /// The game's own `besfa mcp`, which builds and runs the game, so code
    /// Claude edited runs too.
    fn default() -> Self {
        Self {
            servers: BTreeSet::from(["besfa".to_string()]),
            github: false,
            merge: false,
            commands: String::new(),
            shell: Shell::default(),
            permission: Permission::default(),
        }
    }
}

impl ProjectAccess {
    /// Claude's allow rules for it. The project's own deny rules still win,
    /// but only over commands they match as written.
    fn rules(&self) -> Vec<String> {
        let mut rules: Vec<String> = self
            .servers
            .iter()
            .map(|server| {
                // Claude's tool names keep only these characters of a
                // server's name.
                let name: String = server
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || "_-".contains(c) {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                format!("mcp__{name}")
            })
            .collect();
        let shell = self.shell.tool();
        if self.github {
            rules.extend([format!("{shell}(git *)"), format!("{shell}(gh *)")]);
        }
        rules.extend(
            self.commands
                .lines()
                .map(str::trim)
                .filter(|command| !command.is_empty())
                // One written as a rule, like `PowerShell(cargo *)` or one
                // copied from a Claude settings file, stays one.
                .map(|command| {
                    let rule = command.ends_with(')')
                        && command.split_once('(').is_some_and(|(tool, _)| {
                            tool.starts_with(|c: char| c.is_ascii_uppercase())
                                && tool.chars().all(|c| c.is_ascii_alphanumeric())
                        });
                    if rule {
                        command.into()
                    } else {
                        format!("{shell}({command})")
                    }
                }),
        );
        rules
    }

    /// Allows from now on what "Always allow" on `ask` allows.
    fn allow(&mut self, ask: &Ask) {
        match ask.rule() {
            Some(Rule::Server(server)) => {
                self.servers.insert(server);
            }
            Some(Rule::Command(line, _))
                if !self.commands.lines().any(|old| old.trim() == line) =>
            {
                if !self.commands.is_empty() && !self.commands.ends_with('\n') {
                    self.commands.push('\n');
                }
                self.commands.push_str(&line);
                self.commands.push('\n');
            }
            Some(Rule::Command(..)) | None => {}
        }
    }
}

/// What a Claude run asks the user before it uses a tool its rules do not
/// allow. It waits for the answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    id: String,
    pub tool: String,
    input: Value,
}

/// What "Always allow" saves for an ask.
#[derive(Debug, PartialEq)]
enum Rule {
    /// A line for `ProjectAccess::commands`, and the same for Claude's
    /// rule: the command's program with any arguments.
    Command(String, String),
    /// A .mcp.json server, every tool of it.
    Server(String),
}

impl Ask {
    /// The ask `line` is, if it is one: a `can_use_tool` control request of
    /// Claude's stream-json output.
    pub fn read(line: &str) -> Option<Self> {
        let event: Value = serde_json::from_str(line).ok()?;
        let request = &event["request"];
        if event["type"] != "control_request" || request["subtype"] != "can_use_tool" {
            return None;
        }
        Some(Self {
            id: event["request_id"].as_str()?.into(),
            tool: request["tool_name"].as_str()?.into(),
            input: request["input"].clone(),
        })
    }

    /// The request it came in, which its answer names.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// What it would do: the command it runs, the file or address it
    /// reaches, or else all it was given.
    pub fn what(&self) -> String {
        ["command", "file_path", "path", "url", "pattern"]
            .iter()
            .find_map(|key| self.input[key].as_str())
            .map_or_else(|| self.input.to_string(), str::to_string)
    }

    fn rule(&self) -> Option<Rule> {
        if let Some(server) = self
            .tool
            .strip_prefix("mcp__")
            .and_then(|rest| rest.split_once("__"))
        {
            return Some(Rule::Server(server.0.into()));
        }
        // ponytail: the first word only, so `cd x && cargo build` saves
        // `cd *`; the user can write the rule in Settings instead.
        let program = self.input["command"].as_str()?.split_whitespace().next()?;
        let content = format!("{program} *");
        let line = if self.tool == "Bash" {
            content.clone()
        } else {
            format!("{}({content})", self.tool)
        };
        Some(Rule::Command(line, content))
    }

    /// Claude's allow rule for what "Always allow" saves, if anything.
    pub fn allowed(&self) -> Option<String> {
        match self.rule()? {
            Rule::Command(_, content) => Some(format!("{}({content})", self.tool)),
            Rule::Server(server) => Some(format!("mcp__{server}")),
        }
    }

    /// What "Always allow" saves, as the button shows it, if anything.
    pub fn saves(&self) -> Option<String> {
        match self.rule()? {
            Rule::Command(line, _) => Some(line),
            Rule::Server(server) => Some(format!("{server} MCP")),
        }
    }

    /// The line that answers it. One allowed `always` also stops the run
    /// from asking again about what that saves.
    pub fn answer(&self, allow: bool, always: bool) -> String {
        let response = if allow {
            let mut response = serde_json::json!({"behavior": "allow", "updatedInput": self.input});
            let rule = match self.rule().filter(|_| always) {
                Some(Rule::Command(_, content)) => {
                    Some(serde_json::json!({"toolName": self.tool, "ruleContent": content}))
                }
                Some(Rule::Server(server)) => {
                    Some(serde_json::json!({"toolName": format!("mcp__{server}")}))
                }
                None => None,
            };
            if let Some(rule) = rule {
                response["updatedPermissions"] = serde_json::json!([{
                    "type": "addRules",
                    "rules": [rule],
                    "behavior": "allow",
                    "destination": "session",
                }]);
            }
            response
        } else {
            serde_json::json!({"behavior": "deny", "message": "The user said no."})
        };
        serde_json::json!({
            "type": "control_response",
            "response": {"subtype": "success", "request_id": self.id, "response": response},
        })
        .to_string()
    }
}

/// Sends `input`, the prompt, to a run of `program`. Claude takes it as a
/// stream-json message and keeps reading for the answers to its asks until
/// `ended` says its result came.
pub fn send_prompt(process: &mut CliProcess, program: &str, input: String) {
    if program == Provider::Claude.command() {
        let message = serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": input},
        });
        process.send_first(message.to_string());
    } else {
        process.send_all(input);
    }
}

/// Whether `line` is Claude's result, after which it reads nothing more.
pub fn ended(line: &str) -> bool {
    serde_json::from_str::<Value>(line).is_ok_and(|event| event["type"] == "result")
}

/// `project` as one key however it is written: Windows ignores case, takes
/// either slash, and the path every Agent window tab passes ends in one.
fn project_key(project: &Path) -> String {
    project
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// The provider and model the Agent tab last picked, each provider's model
/// (empty for the CLI's default), how each provider is reached, and what
/// the agents may use in each project.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct AgentSettings {
    pub provider: Provider,
    pub models: BTreeMap<Provider, String>,
    pub access: BTreeMap<Provider, Access>,
    /// By `project_key`; a project missing here has the defaults.
    pub projects: BTreeMap<String, ProjectAccess>,
}

impl AgentSettings {
    /// The settings file, next to the recent projects list.
    pub fn file() -> PathBuf {
        let base = std::env::var("APPDATA").or_else(|_| std::env::var("HOME"));
        Path::new(&base.unwrap_or_else(|_| ".".into())).join("Besfa/agent.ron")
    }

    /// What `file` keeps, or the defaults when it keeps nothing usable.
    pub fn read(file: &Path) -> Self {
        fs::read_to_string(file)
            .ok()
            .and_then(|text| ron::from_str::<Self>(&text).ok())
            .unwrap_or_default()
    }

    pub fn write(&self, file: &Path) -> Result<(), String> {
        if let Some(folder) = file.parent() {
            fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        let text = ron::to_string(self).map_err(|error| error.to_string())?;
        fs::write(file, text).map_err(|error| error.to_string())
    }

    pub fn model(&self, provider: Provider) -> &str {
        self.models.get(&provider).map_or("", String::as_str)
    }

    pub fn access(&self, provider: Provider) -> Access {
        self.access.get(&provider).copied().unwrap_or_default()
    }

    /// What the agents may use in `project`.
    pub fn project(&self, project: &Path) -> ProjectAccess {
        self.projects
            .get(&project_key(project))
            .cloned()
            .unwrap_or_default()
    }

    /// Allows the agents in `project` from now on what "Always allow" on
    /// `ask` allows.
    pub fn allow(&mut self, project: &Path, ask: &Ask) {
        let mut access = self.project(project);
        access.allow(ask);
        self.projects.insert(project_key(project), access);
    }
}

/// Each provider's CLI state and saved API key, and the GitHub CLI's state.
pub struct Agents {
    cli: BTreeMap<Provider, Cli>,
    keys: BTreeMap<Provider, String>,
    checks: Option<Receiver<(Provider, Cli)>>,
    /// Where `gh` stands, for the agents' GitHub access.
    gh: Cli,
    gh_check: Option<Receiver<Cli>>,
    /// Why the last key could not be saved.
    key_error: Option<String>,
}

impl Agents {
    /// The saved keys, with the CLIs still to check.
    pub fn load() -> Self {
        Self {
            cli: Provider::ALL
                .map(|provider| (provider, Cli::Checking))
                .into(),
            keys: Provider::ALL
                .map(|provider| {
                    (
                        provider,
                        read_secret(&key_target(provider)).unwrap_or_default(),
                    )
                })
                .into(),
            checks: None,
            gh: Cli::Checking,
            gh_check: None,
            key_error: None,
        }
    }

    /// Checks every CLI again in the background.
    pub fn check(&mut self, ctx: &egui::Context) {
        let (sender, receiver) = mpsc::channel();
        for provider in Provider::ALL {
            self.cli.insert(provider, Cli::Checking);
            let (sender, ctx) = (sender.clone(), ctx.clone());
            thread::spawn(move || {
                let _ = sender.send((provider, provider.check()));
                ctx.request_repaint();
            });
        }
        self.checks = Some(receiver);
        self.gh = Cli::Checking;
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = sender.send(check_gh());
            ctx.request_repaint();
        });
        self.gh_check = Some(receiver);
    }

    /// Agents whose CLIs stand as `cli`, in `Provider::ALL` order, with
    /// `keys` saved; they are never checked.
    #[cfg(test)]
    pub fn fixed(cli: [Cli; 3], keys: &[(Provider, &str)]) -> Self {
        Self {
            cli: Provider::ALL.into_iter().zip(cli).collect(),
            keys: keys
                .iter()
                .map(|(provider, key)| (*provider, key.to_string()))
                .collect(),
            checks: Some(mpsc::channel().1),
            gh: Cli::LoggedIn,
            gh_check: None,
            key_error: None,
        }
    }

    /// Checks the CLIs unless they were checked already.
    pub fn check_once(&mut self, ctx: &egui::Context) {
        if self.checks.is_none() {
            self.check(ctx);
        }
    }

    pub fn poll(&mut self) {
        if let Some(checks) = &self.checks {
            self.cli.extend(checks.try_iter());
        }
        if let Some(gh) = self
            .gh_check
            .as_ref()
            .and_then(|check| check.try_iter().last())
        {
            self.gh = gh;
        }
    }

    fn cli(&self, provider: Provider) -> Cli {
        self.cli.get(&provider).copied().unwrap_or(Cli::Checking)
    }

    pub fn route(&self, provider: Provider, access: Access) -> Result<Route, &'static str> {
        route(
            access,
            self.cli(provider),
            self.keys.get(&provider).map_or("", String::as_str),
        )
    }

    /// Whether the provider can take a prompt the way it is reached, once
    /// its CLI is checked.
    fn ready(&self, provider: Provider, access: Access) -> Option<bool> {
        match self.cli(provider) {
            Cli::Checking => None,
            _ => Some(self.route(provider, access).is_ok()),
        }
    }
}

/// Where the GitHub CLI stands. Run off the UI thread.
fn check_gh() -> Cli {
    if process::find_program_on_path("gh").is_none() {
        return Cli::Missing;
    }
    let status = process::command("gh")
        .args(["auth", "status"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if status.is_ok_and(|status| status.success()) {
        Cli::LoggedIn
    } else {
        Cli::LoggedOut
    }
}

fn key_target(provider: Provider) -> String {
    format!("Besfa/agent/{}", provider.command())
}

/// A generic credential's secret, if the Credential Manager has one.
fn read_secret(target: &str) -> Option<String> {
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    unsafe {
        CredReadW(
            &HSTRING::from(target),
            CRED_TYPE_GENERIC,
            None,
            &mut credential,
        )
        .ok()?;
        let blob = std::slice::from_raw_parts(
            (*credential).CredentialBlob,
            (*credential).CredentialBlobSize as usize,
        );
        let secret = String::from_utf8(blob.to_vec()).ok();
        CredFree(credential as *const _);
        secret
    }
}

/// Saves `secret` under `target` for this Windows user, or removes it when
/// empty.
fn write_secret(target: &str, secret: &str) -> Result<(), String> {
    if secret.is_empty() {
        // Nothing saved is as good as removed.
        let _ = unsafe { CredDeleteW(&HSTRING::from(target), CRED_TYPE_GENERIC, None) };
        return Ok(());
    }
    let mut name: Vec<u16> = target.encode_utf16().chain([0]).collect();
    let mut blob = secret.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    unsafe { CredWriteW(&credential, 0) }.map_err(|error| error.to_string())
}

/// Shows the settings window while `open`: every provider, how it is
/// reached, and only what that way needs. Returns true when `settings`
/// changed, so the caller saves them.
pub fn window(
    ctx: &egui::Context,
    open: &mut bool,
    settings: &mut AgentSettings,
    agents: &mut Agents,
) -> bool {
    let mut changed = false;
    egui::Window::new(t("Agent Settings"))
        .id(egui::Id::new("agent_settings"))
        .open(open)
        .resizable(false)
        .show(ctx, |ui| changed = show(ui, settings, agents));
    changed
}

fn show(ui: &mut Ui, settings: &mut AgentSettings, agents: &mut Agents) -> bool {
    let before = settings.clone();
    ui.set_width(420.0);
    for provider in Provider::ALL {
        let mut access = settings.access(provider);
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // Padding to a fixed width lines the toggles up.
                let name = ui.strong(provider.label());
                ui.add_space((70.0 - name.rect.width()).max(0.0));
                ui.selectable_value(&mut access, Access::Cli, "CLI");
                ui.selectable_value(&mut access, Access::Key, t("API key"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match agents.ready(provider, access) {
                        None => ui.colored_label(panels::MUTED, t("checking")),
                        Some(true) => ui.colored_label(panels::GOOD, t("ready")),
                        Some(false) => ui.colored_label(panels::WARNING, t("can't send yet")),
                    };
                });
            });
            let command = provider.command();
            let note = |ui: &mut Ui, text: String| {
                ui.label(RichText::new(text).color(panels::MUTED).small());
            };
            match (access, agents.cli(provider)) {
                (_, Cli::Checking) => note(ui, t("Checking `{}`.").replace("{}", command)),
                (_, Cli::Missing) => note(ui, t("`{}` is not installed.").replace("{}", command)),
                (Access::Cli, Cli::LoggedOut) => note(
                    ui,
                    t("`{}` is not logged in. Run `{}` in a terminal to log in.")
                        .replace("{}", command),
                ),
                (Access::Cli, Cli::LoggedIn) => {
                    note(ui, t("`{}` is logged in.").replace("{}", command))
                }
                (Access::Key, _) => {}
            }
            if access == Access::Key {
                let key = agents.keys.entry(provider).or_default();
                let field = ui.add(
                    egui::TextEdit::singleline(key)
                        .password(true)
                        .hint_text(t("{} API key").replace("{}", provider.label()))
                        .desired_width(f32::INFINITY),
                );
                if field.lost_focus() {
                    agents.key_error = write_secret(&key_target(provider), key.trim()).err();
                }
            }
        });
        settings.access.insert(provider, access);
    }
    if let Some(error) = &agents.key_error {
        ui.colored_label(panels::ERROR, format!("Could not save the key: {error}"));
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t("Keys stay in the Windows Credential Manager."))
                .color(panels::MUTED)
                .small(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(t("Check again")).clicked() {
                agents.check(ui.ctx());
            }
        });
    });
    *settings != before
}

/// The Agent window's Settings tab: how each provider is reached, and what
/// the agents may use in the project in `directory`. Returns true when
/// `settings` changed, so the caller saves them.
pub fn settings_tab(
    ui: &mut Ui,
    settings: &mut AgentSettings,
    agents: &mut Agents,
    directory: &Path,
) -> bool {
    let mut changed = false;
    egui::ScrollArea::vertical().show(ui, |ui| {
        changed = show(ui, settings, agents);
        ui.add_space(8.0);
        let before = settings.project(directory);
        let mut access = before.clone();
        let note = |ui: &mut Ui, text: &str| {
            ui.label(RichText::new(text).color(panels::MUTED).small());
        };
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.strong("GitHub");
            ui.checkbox(
                &mut access.github,
                t("Let Claude commit, push and open pull requests"),
            );
            note(ui, t("It then runs git and gh without asking, and gh reaches every repository your GitHub login can. Deny rules in the project's .claude/settings.json still win, but only over commands they match as written, so block force pushes with branch protection on GitHub."));
            ui.add_enabled_ui(access.github, |ui| {
                ui.checkbox(
                    &mut access.merge,
                    t("Organization runs review and merge their pull requests"),
                );
            });
            note(ui, t("An agent that opens a pull request reviews its diff, fixes what is wrong and merges it once its checks pass, or right away without any, then closes the issue it finished. It is told never to push a refused merge through with --admin, but only a branch rule that also applies to administrators enforces that. A rule in the project's CLAUDE.md or .claude/agent-memory that says never to merge can still stop them."));
            let gh = match agents.gh {
                Cli::Checking => t("Checking `{}`.").replace("{}", "gh"),
                Cli::Missing => t("`{}` is not installed.").replace("{}", "gh"),
                Cli::LoggedOut => t("`{}` is not logged in. Run `{}` in a terminal to log in.")
                    .replacen("{}", "gh", 1)
                    .replacen("{}", "gh auth login", 1),
                Cli::LoggedIn => t("`{}` is logged in.").replace("{}", "gh"),
            };
            let color = match agents.gh {
                Cli::Missing | Cli::LoggedOut => panels::WARNING,
                _ => panels::MUTED,
            };
            ui.label(RichText::new(gh).color(color).small());
        });
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.strong(t("Permissions"));
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(&mut access.permission, Permission::Auto, t("Auto"));
                ui.radio_value(
                    &mut access.permission,
                    Permission::AcceptEdits,
                    t("Accept edits"),
                );
                ui.radio_value(&mut access.permission, Permission::Manual, t("Ask every time"));
            });
            let note_text = match access.permission {
                Permission::Auto => t("Claude decides on its own which edits and commands to run without asking, and may refuse the rest or ask about them in the chat. What it judges safe can include commands you would want to check, so keep branch protection on in GitHub."),
                Permission::AcceptEdits => t("Claude edits the project's files without asking, and asks in the chat before commands and other tools the rules below do not allow."),
                Permission::Manual => t("Claude asks in the chat before every edit, command and tool the rules below do not allow."),
            };
            note(ui, note_text);
            note(ui, t("Only Claude follows this choice. Codex and Gemini edit files without asking and do nothing more."));
        });
        if cfg!(windows) {
            ui.group(|ui| {
                ui.set_width(ui.available_width());
                ui.strong(t("Shell"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut access.shell, Shell::PowerShell, "PowerShell");
                    ui.radio_value(&mut access.shell, Shell::Bash, "Bash");
                });
                note(ui, t("The shell Claude runs commands in: PowerShell is this PC's. Bash turns PowerShell off. A CCGS role runs its shell commands only if its role file's `tools:` lists Bash, so pick Bash for a project that uses such roles."));
            });
        }
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.strong(t("Commands"));
            ui.add(
                egui::TextEdit::multiline(&mut access.commands)
                    .code_editor()
                    .hint_text("cargo *")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            note(ui, t("Commands Claude may run without asking, in the chat and in Organization runs, one per line; * stands for any arguments. Agents need the ones that build and test the game, like cargo *, to finish their work. Claude asks in the chat before running others, and Always allow adds them here. They are rules for the shell above; a line like PowerShell(cargo *) is kept as written for that tool, and a line like Bash(cargo *) is never used while PowerShell is picked. An agent playing a CCGS role runs them only if its role file lists Bash."));
        });
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            let file = directory.join(".mcp.json");
            ui.horizontal(|ui| {
                ui.strong(t("MCP servers"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t("Open .mcp.json")).clicked() {
                        if !file.exists() {
                            let _ = fs::write(&file, "{\n  \"mcpServers\": {}\n}\n");
                        }
                        crate::project::open_url(&file.display().to_string());
                    }
                });
            });
            note(ui, t("The servers in the project's .mcp.json. Claude may call the tools of those checked."));
            let mut listed = Vec::new();
            match mcp_servers(&file) {
                Err(error) => {
                    ui.colored_label(
                        panels::ERROR,
                        t("Could not read .mcp.json: {}").replace("{}", &error),
                    );
                }
                Ok(servers) if servers.is_empty() => note(ui, t("It lists no servers.")),
                Ok(servers) => {
                    for (name, start) in servers {
                        listed.push(name.clone());
                        let mut on = access.servers.contains(&name);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut on, &name).changed() {
                                if on {
                                    access.servers.insert(name.clone());
                                } else {
                                    access.servers.remove(&name);
                                }
                            }
                            ui.add(
                                egui::Label::new(RichText::new(start).color(panels::MUTED).small())
                                    .truncate(),
                            );
                        });
                    }
                }
            }
            // Ones .mcp.json lacks, as a server of the user's own Claude
            // settings that Always allow in the chat saved, stay visible
            // to take back.
            let others: Vec<String> = access
                .servers
                .iter()
                .filter(|server| !listed.contains(server))
                .cloned()
                .collect();
            for name in others {
                let mut on = true;
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut on, &name).changed() {
                        access.servers.remove(&name);
                    }
                    let why = t("Not in .mcp.json");
                    ui.label(RichText::new(why).color(panels::MUTED).small());
                });
            }
            note(ui, t("Codex and Gemini do not read .mcp.json. An Organization agent playing a CCGS role uses only the tools its role file lists."));
        });
        note(ui, t("These choices stay on this computer, not in the project, so a copy of it starts with GitHub off."));
        if access != before {
            settings.projects.insert(project_key(directory), access);
            changed = true;
        }
    });
    changed
}

/// Each server in the .mcp.json `file`, with what starts it, or why the
/// file could not be read. No file lists none.
fn mcp_servers(file: &Path) -> Result<Vec<(String, String)>, String> {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let config: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let Some(servers) = config["mcpServers"].as_object() else {
        return Ok(Vec::new());
    };
    Ok(servers
        .iter()
        .map(|(name, server)| {
            let start = match server["url"].as_str() {
                Some(url) => url.to_string(),
                None => std::iter::once(&server["command"])
                    .chain(server["args"].as_array().into_iter().flatten())
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
            };
            (name.clone(), start)
        })
        .collect())
}

/// One turn of a chat: what the user sent, or what an agent printed back.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Message {
    pub user: bool,
    pub text: String,
    /// What the agent thought and did on the way, folded under its name.
    pub thoughts: String,
    /// The organization agent that said it, and its color as RGB.
    pub speaker: Option<(String, [u8; 3])>,
    /// The organization the chat's agent handed its task to. Its agents
    /// answer in the messages after this one, shown as a child session.
    pub child: Option<String>,
}

/// Where the organization run answering in the shown chat stands, until it
/// is done.
#[derive(Clone, Copy, PartialEq)]
enum Live {
    Working,
    Paused,
}

/// What a chat agent is told about `lineup`, the project's organizations,
/// so it can hand a task that needs a team to one of them. Empty when there
/// is none to hand to.
fn handoff_note(lineup: &str) -> String {
    if lineup.is_empty() {
        return String::new();
    }
    format!(
        "\n\n---\nFrom the Besfa editor: this project also has organizations, teams of agents the editor runs on a task, each in its role and each taking the reports of the agents linked before it:\n{lineup}\nDo the task yourself when one agent can do it well. Hand it to an organization only when it needs several of one organization's roles working together, such as design, code and testing, since every agent in it runs and a run costs that many times what you do. To hand it over, do none of the work yourself: answer with a brief for the team that stands on its own, saying what to do, what the user said that matters and what done looks like, and end with a last line `ORGANIZATION: <name>` naming one of the organizations above. The editor then runs that organization on your brief in this chat."
    )
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
struct Chat {
    /// Set once something is sent, so a paused organization run can find
    /// the chat it answers in.
    id: u64,
    messages: Vec<Message>,
    /// Every file sent in this chat. The resent chat names them all, so the
    /// CLI needs to reach them all.
    files: Vec<PathBuf>,
}

impl Chat {
    fn title(&self) -> String {
        self.messages
            .iter()
            .find(|message| message.user)
            .and_then(|message| message.text.lines().next())
            .map_or_else(
                || t("New chat").into(),
                |line| line.chars().take(30).collect(),
            )
    }

    /// What goes to the CLI for `prompt`: each run starts fresh, so the
    /// chat so far comes first. ponytail: the whole chat goes every time;
    /// resume the CLI's own session if long chats grow too big.
    fn input(&self, prompt: &str) -> String {
        if self.messages.is_empty() {
            return prompt.into();
        }
        let mut input = String::from("Earlier in this conversation:\n\n");
        for message in &self.messages {
            let who = match &message.speaker {
                _ if message.user => "User",
                Some((name, _)) => name,
                None => "You",
            };
            input.push_str(&format!("{who}: {}\n\n", message.text));
        }
        input.push_str(&format!("Now the user says:\n\n{prompt}"));
        input
    }
}

/// The Agent window: past chats on the left, the chat in the middle, and the
/// prompt card below it.
#[derive(Default)]
pub struct AgentPanel {
    prompt: String,
    /// The organization prompts go to, in place of the picked model.
    organization: Option<String>,
    /// Continue was clicked: the paused organization run goes on.
    continuing: bool,
    run: Option<CliProcess>,
    chat: Chat,
    /// Earlier chats, newest last.
    history: Vec<Chat>,
    /// What the chats looked like when last written, to write again only
    /// once they change.
    kept: (usize, u64, usize, usize),
    sidebar_closed: bool,
    /// Files to send with the next prompt, by path.
    attached: Vec<PathBuf>,
    /// The open file dialog's pick, once the user makes it.
    picking: Option<Receiver<Option<Vec<PathBuf>>>>,
    /// A file the run may change, and when it was changed before the run.
    watched: Option<(PathBuf, Option<SystemTime>)>,
    /// Whether the run prints Claude's stream-json events, not plain text.
    streamed: bool,
    /// What the run was sent, the chat so far and the user's prompt, when
    /// it was told it may hand its task to an organization; the team gets
    /// it too, as a task sent to it straight does.
    offered: Option<String>,
    /// The organization the last run handed its task to, and the task, for
    /// the editor to start.
    handoff: Option<(String, String)>,
    /// What the running agent, this chat's or an organization's, waits on
    /// the user to allow, oldest first.
    asks: Vec<Ask>,
    /// Those the user answered, whether allowed, and whether always, for
    /// the editor to send and save.
    answered: Vec<(Ask, bool, bool)>,
    /// Laid-out code blocks and the like of the replies' Markdown.
    markdown: egui_commonmark::CommonMarkCache,
    /// Each unit's lengths, spinner, live state and height when it was
    /// last drawn, by its first message. Those scrolled far out of view
    /// are not drawn again.
    heights: Vec<Option<((usize, usize, bool, u8), f32)>>,
    /// The width `heights` were measured at.
    heights_width: f32,
}

impl AgentPanel {
    pub fn running(&self) -> bool {
        self.run.is_some()
    }

    /// Where the chats of `project` are kept: under the user, like the
    /// agent settings, so they stay out of the project's commits. The
    /// folder's name for reading, then a hash of the project's key, FNV-1a
    /// so it stays the same across Rust versions, which tells `a_b` and
    /// `a\b` apart.
    pub fn chats_file(project: &Path) -> PathBuf {
        let key = project_key(project);
        let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
        let folder: String = key
            .rsplit('\\')
            .next()
            .unwrap_or_default()
            .chars()
            .filter(|c| c.is_alphanumeric() || "-_".contains(*c))
            .take(40)
            .collect();
        AgentSettings::file()
            .with_file_name("chats")
            .join(format!("{folder}-{hash:016x}.ron"))
    }

    /// A panel with the chats `file` keeps, the last one shown.
    pub fn read(file: &Path) -> Self {
        let mut panel = Self::default();
        if let Some((history, chat)) = fs::read_to_string(file)
            .ok()
            .and_then(|text| ron::from_str::<(Vec<Chat>, Chat)>(&text).ok())
        {
            panel.history = history;
            panel.chat = chat;
        }
        panel.kept = panel.shape();
        panel
    }

    /// Enough of the chats to tell they changed: how many there are, which
    /// one is shown, and how far it has got.
    fn shape(&self) -> (usize, u64, usize, usize) {
        let last = self
            .chat
            .messages
            .last()
            .map_or(0, |message| message.text.len() + message.thoughts.len());
        (
            self.history.len(),
            self.chat.id,
            self.chat.messages.len(),
            last,
        )
    }

    /// Writes the chats to `file` once they changed, unless an agent is
    /// still answering: its turn is written when it is done. A paused
    /// organization run keeps its own chat meanwhile.
    pub fn keep(&mut self, file: &Path, busy: bool) -> Result<(), String> {
        let shape = self.shape();
        if busy || shape == self.kept {
            return Ok(());
        }
        self.kept = shape;
        if let Some(folder) = file.parent() {
            fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        let text =
            ron::to_string(&(&self.history, &self.chat)).map_err(|error| error.to_string())?;
        // Whole or not at all: a file cut short would read as no chats and
        // be written over.
        let part = file.with_extension("ron.part");
        fs::write(&part, text).map_err(|error| error.to_string())?;
        fs::rename(&part, file).map_err(|error| error.to_string())
    }

    /// A panel with earlier chats and one whose agent handed its task to an
    /// organization, for the manual's screenshot.
    #[cfg(test)]
    pub fn demo() -> Self {
        let chat = |turns: &[(bool, &str)]| Chat {
            messages: turns
                .iter()
                .map(|(user, text)| Message {
                    user: *user,
                    text: text.to_string(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let mut panel = Self::default();
        panel.history = vec![
            chat(&[(true, "Make the terrain greener")]),
            chat(&[(true, "Add a spawn point for the zombie")]),
        ];
        panel.chat = chat(&[(true, "Add a double jump to the player.")]);
        panel.say(
            "This needs design, code and a test pass, so it goes to Studio.\n\nAdd a double jump to the player: decide its height and cost, implement it, and test it.",
        );
        panel.handed_off("Studio");
        let agent = |name: &str, color: [u8; 3], thoughts: &str, text: &str| Message {
            text: text.into(),
            thoughts: thoughts.into(),
            speaker: Some((name.into(), color)),
            ..Default::default()
        };
        panel.chat.messages.extend([
            agent(
                "Game Designer",
                [0x4F, 0x86, 0x5A],
                "[Read] design/gdd/movement.md",
                "A second jump in the air costs 10 stamina, and lands softer than the first.",
            ),
            agent(
                "Gameplay Programmer",
                [0x46, 0x6E, 0xA8],
                "[Grep] fn jump\n[Edit] src/player.rs",
                "",
            ),
        ]);
        panel
    }

    /// Runs `invocation` on `prompt` in `directory`, telling it it may hand
    /// the task to one of the organizations in `lineup`. `poll` tells once
    /// it exits whether it changed `watch`.
    pub fn start(
        &mut self,
        (program, args, env): (String, Vec<String>, Vec<(String, String)>),
        prompt: &str,
        directory: &Path,
        watch: PathBuf,
        lineup: &str,
        ctx: &egui::Context,
    ) {
        let note = handoff_note(lineup);
        let input = self.ask(prompt);
        self.offered = (!note.is_empty()).then(|| input.clone());
        let input = input + &note;
        self.chat.messages.push(Message::default());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let env: Vec<(&str, &str)> = env
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        match CliProcess::start(directory, &program, &args, &env, ctx) {
            Ok(mut run) => {
                send_prompt(&mut run, &program, input);
                self.streamed = program == Provider::Claude.command();
                self.watched = Some((watch.clone(), modified(&watch)));
                self.run = Some(run);
            }
            Err(error) => self.reply(&format!("Could not start {program}: {error}")),
        }
    }

    /// The organization prompts go to, if not the picked model.
    pub fn organization(&self) -> Option<&str> {
        self.organization.as_deref()
    }

    /// Adds the user's `prompt` to the chat and returns what goes to an
    /// agent for it.
    pub fn ask(&mut self, prompt: &str) -> String {
        if self.chat.id == 0 {
            // Unique enough to tell this project's chats apart.
            self.chat.id = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(1, |time| time.as_nanos() as u64);
        }
        let input = self.chat.input(prompt);
        self.chat.messages.push(Message {
            user: true,
            text: prompt.into(),
            ..Default::default()
        });
        input
    }

    /// The shown chat, for an organization run to answer in.
    pub fn chat_id(&self) -> u64 {
        self.chat.id
    }

    /// Shows the chat a paused organization run answers in, sending to that
    /// organization, as it was before the editor closed.
    pub fn restore(
        &mut self,
        (id, messages): (u64, &[Message]),
        files: &[PathBuf],
        organization: &str,
    ) {
        // A run the chat's agent handed over leaves the user's own pick, so
        // what they send next goes where it went before. Its marker follows
        // the prompt that started the run, the last one in the run's own copy
        // of the chat, since nobody can send while it runs; the saved copy
        // may hold later prompts, and the name may since have changed.
        let started = messages.iter().rposition(|m| m.user).map_or(0, |at| at + 1);
        let handed = messages[started..].iter().any(|m| m.child.is_some());
        let mut messages = messages.to_vec();
        // An agent the editor closed on said nothing yet.
        for message in &mut messages {
            if message.speaker.is_some() && message.text.is_empty() {
                message.text = "Stopped.".into();
            }
        }
        let mut chat = Chat {
            id,
            messages,
            files: files.to_vec(),
        };
        // The saved copy is the newer one if the user went on in that chat
        // after the run paused; the paused one if the editor closed on it.
        if let Some(saved) = std::iter::once(&self.chat)
            .chain(&self.history)
            .find(|saved| saved.id == id)
            && saved.messages.len() > chat.messages.len()
        {
            chat = saved.clone();
        }
        self.show_chat(chat);
        if !handed {
            self.organization = Some(organization.into());
        }
    }

    /// Whether Continue was clicked since the last call.
    pub fn take_continue(&mut self) -> bool {
        std::mem::take(&mut self.continuing)
    }

    /// The organization the run that just ended handed its task to, the
    /// task for it, and every file attached in the chat.
    pub fn take_handoff(&mut self) -> Option<(String, String, Vec<PathBuf>)> {
        let (organization, task) = self.handoff.take()?;
        Some((organization, task, self.chat.files.clone()))
    }

    /// Marks where `organization`'s agents, handed the task, start
    /// answering in the chat. ponytail: the agent that handed it over does
    /// not hear back; run it again on the team's reports if users want it
    /// to sum them up.
    pub fn handed_off(&mut self, organization: &str) {
        self.chat.messages.push(Message {
            text: format!("Handed the task to {organization}."),
            child: Some(organization.into()),
            ..Default::default()
        });
    }

    /// Adds a message from the editor, such as why the organization could
    /// not start.
    pub fn say(&mut self, text: &str) {
        self.chat.messages.push(Message {
            text: text.into(),
            ..Default::default()
        });
    }

    /// The chat the organization's agents answer in, and what they ask.
    pub fn chat_and_asks(&mut self) -> (&mut Vec<Message>, &mut Vec<Ask>) {
        (&mut self.chat.messages, &mut self.asks)
    }

    /// The asks the user answered since the last call: each ask, whether
    /// allowed, and whether always.
    pub fn take_answers(&mut self) -> Vec<(Ask, bool, bool)> {
        std::mem::take(&mut self.answered)
    }

    /// Sends `line` to this chat's own run. False when none runs.
    pub fn send(&mut self, line: &str) -> bool {
        self.run.as_mut().map(|run| run.send(line)).is_some()
    }

    /// How many asks wait on the user.
    pub fn asking(&self) -> usize {
        self.asks.len()
    }

    /// Forgets the asks once no agent runs to answer.
    pub fn drop_asks(&mut self) {
        self.asks.clear();
    }

    /// Adds a line to the agent's latest message.
    fn reply(&mut self, line: &str) {
        if let Some(message) = self.chat.messages.last_mut() {
            if !message.text.is_empty() {
                message.text.push('\n');
            }
            message.text.push_str(line);
        }
    }

    /// Takes what the run wrote. Once it exits, returns whether it changed
    /// the watched file.
    pub fn poll(&mut self) -> Option<bool> {
        for output in self.run.as_ref()?.poll() {
            match output {
                Output::Line(line) => {
                    if let Some(ask) = Ask::read(&line) {
                        self.asks.push(ask);
                    } else if !self.streamed {
                        self.reply(&line);
                    } else {
                        if ended(&line)
                            && let Some(run) = &mut self.run
                        {
                            run.close();
                        }
                        let Some(message) = self.chat.messages.last_mut() else {
                            continue;
                        };
                        let (text, thoughts) = (&mut message.text, &mut message.thoughts);
                        organization::take(organization::streamed(&line), text, thoughts);
                    }
                }
                Output::Exit(code) => {
                    if code != 0 {
                        self.reply(&format!("Exited with code {code}."));
                    } else if self.chat.messages.last().is_some_and(|m| m.text.is_empty()) {
                        // Shows it finished, and keeps an empty turn out of
                        // the chat the next prompt resends.
                        self.reply("Done.");
                    }
                    let said = self.chat.messages.last().map(|m| m.text.as_str());
                    if let Some(mut task) = self.offered.take()
                        && code == 0
                        && let Some((organization, brief)) = said.and_then(organization::handoff)
                    {
                        if !brief.is_empty() {
                            task.push_str(&format!(
                                "\n\nThe agent the user asked handed this task to your team, adding:\n{brief}"
                            ));
                            // The child session below names the organization.
                            if let Some(message) = self.chat.messages.last_mut() {
                                message.text.clone_from(&brief);
                            }
                        }
                        self.handoff = Some((organization, task));
                    }
                    self.run = None;
                    let (file, before) = self.watched.take()?;
                    return Some(modified(&file) != before);
                }
            }
        }
        None
    }

    /// Puts the current chat away, if it has anything, and shows an empty one.
    fn new_chat(&mut self) {
        self.show_chat(Chat::default());
    }

    /// Shows `chat` in place of the current one, which goes to the history.
    /// What was attached or typed for the current one stays behind with it.
    /// An older copy of `chat`, as a paused run's chat has among the saved
    /// ones, gives way to it.
    fn show_chat(&mut self, chat: Chat) {
        self.attached.clear();
        self.prompt.clear();
        self.picking = None;
        // The heights are by position, so they belong to the chat shown.
        self.heights.clear();
        let id = chat.id;
        if id != 0 {
            self.history.retain(|other| other.id != id);
        }
        let current = std::mem::replace(&mut self.chat, chat);
        if !current.messages.is_empty() && (id == 0 || current.id != id) {
            self.history.push(current);
        }
    }

    /// Shows the window's contents, where the provider and model, or `org`,
    /// are picked too. Returns the prompt to send and every file attached
    /// in this chat, unless `blocked` says why it cannot go.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        settings: &mut AgentSettings,
        agents: &Agents,
        blocked: Option<&str>,
        org: &mut OrgPanel,
    ) -> Option<(String, Vec<PathBuf>)> {
        let busy = self.running() || org.running();
        if !self.sidebar_closed {
            egui::Panel::left("agent_history")
                .default_size(190.0)
                .show(ui, |ui| self.sidebar(ui, busy));
        }
        let send = egui::Panel::bottom("agent_prompt")
            .show(ui, |ui| {
                self.prompt_card(ui, settings, agents, blocked, org)
            })
            .inner;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                let label = if self.sidebar_closed {
                    t("Show history")
                } else {
                    t("Hide history")
                };
                if ui.small_button(label).clicked() {
                    self.sidebar_closed = !self.sidebar_closed;
                }
                // The sidebar's "+ New Agent" is gone while it is hidden.
                let new = egui::Button::new(t("+ New")).small();
                if ui.add_enabled(!busy, new).clicked() {
                    self.new_chat();
                }
                ui.label(RichText::new(self.chat.title()).strong());
            });
            ui.separator();
            let paused = org
                .paused()
                .is_some_and(|paused| self.chat.id != 0 && paused.chat().0 == self.chat.id);
            // A run keeps the chat it answers in shown until it is done.
            let live = if org.running() {
                Some(Live::Working)
            } else {
                paused.then_some(Live::Paused)
            };
            self.messages(ui, busy, live);
        });
        send
    }

    fn sidebar(&mut self, ui: &mut Ui, busy: bool) {
        ui.add_space(4.0);
        let new_agent =
            egui::Button::new(t("+ New Agent")).min_size(egui::vec2(ui.available_width(), 28.0));
        if ui.add_enabled(!busy, new_agent).clicked() {
            self.new_chat();
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new(t("History"))
                .small()
                .strong()
                .color(panels::MUTED),
        );
        let mut open = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.history.is_empty() {
                ui.colored_label(panels::MUTED, t("No earlier chats."));
            }
            for (index, chat) in self.history.iter().enumerate().rev() {
                let row = egui::Button::new(chat.title())
                    .frame(false)
                    .truncate()
                    .min_size(egui::vec2(ui.available_width(), 0.0));
                if ui.add_enabled(!busy, row).clicked() {
                    open = Some(index);
                }
            }
        });
        if let Some(index) = open {
            let chat = self.history.remove(index);
            self.show_chat(chat);
        }
    }

    /// The chat's messages, where `live` says how the organization run
    /// answering in it stands, if it is not done.
    fn messages(&mut self, ui: &mut Ui, busy: bool, live: Option<Live>) {
        // Markdown is parsed again on every draw, and typing redraws the
        // window, so a unit well out of view is not drawn: it takes the
        // height it was last drawn at. ponytail: the key is the message
        // lengths, the spinner and the live state, not the text itself,
        // a same-length rewrite out of view keeps its old height until seen.
        const VIEW_MARGIN: f32 = 200.0;
        let mut markdown = std::mem::take(&mut self.markdown);
        let mut heights = std::mem::take(&mut self.heights);
        let width = ui.available_width();
        if self.heights_width != width {
            heights.clear();
            self.heights_width = width;
        }
        heights.resize(self.chat.messages.len(), None);
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .stick_to_bottom(true)
            .show_viewport(ui, |ui, view| {
                // The viewport counts from the top of the content, but the
                // cursor's position moves with the scroll: count up here.
                let mut y = 4.0_f32;
                ui.add_space(4.0);
                let messages = &self.chat.messages;
                if messages.is_empty() {
                    ui.colored_label(panels::MUTED, t("Ask the agent to change the project."));
                }
                let mut index = 0;
                while index < messages.len() {
                    let start = ui.cursor().top();
                    let organization = messages[index].child.as_ref();
                    // A child session's agents, and the rounds it announces,
                    // answer under it until someone else speaks.
                    let end = match organization {
                        Some(_) => {
                            index
                                + 1
                                + messages[index + 1..]
                                    .iter()
                                    .take_while(|message| message.speaker.is_some())
                                    .count()
                        }
                        None => index + 1,
                    };
                    let live = live.filter(|_| end == messages.len());
                    let working =
                        busy && end == messages.len() && messages[end - 1].text.is_empty();
                    let state = match live {
                        None => 0,
                        Some(Live::Working) => 1,
                        Some(Live::Paused) => 2,
                    };
                    let key = (
                        messages[index..end].iter().map(|m| m.text.len()).sum(),
                        messages[index..end].iter().map(|m| m.thoughts.len()).sum(),
                        working,
                        state,
                    );
                    let known = heights[index]
                        .filter(|(seen, _)| *seen == key)
                        .map(|(_, px)| px);
                    if let Some(px) = known.filter(|px| {
                        y + px < view.min.y - VIEW_MARGIN || y > view.max.y + VIEW_MARGIN
                    }) {
                        ui.add_space(px);
                        y += px;
                        index = end;
                        continue;
                    }
                    if let Some(organization) = organization {
                        egui::Frame::new()
                            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                            .corner_radius(8.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let id = egui::Id::new(("child", self.chat.id, index));
                                CollapsingState::load_with_default_open(ui.ctx(), id, true)
                                    .show_header(ui, |ui| {
                                        let title =
                                            format!("{} · {organization}", t("Child session"));
                                        ui.label(RichText::new(title).strong());
                                        match live {
                                            Some(Live::Working) => {
                                                ui.spinner();
                                                ui.colored_label(panels::MUTED, t("working"));
                                            }
                                            Some(Live::Paused) => {
                                                ui.colored_label(panels::WARNING, t("paused"));
                                            }
                                            None => {}
                                        }
                                    })
                                    .body(|ui| {
                                        for at in index + 1..end {
                                            self.message(ui, &mut markdown, at, busy);
                                            ui.add_space(8.0);
                                        }
                                    });
                            });
                        ui.add_space(8.0);
                    } else {
                        self.message(ui, &mut markdown, index, busy);
                        ui.add_space(8.0);
                    }
                    let px = ui.cursor().top() - start;
                    heights[index] = Some((key, px));
                    y += px;
                    index = end;
                }
                if let Some(answer) = asks(ui, &self.asks) {
                    let (index, allow, always) = answer;
                    let ask = self.asks.remove(index);
                    self.answered.push((ask, allow, always));
                }
            });
        self.heights = heights;
        self.markdown = markdown;
    }

    /// The chat's message at `index`: the user's, an organization agent's
    /// under its name, or the chat agent's. The last one is still being
    /// written while `busy`.
    fn message(
        &self,
        ui: &mut Ui,
        markdown: &mut egui_commonmark::CommonMarkCache,
        index: usize,
        busy: bool,
    ) {
        let message = &self.chat.messages[index];
        let working = message.text.is_empty() && index + 1 == self.chat.messages.len() && busy;
        if message.user {
            egui::Frame::new()
                .fill(ui.visuals().extreme_bg_color)
                .corner_radius(8.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(&message.text);
                });
        } else if let Some((name, [r, g, b])) = &message.speaker {
            let color = &Color32::from_rgb(*r, *g, *b);
            // Like a team chat: the agent's initial, its name, then what it
            // said.
            ui.horizontal_top(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 6.0, *color);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    name.chars().next().unwrap_or('?'),
                    egui::FontId::proportional(15.0),
                    Color32::WHITE,
                );
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).strong().color(*color));
                        if working {
                            ui.spinner();
                        }
                    });
                    self.thoughts(ui, index, message);
                    reply(ui, markdown, &message.text);
                });
            });
        } else {
            self.thoughts(ui, index, message);
            if working {
                ui.spinner();
            } else {
                reply(ui, markdown, &message.text);
            }
        }
    }

    /// `message`'s thoughts, folded until clicked.
    fn thoughts(&self, ui: &mut Ui, index: usize, message: &Message) {
        if message.thoughts.is_empty() {
            return;
        }
        let count = message.thoughts.lines().count();
        let heading = format!("{} ({count})", t("Thoughts"));
        egui::CollapsingHeader::new(RichText::new(heading).small().color(panels::MUTED))
            .id_salt(("thoughts", self.history.len(), index))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(&message.thoughts)
                        .small()
                        .color(panels::MUTED),
                );
            });
    }

    /// The card with the attached files, the prompt and the controls under
    /// it. Returns the prompt to send and every file attached in this chat.
    fn prompt_card(
        &mut self,
        ui: &mut Ui,
        settings: &mut AgentSettings,
        agents: &Agents,
        blocked: Option<&str>,
        org: &mut OrgPanel,
    ) -> Option<(String, Vec<PathBuf>)> {
        if let Some(picking) = &self.picking {
            match picking.try_recv() {
                Ok(files) => {
                    self.attached.extend(files.unwrap_or_default());
                    self.picking = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.picking = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let mut send = None;
        ui.add_space(6.0);
        egui::Frame::new()
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .corner_radius(10.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if !self.attached.is_empty() {
                    let mut removed = None;
                    ui.horizontal_wrapped(|ui| {
                        for (index, file) in self.attached.iter().enumerate() {
                            let name = file.file_name().map_or_else(
                                || file.display().to_string(),
                                |name| name.to_string_lossy().into_owned(),
                            );
                            if ui
                                .small_button(format!("{name}  x"))
                                .on_hover_text(file.display().to_string())
                                .clicked()
                            {
                                removed = Some(index);
                            }
                        }
                    });
                    if let Some(index) = removed {
                        self.attached.remove(index);
                    }
                }
                let field = ui.add(
                    egui::TextEdit::multiline(&mut self.prompt)
                        .frame(egui::Frame::NONE)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text(t("Ask the agent to change the project. Ctrl+Enter sends.")),
                );
                let shortcut = field.has_focus()
                    && ui.input(|input| input.modifiers.command && input.key_pressed(Key::Enter));
                ui.horizontal(|ui| {
                    let attach = egui::Button::new(t("+ Attach"));
                    if ui.add_enabled(self.picking.is_none(), attach).clicked() {
                        // The dialog waits for the user, so it runs on its
                        // own thread to keep the editor drawing. ponytail:
                        // no parent window, so it may open behind this one.
                        let (sender, receiver) = mpsc::channel();
                        let ctx = ui.ctx().clone();
                        thread::spawn(move || {
                            let _ = sender.send(rfd::FileDialog::new().pick_files());
                            ctx.request_repaint();
                        });
                        self.picking = Some(receiver);
                    }
                    model_picker(ui, settings, agents, org.names(), &mut self.organization);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(run) = &self.run {
                            if ui.button(t("Stop")).clicked() {
                                run.stop();
                            }
                            return;
                        }
                        if org.running() {
                            if ui.button(t("Stop")).clicked() {
                                org.stop();
                            }
                            return;
                        }
                        let can_send = blocked.is_none() && !self.prompt.trim().is_empty();
                        let button = ui.add_enabled(can_send, egui::Button::new(t("Send")));
                        let paused = org
                            .paused()
                            .filter(|paused| self.chat.id != 0 && paused.chat().0 == self.chat.id);
                        if let Some(paused) = paused {
                            let resume = ui
                                .add_enabled(blocked.is_none(), egui::Button::new(t("Continue")))
                                .on_hover_text(
                                    t("Runs the agents of {} that are not done yet, handing on what the others answered.")
                                        .replace("{}", paused.organization()),
                                );
                            if resume.clicked() {
                                self.continuing = true;
                            }
                        }
                        if can_send && (button.clicked() || shortcut) {
                            let mut prompt = std::mem::take(&mut self.prompt).trim().to_string();
                            let files = std::mem::take(&mut self.attached);
                            // Plain paths, since `@path` stops at a space and
                            // codex does not read it.
                            if !files.is_empty() {
                                prompt.push_str("\n\nAttached files:");
                                for file in &files {
                                    prompt.push_str(&format!("\n{}", file.display()));
                                }
                            }
                            for file in files {
                                if !self.chat.files.contains(&file) {
                                    self.chat.files.push(file);
                                }
                            }
                            send = Some((prompt, self.chat.files.clone()));
                        }
                        if let Some(reason) = blocked {
                            ui.colored_label(panels::WARNING, reason);
                        }
                    });
                });
            });
        send
    }
}

/// An agent's reply, rendered from its Markdown. A clicked link opens in the
/// browser, and only a web link: the reply is the agent's, and `open_url`
/// hands anything else, like a `file:` link to a program, to `explorer` as is.
pub fn reply(ui: &mut Ui, cache: &mut egui_commonmark::CommonMarkCache, text: &str) {
    let start = ui.ctx().output(|output| output.commands.len());
    ui.scope(|ui| {
        // Body text only: links keep the hyperlink color.
        ui.visuals_mut().widgets.noninteractive.fg_stroke.color = panels::TEXT;
        egui_commonmark::CommonMarkViewer::new().show(ui, cache, text);
    });
    // eframe is built without its own link opening, so the clicks are ours.
    let mut links = Vec::new();
    ui.ctx().output_mut(|output| {
        let mut added = output.commands.split_off(start);
        added.retain(|command| match command {
            egui::OutputCommand::OpenUrl(open) => {
                links.push(open.url.clone());
                false
            }
            _ => true,
        });
        output.commands.extend(added);
    });
    for link in links.iter().filter_map(|link| web_link(link)) {
        crate::project::open_url(&link);
    }
}

/// `link` as it is safe to hand to `explorer`, if it is a web link. Explorer
/// splits its command line at commas, so a comma is encoded; a quote,
/// whitespace or a control character could end the argument, so such a link
/// is not opened.
fn web_link(link: &str) -> Option<String> {
    let web = link.starts_with("https://") || link.starts_with("http://");
    let plain = !link
        .chars()
        .any(|c| c == '"' || c.is_whitespace() || c.is_control());
    (web && plain).then(|| link.replace(',', "%2C"))
}

/// A card for each ask, with what it would do. Returns the one answered:
/// its index, whether allowed, and whether always.
fn asks(ui: &mut Ui, asks: &[Ask]) -> Option<(usize, bool, bool)> {
    let mut answer = None;
    for (index, ask) in asks.iter().enumerate() {
        egui::Frame::new()
            .stroke((1.0, panels::WARNING))
            .corner_radius(8.0)
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let heading = t("The agent asks to use {}:").replace("{}", &ask.tool);
                ui.label(RichText::new(heading).strong().color(panels::WARNING));
                let what: String = ask.what().chars().take(2000).collect();
                ui.add(egui::Label::new(RichText::new(what).monospace()).wrap());
                ui.horizontal_wrapped(|ui| {
                    if ui.button(t("Allow")).clicked() {
                        answer = Some((index, true, false));
                    }
                    if let Some(saves) = ask.saves() {
                        let always = format!("{}: {saves}", t("Always allow"));
                        if ui
                            .button(always)
                            .on_hover_text(t(
                                "Saved in Settings > Agent Settings for this project.",
                            ))
                            .clicked()
                        {
                            answer = Some((index, true, true));
                        }
                    }
                    if ui.button(t("Deny")).clicked() {
                        answer = Some((index, false, false));
                    }
                });
            });
        ui.add_space(8.0);
    }
    answer
}

pub fn model_label(model: &str) -> &str {
    if model.is_empty() {
        t("CLI default")
    } else {
        model
    }
}

/// One list of every provider's models: each provider's name and how it
/// would be reached, then its models by name alone. Below them, the
/// project's organizations by name.
fn model_picker(
    ui: &mut Ui,
    settings: &mut AgentSettings,
    agents: &Agents,
    organizations: &[String],
    organization: &mut Option<String>,
) {
    let provider = settings.provider;
    let selected = match organization {
        Some(name) => format!("{} · {name}", t("Organization")),
        None => format!(
            "{} · {}",
            provider.label(),
            model_label(settings.model(provider))
        ),
    };
    egui::ComboBox::from_id_salt("agent_model")
        .selected_text(selected)
        // Every provider's models at once, without scrolling.
        .height(f32::INFINITY)
        .show_ui(ui, |ui| {
            for provider in Provider::ALL {
                let access = settings.access(provider);
                let (how, color) = match (agents.ready(provider, access), access) {
                    (None, _) => (t("checking"), panels::MUTED),
                    (Some(false), _) => (t("can't send yet"), panels::WARNING),
                    (Some(true), Access::Cli) => ("CLI", panels::MUTED),
                    (Some(true), Access::Key) => (t("API key"), panels::MUTED),
                };
                let mut heading = egui::text::LayoutJob::default();
                for text in [
                    RichText::new(provider.label()).strong(),
                    RichText::new(format!("  {how}")).color(color).small(),
                ] {
                    text.append_to(
                        &mut heading,
                        ui.style(),
                        egui::FontSelection::Default,
                        egui::Align::Center,
                    );
                }
                ui.label(heading);
                ui.indent(provider.label(), |ui| {
                    for model in std::iter::once("").chain(provider.models().iter().copied()) {
                        let current = organization.is_none()
                            && settings.provider == provider
                            && settings.model(provider) == model;
                        if ui.selectable_label(current, model_label(model)).clicked() {
                            settings.provider = provider;
                            settings.models.insert(provider, model.into());
                            *organization = None;
                        }
                    }
                });
            }
            ui.label(RichText::new(t("Organization")).strong());
            ui.indent("organization", |ui| {
                for name in organizations {
                    let current = organization.as_ref() == Some(name);
                    let team = ui.selectable_label(current, name).on_hover_text(t(
                        "Each agent answers in the chat, in link order. Set them up in the Organization tab.",
                    ));
                    if team.clicked() {
                        *organization = Some(name.clone());
                    }
                }
            });
        });
}

/// Closing the project stops the agent rather than let it edit on unseen.
impl Drop for AgentPanel {
    fn drop(&mut self) {
        if let Some(run) = &self.run {
            run.stop();
        }
    }
}

fn modified(file: &Path) -> Option<SystemTime> {
    fs::metadata(file)
        .and_then(|metadata| metadata.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn goes_the_chosen_way_once_that_way_has_what_it_needs() {
        assert_eq!(route(Access::Cli, Cli::LoggedIn, "sk"), Ok(Route::Cli));
        assert!(route(Access::Cli, Cli::LoggedOut, "sk").is_err());
        assert_eq!(
            route(Access::Key, Cli::LoggedIn, " sk "),
            Ok(Route::Key("sk".into()))
        );
        assert_eq!(
            route(Access::Key, Cli::LoggedOut, "sk"),
            Ok(Route::Key("sk".into()))
        );
        assert!(route(Access::Key, Cli::LoggedIn, "").is_err());
        assert!(route(Access::Key, Cli::Missing, "sk").is_err());
        assert!(route(Access::Cli, Cli::Checking, "").is_err());
    }

    #[test]
    fn shows_the_key_box_only_for_a_provider_switched_to_its_key() {
        let mut harness = Harness::builder().build_ui_state(
            |ui, (settings, agents): &mut (AgentSettings, Agents)| {
                show(ui, settings, agents);
            },
            (AgentSettings::default(), agents(Cli::LoggedOut)),
        );
        harness.run();
        assert_eq!(
            harness
                .query_all_by_role(egui::accesskit::Role::PasswordInput)
                .count(),
            0
        );
        assert!(harness.query_by_label_contains("Run `claude`").is_some());

        harness.get_all_by_label("API key").next().unwrap().click();
        harness.run();
        assert_eq!(harness.state().0.access(Provider::Claude), Access::Key);
        assert_eq!(
            harness
                .query_all_by_role(egui::accesskit::Role::PasswordInput)
                .count(),
            1
        );
        assert!(harness.query_by_label_contains("Run `claude`").is_none());
    }

    #[test]
    fn passes_the_key_in_the_variable_each_cli_reads() {
        let besfa = ProjectAccess::default();
        let (program, args, env) = invocation(Provider::Claude, &Route::Cli, "opus", &[], &besfa);
        assert_eq!(program, "claude");
        // Auto is the default permission mode.
        let mut expected = vec![
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--permission-prompt-tool",
            "stdio",
            "--append-system-prompt",
            AGENT_WINDOW,
            "--permission-mode",
            "auto",
        ];
        if cfg!(windows) {
            expected.extend(["--disallowedTools", "Bash"]);
        }
        expected.extend(["--allowedTools", "mcp__besfa", "--model", "opus"]);
        assert_eq!(args, expected);
        if cfg!(windows) {
            assert_eq!(
                env,
                [("CLAUDE_CODE_USE_POWERSHELL_TOOL".to_string(), "1".to_string())]
            );
        } else {
            assert!(env.is_empty());
        }

        let (program, args, env) =
            invocation(Provider::OpenAi, &Route::Key("sk".into()), "", &[], &besfa);
        assert_eq!(program, "codex");
        assert_eq!(args[0], "exec");
        assert_eq!(env, [("CODEX_API_KEY".to_string(), "sk".to_string())]);

        let (_, args, _) = invocation(Provider::Gemini, &Route::Cli, "", &[], &besfa);
        assert_eq!(args, ["--prompt", "", "--approval-mode", "auto_edit"]);
    }

    #[test]
    fn passes_the_projects_permission_mode_to_claude_only() {
        let mode = |permission: Permission, provider: Provider| {
            let access = ProjectAccess {
                permission,
                ..Default::default()
            };
            let (_, args, _) = invocation(provider, &Route::Cli, "", &[], &access);
            args.iter()
                .position(|arg| arg == "--permission-mode")
                .map(|at| args[at + 1].clone())
        };
        assert_eq!(mode(Permission::Auto, Provider::Claude).as_deref(), Some("auto"));
        assert_eq!(
            mode(Permission::AcceptEdits, Provider::Claude).as_deref(),
            Some("acceptEdits")
        );
        assert_eq!(mode(Permission::Manual, Provider::Claude).as_deref(), Some("manual"));
        assert_eq!(mode(Permission::Auto, Provider::OpenAi), None);
        assert_eq!(mode(Permission::Auto, Provider::Gemini), None);

        // A project saved before the choice existed gets the default.
        let old: ProjectAccess = ron::from_str("(github:true)").unwrap();
        assert_eq!(old.permission, Permission::Auto);
        assert!(old.github);
    }

    #[test]
    fn bash_shell_leaves_bash_on_and_uses_bash_rules() {
        let access = ProjectAccess {
            commands: "cargo *\n".into(),
            shell: Shell::Bash,
            ..Default::default()
        };
        let (_, args, env) = invocation(Provider::Claude, &Route::Cli, "", &[], &access);
        assert!(!args.iter().any(|arg| arg == "--disallowedTools"), "{args:?}");
        assert!(env.is_empty(), "{env:?}");
        assert!(args.contains(&"Bash(cargo *)".to_string()), "{args:?}");

        let powershell = ProjectAccess {
            shell: Shell::PowerShell,
            ..access
        };
        let (_, args, env) = invocation(Provider::Claude, &Route::Cli, "", &[], &powershell);
        let disallow = args.iter().position(|arg| arg == "--disallowedTools");
        assert_eq!(disallow.map(|at| args[at + 1].as_str()), Some("Bash"));
        assert_eq!(
            env,
            [("CLAUDE_CODE_USE_POWERSHELL_TOOL".to_string(), "1".to_string())]
        );
        assert!(args.contains(&"PowerShell(cargo *)".to_string()), "{args:?}");
    }

    #[test]
    fn allows_claude_what_the_project_allows() {
        let allowed = |access: &ProjectAccess| {
            let (_, args, _) = invocation(Provider::Claude, &Route::Cli, "opus", &[], access);
            let Some(start) = args.iter().position(|arg| arg == "--allowedTools") else {
                return Vec::new();
            };
            // The list must end at a flag, or it would take the model too.
            assert_eq!(args[args.len() - 2..], ["--model", "opus"]);
            args[start + 1..args.len() - 2].to_vec()
        };
        assert_eq!(allowed(&ProjectAccess::default()), ["mcp__besfa"]);

        let mut access = ProjectAccess {
            servers: ["besfa", "my.server"].map(String::from).into(),
            github: true,
            merge: false,
            commands:
                " cargo *\r\n\npwsh -File tools/drive.ps1 *\nBash(just *)\nPowerShell(cargo *)\n"
                    .into(),
            shell: Shell::default(),
            permission: Permission::default(),
        };
        // Plain lines are for the chosen shell tool; a written rule stays as
        // it is.
        let shell = access.shell.tool();
        assert_eq!(
            allowed(&access),
            [
                "mcp__besfa".to_string(),
                "mcp__my_server".into(),
                format!("{shell}(git *)"),
                format!("{shell}(gh *)"),
                format!("{shell}(cargo *)"),
                format!("{shell}(pwsh -File tools/drive.ps1 *)"),
                "Bash(just *)".into(),
                "PowerShell(cargo *)".into(),
            ]
        );

        access.servers.clear();
        access.github = false;
        access.commands.clear();
        assert!(allowed(&access).is_empty(), "no flag at all");

        // Codex and Gemini get none of it.
        access.github = true;
        for provider in [Provider::OpenAi, Provider::Gemini] {
            let (_, args, _) = invocation(provider, &Route::Cli, "", &[], &access);
            assert!(!args.iter().any(|arg| arg.contains("Bash(")), "{args:?}");
        }
    }

    /// A `can_use_tool` request as Claude writes it, shortened.
    fn ask_line(tool: &str, input: &str) -> String {
        format!(
            r#"{{"type":"control_request","request_id":"r1","request":{{"subtype":"can_use_tool","tool_name":"{tool}","display_name":"{tool}","input":{input},"permission_suggestions":[]}}}}"#
        )
    }

    #[test]
    fn reads_what_claude_asks_and_answers_it() {
        assert_eq!(Ask::read(r#"{"type":"assistant","message":{}}"#), None);
        assert_eq!(Ask::read("cargo build"), None);
        let ask = Ask::read(&ask_line(
            "Bash",
            r#"{"command":"cargo build --release","description":"Build"}"#,
        ))
        .unwrap();
        assert_eq!(ask.tool, "Bash");
        assert_eq!(ask.what(), "cargo build --release");
        assert_eq!(ask.saves().as_deref(), Some("cargo *"));

        let answer =
            |allow, always| -> Value { serde_json::from_str(&ask.answer(allow, always)).unwrap() };
        let once = answer(true, false);
        assert_eq!(once["type"], "control_response");
        assert_eq!(once["response"]["request_id"], "r1");
        assert_eq!(once["response"]["response"]["behavior"], "allow");
        assert_eq!(once["response"]["response"]["updatedInput"], ask.input);
        assert!(once["response"]["response"]["updatedPermissions"].is_null());
        // Always also stops this run from asking again.
        let always = answer(true, true);
        let rule = &always["response"]["response"]["updatedPermissions"][0];
        assert_eq!(rule["destination"], "session");
        assert_eq!(
            rule["rules"][0],
            serde_json::json!({"toolName": "Bash", "ruleContent": "cargo *"})
        );
        let no = answer(false, false);
        assert_eq!(no["response"]["response"]["behavior"], "deny");

        let shell = Ask::read(&ask_line("PowerShell", r#"{"command":"cargo test"}"#)).unwrap();
        assert_eq!(shell.saves().as_deref(), Some("PowerShell(cargo *)"));
        assert_eq!(ask.allowed().as_deref(), Some("Bash(cargo *)"));
        assert_eq!(shell.allowed().as_deref(), Some("PowerShell(cargo *)"));
        let tool = Ask::read(&ask_line("mcp__my_server__run", r#"{"x":1}"#)).unwrap();
        assert_eq!(tool.saves().as_deref(), Some("my_server MCP"));
        assert_eq!(tool.allowed().as_deref(), Some("mcp__my_server"));
        assert_eq!(tool.what(), r#"{"x":1}"#);
        let fetch = Ask::read(&ask_line("WebFetch", r#"{"url":"https://a.b"}"#)).unwrap();
        assert_eq!(fetch.saves(), None, "only once");
        assert_eq!(fetch.what(), "https://a.b");
    }

    #[test]
    fn always_allow_saves_the_rule_for_the_project_once() {
        let project = Path::new(r"C:\Games\Dusk");
        let mut settings = AgentSettings::default();
        settings.projects.insert(
            project_key(project),
            ProjectAccess {
                commands: "pwsh *".into(),
                ..Default::default()
            },
        );
        let cargo = Ask::read(&ask_line("Bash", r#"{"command":"cargo build"}"#)).unwrap();
        settings.allow(project, &cargo);
        settings.allow(project, &cargo);
        let shell = Ask::read(&ask_line("PowerShell", r#"{"command":"git status"}"#)).unwrap();
        settings.allow(project, &shell);
        let tool = Ask::read(&ask_line("mcp__github__create_pr", "{}")).unwrap();
        settings.allow(project, &tool);
        let access = settings.project(project);
        assert_eq!(access.commands, "pwsh *\ncargo *\nPowerShell(git *)\n");
        assert!(access.servers.contains("github") && access.servers.contains("besfa"));
        assert!(access.rules().contains(&"PowerShell(git *)".to_string()));
        assert!(settings.project(Path::new(r"C:\Other")).commands.is_empty());
    }

    /// Needs a logged-in `claude` and spends a little of its usage:
    /// `cargo test -p besfa_editor real_claude -- --ignored`.
    #[test]
    #[ignore]
    fn real_claude_asks_once_and_takes_always() {
        let dir = std::env::temp_dir().join(format!("besfa-real-ask-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // Auto may run the command without asking; this tests the asking.
        let none = ProjectAccess {
            servers: Default::default(),
            permission: Permission::AcceptEdits,
            ..Default::default()
        };
        let invocation = invocation(Provider::Claude, &Route::Cli, "haiku", &[], &none);
        let ctx = egui::Context::default();
        let mut panel = AgentPanel::default();
        let prompt = "Use the Bash tool to run exactly: python -c \"open('a.txt','w').write('x')\" and then, as a separate Bash call, exactly: python -c \"open('b.txt','w').write('x')\" Then say ok.";
        panel.start(invocation, prompt, &dir, dir.join("none"), "", &ctx);
        let mut asked = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
        while panel.poll().is_none() && std::time::Instant::now() < deadline {
            for ask in std::mem::take(&mut panel.asks) {
                assert!(panel.send(&ask.answer(true, true)));
                asked.push(ask.what());
            }
            thread::sleep(std::time::Duration::from_millis(20));
        }
        let last = panel.chat.messages.last().unwrap();
        eprintln!(
            "asked {asked:?}
text {:?}
thoughts {:?}",
            last.text, last.thoughts
        );
        assert!(dir.join("a.txt").is_file() && dir.join("b.txt").is_file());
        assert_eq!(asked.len(), 1, "always covers the second");
        assert!(!panel.running());
    }

    #[test]
    fn asks_in_the_chat_and_hands_the_answer_over() {
        let mut harness = tab(None);
        let cargo = Ask::read(&ask_line("Bash", r#"{"command":"cargo build"}"#)).unwrap();
        let fetch = Ask::read(&ask_line("WebFetch", r#"{"url":"https://a.b"}"#)).unwrap();
        harness.state_mut().panel.asks = vec![cargo.clone(), fetch.clone()];
        harness.run();
        assert!(harness.query_by_label("cargo build").is_some());
        assert!(harness.query_by_label("https://a.b").is_some());
        assert_eq!(
            harness.query_all_by_label_contains("Always allow").count(),
            1,
            "WebFetch can only be allowed once"
        );
        harness.get_by_label("Always allow: cargo *").click();
        harness.run();
        let panel = &mut harness.state_mut().panel;
        assert_eq!(panel.take_answers(), [(cargo, true, true)]);
        assert_eq!(panel.asks, std::slice::from_ref(&fetch));
        harness.get_by_label("Deny").click();
        harness.run();
        let panel = &mut harness.state_mut().panel;
        assert_eq!(panel.take_answers(), [(fetch, false, false)]);
        assert!(panel.asks.is_empty());
    }

    #[test]
    fn keeps_the_chats_for_the_next_time_the_project_opens() {
        let file = std::env::temp_dir().join(format!("besfa-chats-{}/c.ron", std::process::id()));
        let _ = fs::remove_dir_all(file.parent().unwrap());
        let mut panel = AgentPanel::read(&file);
        assert!(panel.keep(&file, false).is_ok());
        assert!(!file.exists(), "nothing to keep yet");

        panel.ask("Add a jump.");
        panel.chat.messages.push(Message::default());
        panel.keep(&file, true).unwrap();
        assert!(!file.exists(), "not while the agent answers");
        panel.reply("Jump added.");
        panel.keep(&file, false).unwrap();
        let id = panel.chat_id();
        panel.new_chat();
        panel.ask("Make it higher.");
        panel.keep(&file, false).unwrap();

        let mut back = AgentPanel::read(&file);
        assert_eq!(back.history.len(), 1);
        assert_eq!(back.history[0].messages[1].text, "Jump added.");
        assert_eq!(back.chat.messages[0].text, "Make it higher.");

        // A paused run's chat shows once, the longer copy of it: the saved
        // one when the user went on after the pause...
        let user = |text: &str| Message {
            user: true,
            text: text.into(),
            ..Default::default()
        };
        let mut paused = vec![user("Add a jump.")];
        let mut again = AgentPanel::read(&file);
        again.restore((id, &paused), &[], "Studio");
        assert_eq!(again.chat.id, id);
        assert_eq!(again.chat.messages[1].text, "Jump added.");
        assert_eq!(again.history.len(), 1, "{:?}", again.history);
        assert_eq!(again.history[0].messages[0].text, "Make it higher.");
        assert_eq!(again.organization(), Some("Studio"));
        // ...the paused one when the editor closed on the run.
        let agent = Message {
            speaker: Some(("Game Designer".into(), [0; 3])),
            ..Default::default()
        };
        paused.extend([agent.clone(), agent]);
        back.restore((id, &paused), &[], "Studio");
        assert_eq!(back.chat.messages.len(), 3);
        assert_eq!(back.chat.messages[2].text, "Stopped.");
        assert_eq!(back.history.len(), 1, "{:?}", back.history);
        fs::remove_dir_all(file.parent().unwrap()).unwrap();

        let name = |path: &str| AgentPanel::chats_file(Path::new(path));
        assert_eq!(name(r"C:\Games\Dusk\"), name("c:/games/dusk"));
        assert_ne!(name(r"C:\Games\a_b"), name(r"C:\Games\a\b"));
        assert_ne!(name(r"C:\Games\a_b"), name(r"C:\Games\a-b"));
        let dusk = name(r"C:\Games\Dusk").display().to_string();
        assert!(
            dusk.contains(r"Besfa\chats\dusk-") && dusk.ends_with(".ron"),
            "{dusk}"
        );
    }

    #[test]
    fn keeps_each_projects_access_under_the_user_not_the_project() {
        let mut settings = AgentSettings::default();
        assert_eq!(
            settings.project(Path::new(r"C:\Games\Dusk")),
            ProjectAccess::default()
        );
        let access = ProjectAccess {
            github: true,
            ..Default::default()
        };
        settings
            .projects
            .insert(project_key(Path::new(r"C:\Games\Dusk\")), access.clone());
        // The tabs pass the path with a trailing slash; the hub's may differ
        // in case.
        assert_eq!(settings.project(Path::new("c:/games/dusk")), access);
        assert_eq!(
            settings.project(Path::new(r"C:\Games\Other")),
            ProjectAccess::default()
        );

        let text = ron::to_string(&settings).unwrap();
        let back: AgentSettings = ron::from_str(&text).unwrap();
        assert_eq!(back, settings);
        // A settings file from before projects had access loads unchanged.
        let old: AgentSettings = ron::from_str("(provider: OpenAi)").unwrap();
        assert_eq!(old.provider, Provider::OpenAi);
        assert!(old.projects.is_empty());
    }

    #[test]
    fn lists_the_mcp_servers_and_what_starts_them() {
        let dir = std::env::temp_dir().join(format!("besfa_mcp_json_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join(".mcp.json");
        let _ = fs::remove_file(&file);
        assert_eq!(mcp_servers(&file), Ok(Vec::new()), "no file lists none");

        fs::write(
            &file,
            r#"{"mcpServers": {
                "besfa": {"command": "besfa", "args": ["mcp"]},
                "github": {"type": "http", "url": "https://api.githubcopilot.com/mcp/"}
            }}"#,
        )
        .unwrap();
        assert_eq!(
            mcp_servers(&file).unwrap(),
            [
                ("besfa".to_string(), "besfa mcp".to_string()),
                (
                    "github".to_string(),
                    "https://api.githubcopilot.com/mcp/".to_string()
                ),
            ]
        );

        fs::write(&file, "{not json").unwrap();
        assert!(mcp_servers(&file).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_settings_tab_lets_claude_use_github_and_each_server() {
        let dir = std::env::temp_dir().join(format!("besfa_settings_tab_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(".mcp.json"),
            r#"{"mcpServers": {"besfa": {"command": "besfa", "args": ["mcp"]}, "github": {"url": "https://example.com/mcp"}}}"#,
        )
        .unwrap();
        let directory = dir.clone();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(600.0, 900.0))
            .build_ui_state(
                move |ui, (settings, agents): &mut (AgentSettings, Agents)| {
                    settings_tab(ui, settings, agents, &directory);
                },
                (AgentSettings::default(), agents(Cli::LoggedIn)),
            );
        harness.run();
        assert!(
            harness
                .query_by_label_contains("`gh` is logged in.")
                .is_some()
        );
        assert!(harness.query_by_label("https://example.com/mcp").is_some());

        harness
            .get_by_label("Let Claude commit, push and open pull requests")
            .click();
        harness.get_by_label("github").click();
        harness.get_by_label("besfa").click();
        harness.run();
        harness
            .get_by_label("Organization runs review and merge their pull requests")
            .click();
        harness.run();
        let access = harness.state().0.project(&dir);
        assert!(access.github);
        assert!(access.merge);
        assert_eq!(access.servers, ["github".to_string()].into());

        // One allowed in the chat that .mcp.json lacks can be taken back.
        let other = Ask::read(&ask_line("mcp__plugin_x__send", "{}")).unwrap();
        harness.state_mut().0.allow(&dir, &other);
        harness.run();
        assert!(harness.query_by_label("Not in .mcp.json").is_some());
        harness.get_by_label("plugin_x").click();
        harness.run();
        let access = harness.state().0.project(&dir);
        assert_eq!(access.servers, ["github".to_string()].into());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lets_each_cli_reach_the_attached_files() {
        let files = [
            PathBuf::from(r"C:\My Art\ref.PNG"),
            PathBuf::from(r"C:\My Art\notes.txt"),
            PathBuf::from(r"C:\My Art\a,b.png"),
        ];
        let none = ProjectAccess {
            servers: Default::default(),
            ..Default::default()
        };
        let (_, args, _) = invocation(Provider::Claude, &Route::Cli, "", &files, &none);
        assert_eq!(args[args.len() - 2..], ["--add-dir", r"C:\My Art"]);
        let (_, args, _) = invocation(Provider::Gemini, &Route::Cli, "", &files, &none);
        assert_eq!(args[4..], ["--include-directories", r"C:\My Art"]);
        let (_, args, _) = invocation(Provider::OpenAi, &Route::Cli, "", &files, &none);
        assert_eq!(args[4..], [r"--image=C:\My Art\ref.PNG"]);
    }

    #[test]
    fn sends_the_chat_so_far_with_the_next_prompt() {
        let chat = Chat {
            messages: vec![
                Message {
                    user: true,
                    text: "Make the tower taller.".into(),
                    ..Default::default()
                },
                Message {
                    user: false,
                    text: "Done.".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(Chat::default().input("Hi"), "Hi");
        let input = chat.input("Now make it red.");
        assert!(input.contains("User: Make the tower taller."));
        assert!(input.contains("You: Done."));
        assert!(input.ends_with("Now make it red."));
    }

    #[test]
    fn keeps_the_provider_and_models_across_a_restart() {
        let mut settings = AgentSettings {
            provider: Provider::OpenAi,
            ..Default::default()
        };
        settings.models.insert(Provider::Claude, "opus".into());
        let text = ron::to_string(&settings).unwrap();
        let back: AgentSettings = ron::from_str(&text).unwrap();
        assert_eq!(back, settings);
        assert_eq!(back.model(Provider::Gemini), "");
    }

    #[test]
    fn keeps_a_key_in_the_credential_manager_until_it_is_cleared() {
        let target = format!("Besfa/test/{}", std::process::id());
        write_secret(&target, "sk-test").unwrap();
        assert_eq!(read_secret(&target).as_deref(), Some("sk-test"));
        write_secret(&target, "").unwrap();
        assert_eq!(read_secret(&target), None);
        write_secret(&target, "").unwrap();
    }

    fn agents(cli: Cli) -> Agents {
        Agents::fixed([cli; 3], &[])
    }

    struct Tab {
        panel: AgentPanel,
        settings: AgentSettings,
        agents: Agents,
        blocked: Option<&'static str>,
        sent: Option<String>,
        files: Vec<PathBuf>,
        org: OrgPanel,
    }

    fn tab(blocked: Option<&'static str>) -> Harness<'static, Tab> {
        Harness::builder().build_ui_state(
            |ui, tab: &mut Tab| {
                let blocked = tab.blocked;
                let (settings, org) = (&mut tab.settings, &mut tab.org);
                if let Some(prompt) = tab.panel.ui(ui, settings, &tab.agents, blocked, org) {
                    tab.sent = Some(prompt.0);
                    tab.files = prompt.1;
                }
            },
            Tab {
                panel: AgentPanel::default(),
                settings: AgentSettings::default(),
                agents: agents(Cli::LoggedIn),
                blocked,
                sent: None,
                files: Vec::new(),
                org: OrgPanel::new(
                    std::env::temp_dir().join("besfa-no-organization/organizations"),
                ),
            },
        )
    }

    #[test]
    fn keeps_earlier_chats_and_brings_one_back() {
        let said = |text: &str| Chat {
            messages: vec![Message {
                user: true,
                text: text.into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut panel = AgentPanel::default();
        panel.new_chat();
        assert!(panel.history.is_empty());
        panel.chat = said("first");
        panel.new_chat();
        assert_eq!(panel.chat.title(), "New chat");
        let chat = panel.history.remove(0);
        panel.chat = said("second");
        panel.show_chat(chat);
        assert_eq!(panel.chat.title(), "first");
        assert_eq!(panel.history[0].title(), "second");
    }

    #[test]
    fn sends_the_prompt_only_when_nothing_blocks_it() {
        let mut harness = tab(Some("Save the scene first."));
        harness.state_mut().panel.prompt = "Add a red cube.\n".into();
        harness.run();
        harness.get_by_label("Send").click();
        harness.run();
        assert_eq!(harness.state().sent, None);

        harness.state_mut().blocked = None;
        harness.run();
        harness.get_by_label("Send").click();
        harness.run();
        assert_eq!(harness.state().sent.as_deref(), Some("Add a red cube."));
        assert!(harness.state().panel.prompt.is_empty());
    }

    #[test]
    fn hides_the_history_and_still_starts_a_new_chat() {
        let mut harness = tab(None);
        harness.state_mut().panel.chat.messages.push(Message {
            user: true,
            text: "first".into(),
            ..Default::default()
        });
        harness.run();
        harness.get_by_label("Hide history").click();
        harness.run();
        assert!(harness.query_by_label("+ New Agent").is_none());

        harness.get_by_label("+ New").click();
        harness.run();
        let panel = &harness.state().panel;
        assert_eq!(panel.chat.title(), "New chat");
        assert_eq!(panel.history.len(), 1);

        harness.get_by_label("Show history").click();
        harness.run();
        assert!(harness.query_by_label("+ New Agent").is_some());
        assert!(harness.query_by_label("first").is_some());
    }

    #[test]
    fn says_done_when_a_run_prints_nothing() {
        let mut panel = AgentPanel::default();
        let quiet = ("cmd".into(), vec!["/c".into(), "exit 0".into()], vec![]);
        let ctx = egui::Context::default();
        panel.start(quiet, "Hi", Path::new("."), "none".into(), "", &ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while panel.poll().is_none() && std::time::Instant::now() < deadline {
            thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(panel.chat.messages[1].text, "Done.");
    }

    /// Runs `program` with `args` on `prompt` as the chat's agent, told of
    /// `lineup`, until it exits.
    fn answer(panel: &mut AgentPanel, program: &str, args: &str, prompt: &str, lineup: &str) {
        let args = args.split(' ').map(str::to_string).collect();
        let ctx = egui::Context::default();
        panel.start(
            (program.into(), args, vec![]),
            prompt,
            Path::new("."),
            "none".into(),
            lineup,
            &ctx,
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while panel.poll().is_none() {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn hands_a_task_to_the_organization_its_answer_names() {
        let lineup = "- Studio: Game Designer; Qa Tester\n";
        let mut panel = AgentPanel::default();
        // What the agent is sent, echoed back: told of the organizations,
        // which alone hands nothing over.
        answer(&mut panel, "findstr", "/r .", "Add a jump.", lineup);
        let echoed = &panel.chat.messages[1].text;
        assert!(echoed.starts_with("Add a jump."), "{echoed}");
        assert!(echoed.contains("- Studio: Game Designer; Qa Tester"));
        assert!(echoed.contains("`ORGANIZATION: <name>`"));
        assert_eq!(panel.take_handoff(), None);

        let hands = "/c echo Design, code and test it.& echo ORGANIZATION: studio";
        answer(&mut panel, "cmd", hands, "Add a double jump.", lineup);
        let (name, task, files) = panel.take_handoff().unwrap();
        assert_eq!(name, "studio");
        // The chat so far and the user's words, as a task sent to the team
        // straight would be, then the agent's brief; not the note.
        assert!(task.starts_with("Earlier in this conversation:"), "{task}");
        let (_, now) = task.rsplit_once("Now the user says:\n\n").unwrap();
        assert!(now.starts_with("Add a double jump.\n\n"), "{task}");
        // The earlier echo above holds a note; this turn's is left out.
        assert!(!now.contains("From the Besfa editor"), "{task}");
        assert!(
            task.ends_with("adding:\nDesign, code and test it."),
            "{task}"
        );
        assert!(files.is_empty());
        assert_eq!(panel.take_handoff(), None);
        // The chat shows the brief; the child session names the team.
        let shown = &panel.chat.messages.last().unwrap().text;
        assert_eq!(shown.trim(), "Design, code and test it.");

        // Not when it was not told it may, or when it failed.
        answer(&mut panel, "cmd", hands, "Add a jump.", "");
        assert_eq!(panel.take_handoff(), None);
        let fails = "/c echo ORGANIZATION: Studio& exit 1";
        answer(&mut panel, "cmd", fails, "Add a jump.", lineup);
        assert_eq!(panel.take_handoff(), None);
    }

    /// Needs a logged-in `claude` and spends a little of its usage, on the
    /// model in `BESFA_HANDOFF_MODEL` or sonnet:
    /// `cargo test -p besfa_editor real_claude_hands -- --ignored`.
    /// Haiku kept the one-agent tasks too, but did the team's task on this
    /// small game itself.
    #[test]
    #[ignore]
    fn real_claude_hands_off_only_what_needs_a_team() {
        let dir = std::env::temp_dir().join(format!("besfa-real-handoff-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        // A game small enough to read at a glance, so the task decides.
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"game\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("src/main.rs"),
            "mod player;\n\nfn main() {\n    let mut player = player::Player::default();\n    player.jump();\n    println!(\"{}\", player.velocity_y);\n}\n",
        )
        .unwrap();
        fs::write(
            dir.join("src/player.rs"),
            "pub const JUMP_SPEED: f32 = 5.0;\n\n#[derive(Default)]\npub struct Player {\n    pub velocity_y: f32,\n    pub grounded: bool,\n}\n\nimpl Player {\n    pub fn jump(&mut self) {\n        if self.grounded {\n            self.velocity_y = JUMP_SPEED;\n            self.grounded = false;\n        }\n    }\n}\n",
        )
        .unwrap();
        let model = std::env::var("BESFA_HANDOFF_MODEL").unwrap_or("sonnet".into());
        let none = ProjectAccess {
            servers: Default::default(),
            permission: Permission::AcceptEdits,
            ..Default::default()
        };
        let lineup = "- Studio (3 agents): Game Designer (Designs mechanics and how they feel.); Gameplay Programmer (Implements gameplay in Rust.); Qa Tester (Writes and runs tests, reports bugs.)\n";
        let ask = |prompt: &str| {
            let mut panel = AgentPanel::default();
            let invocation = invocation(Provider::Claude, &Route::Cli, &model, &[], &none);
            let ctx = egui::Context::default();
            panel.start(invocation, prompt, &dir, dir.join("none"), lineup, &ctx);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
            while panel.poll().is_none() {
                // It may look around first; nobody is there to allow more.
                for ask in std::mem::take(&mut panel.asks) {
                    eprintln!("denied: {}", ask.what());
                    assert!(panel.send(&ask.answer(false, false)));
                }
                let message = &panel.chat.messages[1];
                let late = std::time::Instant::now() > deadline;
                assert!(!late, "{}\n---\n{}", message.thoughts, message.text);
                thread::sleep(std::time::Duration::from_millis(20));
            }
            let message = panel.chat.messages[1].clone();
            eprintln!("{prompt}\n{}\n---\n{}\n", message.thoughts, message.text);
            (panel.take_handoff(), message.text)
        };
        // One agent's work, however much it is about the game.
        let (handoff, said) = ask("Make the player's jump 20% higher.");
        assert_eq!(handoff, None, "{said}");
        let (handoff, said) = ask("Why can't the player jump again in the air?");
        assert_eq!(handoff, None, "{said}");
        let (handoff, said) = ask(
            "Add a double jump to the player of this game: design how it should feel, implement it, and write and run tests for it.",
        );
        let (name, task, _) = handoff.unwrap_or_else(|| panic!("{said}"));
        assert_eq!(name, "Studio", "{said}");
        assert!(task.len() > "Add a double jump".len(), "{task}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn shows_the_organization_handed_the_task_as_a_child_session() {
        let besfa = std::env::temp_dir().join(format!("besfa-child-{}", std::process::id()));
        fs::create_dir_all(besfa.join("organizations")).unwrap();
        fs::write(besfa.join("organizations/Studio.besfa"), "()").unwrap();
        fs::write(
            besfa.join("organization-run.ron"),
            r#"(organization: "Studio", chat: 9)"#,
        )
        .unwrap();
        let mut harness = tab(None);
        harness.state_mut().org = OrgPanel::new(besfa.join("organizations"));
        let panel = &mut harness.state_mut().panel;
        panel.ask("Add a double jump.");
        panel.say("Design, code and test it.");
        panel.handed_off("Studio");
        panel.chat.messages.push(Message {
            text: "Jumps cost stamina.".into(),
            speaker: Some(("Game Designer".into(), [0x4F, 0x86, 0x5A])),
            ..Default::default()
        });
        panel.chat.id = 9;
        harness.run();
        assert!(harness.query_by_label("Child session · Studio").is_some());
        assert!(harness.query_by_label("Jumps cost stamina.").is_some());
        // Its run stopped before the tester: Continue goes on with it.
        assert!(harness.query_by_label("paused").is_some());
        assert!(harness.query_by_label("Continue").is_some());

        // The next prompt carries what the team did.
        let input = harness.state().panel.chat.input("Now a triple jump.");
        assert!(input.contains("You: Handed the task to Studio."), "{input}");
        assert!(input.contains("Game Designer: Jumps cost stamina."));

        // Once someone else speaks, the session above is no longer live.
        harness.state_mut().panel.ask("Thanks.");
        harness.run();
        assert!(harness.query_by_label("Child session · Studio").is_some());
        assert!(harness.query_by_label("paused").is_none());
        fs::remove_dir_all(&besfa).unwrap();
    }

    #[test]
    fn sends_every_file_of_the_chat_and_none_into_the_next() {
        let mut harness = tab(None);
        let panel = &mut harness.state_mut().panel;
        panel.chat.files = vec!["a.png".into()];
        panel.attached = vec!["b.txt".into()];
        panel.prompt = "Use these.".into();
        // The prompt panel grows to fit the attached file a frame late.
        harness.run();
        harness.run();
        harness.get_by_label("Send").click();
        harness.run();
        let files: Vec<PathBuf> = vec!["a.png".into(), "b.txt".into()];
        assert_eq!(harness.state().files, files);
        assert!(harness.state().sent.as_ref().unwrap().ends_with("b.txt"));

        let panel = &mut harness.state_mut().panel;
        panel.attached = vec!["c.txt".into()];
        panel.prompt = "half typed".into();
        panel.new_chat();
        assert!(panel.attached.is_empty() && panel.prompt.is_empty());
        assert!(panel.chat.files.is_empty());
    }

    #[test]
    fn picks_the_provider_and_model_next_to_the_prompt() {
        let mut harness = tab(None);
        harness.run();
        harness.get_by_role(egui::accesskit::Role::ComboBox).click();
        harness.run();
        harness.get_by_label("opus").click();
        harness.run();
        assert_eq!(harness.state().settings.model(Provider::Claude), "opus");

        harness.get_by_role(egui::accesskit::Role::ComboBox).click();
        harness.run();
        // Each provider lists its CLI default, in `Provider::ALL` order.
        harness
            .get_all_by_label("CLI default")
            .nth(1)
            .unwrap()
            .click();
        harness.run();
        assert_eq!(harness.state().settings.provider, Provider::OpenAi);
        assert_eq!(harness.state().settings.model(Provider::Claude), "opus");

        harness.get_by_role(egui::accesskit::Role::ComboBox).click();
        harness.run();
        // The last is the organization, under the heading of the same name.
        harness
            .get_all_by_label("Organization")
            .last()
            .unwrap()
            .click();
        harness.run();
        assert_eq!(harness.state().panel.organization(), Some("Organization"));
    }

    #[test]
    fn continues_a_paused_run_in_its_chat_after_a_restart() {
        let besfa = std::env::temp_dir().join(format!("besfa-paused-{}", std::process::id()));
        fs::create_dir_all(besfa.join("organizations")).unwrap();
        fs::write(besfa.join("organizations/Studio.besfa"), "()").unwrap();
        fs::write(
            besfa.join("organization-run.ron"),
            r#"(organization: "Studio", chat: 5, messages: [(user: true, text: "Add a jump."), (speaker: Some(("Producer", (1, 2, 3))))])"#,
        )
        .unwrap();
        // What the editor does when it opens the project.
        let org = OrgPanel::new(besfa.join("organizations"));
        let mut panel = AgentPanel::default();
        let paused = org.paused().unwrap();
        panel.restore(paused.chat(), &[], paused.organization());
        assert_eq!(panel.chat.title(), "Add a jump.");
        assert_eq!(panel.organization(), Some("Studio"));
        assert_eq!(panel.chat.messages[1].text, "Stopped.");
        // One the chat's agent handed over leaves the model the user picked.
        let handed = Message {
            child: Some("Studio".into()),
            ..Default::default()
        };
        let said = |user: bool| Message {
            user,
            text: "Add a jump.".into(),
            ..Default::default()
        };
        let mut other = AgentPanel::default();
        // Under the name it has since been renamed to, too.
        let run = [said(true), handed.clone(), said(false)];
        other.restore((6, &run), &[], "Game Team");
        assert_eq!(other.organization(), None);
        // Also when the user went on in that chat after it paused, as the
        // manual's "Stop, then ask it to do it itself" has them do.
        let mut went_on = AgentPanel::default();
        let run = [said(true), said(false), handed.clone(), said(false)];
        went_on.chat = Chat {
            id: 8,
            messages: [&run[..], &[said(true), said(false)]].concat(),
            ..Default::default()
        };
        went_on.restore((8, &run), &[], "Studio");
        assert_eq!(went_on.chat.messages.len(), 6);
        assert_eq!(went_on.organization(), None);
        // One sent to it straight after an earlier handoff picks it.
        let mut later = AgentPanel::default();
        let run = [said(true), handed, said(false), said(true), said(false)];
        later.restore((7, &run), &[], "Studio");
        assert_eq!(later.organization(), Some("Studio"));

        let mut harness = tab(None);
        harness.state_mut().org = org;
        harness.state_mut().panel = panel;
        harness.run();
        harness.get_by_label("Continue").click();
        harness.run();
        assert!(harness.state_mut().panel.take_continue());
        assert!(!harness.state_mut().panel.take_continue());

        // Another chat has nothing to continue.
        harness.get_by_label("+ New").click();
        harness.run();
        assert!(harness.query_by_label("Continue").is_none());
        fs::remove_dir_all(&besfa).unwrap();
    }

    #[test]
    fn shows_who_said_what_with_its_thoughts_folded() {
        let mut harness = tab(None);
        harness.state_mut().panel = AgentPanel::demo();
        harness.run();
        assert!(harness.query_by_label("Game Designer").is_some());
        assert!(
            harness
                .query_by_label("[Read] design/gdd/movement.md")
                .is_none()
        );
        harness.get_by_label("Thoughts (1)").click();
        harness.run();
        assert!(
            harness
                .query_by_label("[Read] design/gdd/movement.md")
                .is_some()
        );
        // An agent's turn goes into the next prompt under its name.
        let input = harness.state().panel.chat.input("Now a triple jump.");
        assert!(input.contains("Game Designer: A second jump"));
    }

    #[test]
    fn shows_a_reply_as_markdown() {
        let mut harness = tab(None);
        let panel = &mut harness.state_mut().panel;
        panel.say("## Done\n\n- Edited **src/player.rs**");
        harness.run();
        assert!(harness.query_by_label("Done").is_some());
        assert!(harness.query_by_label("src/player.rs").is_some());
        assert!(harness.query_by_label_contains("**").is_none());
        assert!(harness.query_by_label_contains("##").is_none());
    }

    #[test]
    fn draws_only_the_messages_near_the_view() {
        let mut harness = tab(None);
        let panel = &mut harness.state_mut().panel;
        for index in 0..300 {
            panel.say(&format!("Message number {index}"));
        }
        for _ in 0..4 {
            harness.run();
        }
        let panel = &harness.state().panel;
        assert_eq!(panel.heights.len(), 300);
        assert!(panel.heights.iter().all(|height| height.is_some()));
        assert!(
            harness
                .query_by_label_contains("Message number 299")
                .is_some()
        );
        assert!(
            harness
                .query_by_label_contains("Message number 0")
                .is_none()
        );
    }

    #[test]
    fn forgets_the_heights_when_another_chat_is_shown() {
        let mut harness = tab(None);
        harness.state_mut().panel.say("Still shown");
        harness.run();
        assert_eq!(harness.state().panel.heights.len(), 1);
        harness.state_mut().panel.new_chat();
        assert!(harness.state().panel.heights.is_empty());
    }

    #[test]
    fn opens_only_web_links_from_a_reply() {
        let mut harness = tab(None);
        let panel = &mut harness.state_mut().panel;
        panel.say("Run [this](file:///C:/nonexistent.exe).");
        harness.run();
        harness.get_by_label("this").click();
        harness.step();
        // Taken by the reply, which opens nothing but web links.
        let open =
            |command: &egui::OutputCommand| matches!(command, egui::OutputCommand::OpenUrl(_));
        assert!(!harness.output().platform_output.commands.iter().any(open));
        let pr = "https://github.com/ski0090/besfa/pull/43";
        assert_eq!(web_link(pr).as_deref(), Some(pr));
        assert_eq!(web_link("file:///C:/nonexistent.exe"), None);
        // Explorer would take what follows a comma as a second target.
        let comma = web_link(r"https://example.com/,C:\nonexistent.exe");
        let encoded = r"https://example.com/%2CC:\nonexistent.exe";
        assert_eq!(comma.as_deref(), Some(encoded));
        assert_eq!(
            web_link(r#"https://example.com/" C:\nonexistent.exe"#),
            None
        );
    }
}
