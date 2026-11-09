//! The AI agent the editor's prompts go to. Each provider's command-line tool
//! runs headless in the project, either logged in on its own or with an API
//! key the editor keeps in the Windows Credential Manager, never in a file.
//! The Agent tab picks the provider and model and sends the prompt.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::mpsc::{self, Receiver},
    thread,
    time::SystemTime,
};

use eframe::egui::{self, Key, RichText, Ui};
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
    panels,
    process::{self, CliProcess, Output},
};

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

    fn command(self) -> &'static str {
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

    /// Headless, with the prompt on stdin. Nobody can answer a permission
    /// prompt, so each may edit files in the project without asking and
    /// nothing more. Claude may also call the `besfa mcp` server a game's
    /// .mcp.json starts; headless, it loads that server unasked.
    /// ponytail: Codex and Gemini do not read .mcp.json; add theirs when asked.
    fn args(self) -> &'static [&'static str] {
        match self {
            Provider::Claude => &[
                "-p",
                "--permission-mode",
                "acceptEdits",
                "--allowedTools",
                "mcp__besfa",
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
            "Log in to its command-line tool, or switch it to an API key in File > Agent Settings.",
        )),
        (Access::Key, _) if key.trim().is_empty() => {
            Err(t("Enter its API key in File > Agent Settings."))
        }
        (Access::Key, _) => Ok(Route::Key(key.trim().into())),
    }
}

/// The program, arguments and environment that send a prompt with `files`
/// attached.
pub fn invocation(
    provider: Provider,
    route: &Route,
    model: &str,
    files: &[PathBuf],
) -> (String, Vec<String>, Vec<(String, String)>) {
    let mut args: Vec<String> = provider.args().iter().map(|arg| arg.to_string()).collect();
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
    let env = match route {
        Route::Cli => Vec::new(),
        Route::Key(key) => vec![(provider.key_env().into(), key.clone())],
    };
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

/// The provider and model the Agent tab last picked, each provider's model
/// (empty for the CLI's default), and how each provider is reached.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct AgentSettings {
    pub provider: Provider,
    pub models: BTreeMap<Provider, String>,
    pub access: BTreeMap<Provider, Access>,
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
}

/// Each provider's CLI state and saved API key.
pub struct Agents {
    cli: BTreeMap<Provider, Cli>,
    keys: BTreeMap<Provider, String>,
    checks: Option<Receiver<(Provider, Cli)>>,
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

/// One turn of a chat: what the user sent, or what the agent printed back.
struct Message {
    user: bool,
    text: String,
}

#[derive(Default)]
struct Chat {
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
            let who = if message.user { "User" } else { "You" };
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
    run: Option<CliProcess>,
    chat: Chat,
    /// Earlier chats, newest last. ponytail: kept until the editor closes;
    /// save them under the project if they should survive a restart.
    history: Vec<Chat>,
    sidebar_closed: bool,
    /// Files to send with the next prompt, by path.
    attached: Vec<PathBuf>,
    /// The open file dialog's pick, once the user makes it.
    picking: Option<Receiver<Option<Vec<PathBuf>>>>,
    /// A file the run may change, and when it was changed before the run.
    watched: Option<(PathBuf, Option<SystemTime>)>,
}

impl AgentPanel {
    pub fn running(&self) -> bool {
        self.run.is_some()
    }

    /// A panel with a chat and an earlier one, for the manual's screenshot.
    #[cfg(test)]
    pub fn demo() -> Self {
        let chat = |turns: &[(bool, &str)]| Chat {
            messages: turns
                .iter()
                .map(|(user, text)| Message {
                    user: *user,
                    text: text.to_string(),
                })
                .collect(),
            ..Default::default()
        };
        let mut panel = Self::default();
        panel.history = vec![
            chat(&[(true, "Make the terrain greener")]),
            chat(&[(true, "Add a spawn point for the zombie")]),
        ];
        panel.chat = chat(&[
            (
                true,
                "Put a rock pile next to the oak tree.

Attached files:
assets/prefabs/rock-pile.scn.ron",
            ),
            (
                false,
                "I placed rock-pile.scn.ron 2 meters east of the oak tree and saved the scene.",
            ),
        ]);
        panel.attached = vec!["assets/models/small-stones.glb".into()];
        panel
    }

    /// Runs `invocation` on `prompt` in `directory`. `poll` tells once it
    /// exits whether it changed `watch`.
    pub fn start(
        &mut self,
        (program, args, env): (String, Vec<String>, Vec<(String, String)>),
        prompt: &str,
        directory: &Path,
        watch: PathBuf,
        ctx: &egui::Context,
    ) {
        let input = self.chat.input(prompt);
        self.chat.messages.push(Message {
            user: true,
            text: prompt.into(),
        });
        self.chat.messages.push(Message {
            user: false,
            text: String::new(),
        });
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let env: Vec<(&str, &str)> = env
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        match CliProcess::start(directory, &program, &args, &env, ctx) {
            Ok(mut run) => {
                run.send_all(input);
                self.watched = Some((watch.clone(), modified(&watch)));
                self.run = Some(run);
            }
            Err(error) => self.reply(&format!("Could not start {program}: {error}")),
        }
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
                Output::Line(line) => self.reply(&line),
                Output::Exit(code) => {
                    if code != 0 {
                        self.reply(&format!("Exited with code {code}."));
                    } else if self.chat.messages.last().is_some_and(|m| m.text.is_empty()) {
                        // Shows it finished, and keeps an empty turn out of
                        // the chat the next prompt resends.
                        self.reply("Done.");
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
    fn show_chat(&mut self, chat: Chat) {
        self.attached.clear();
        self.prompt.clear();
        self.picking = None;
        let current = std::mem::replace(&mut self.chat, chat);
        if !current.messages.is_empty() {
            self.history.push(current);
        }
    }

    /// Shows the window's contents, where the provider and model are picked
    /// too. Returns the prompt to send and every file attached in this chat,
    /// unless `blocked` says why it cannot go.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        settings: &mut AgentSettings,
        agents: &Agents,
        blocked: Option<&str>,
    ) -> Option<(String, Vec<PathBuf>)> {
        if !self.sidebar_closed {
            egui::Panel::left("agent_history")
                .default_size(190.0)
                .show(ui, |ui| self.sidebar(ui));
        }
        let send = egui::Panel::bottom("agent_prompt")
            .show(ui, |ui| self.prompt_card(ui, settings, agents, blocked))
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
                if ui.add_enabled(!self.running(), new).clicked() {
                    self.new_chat();
                }
                ui.label(RichText::new(self.chat.title()).strong());
            });
            ui.separator();
            self.messages(ui);
        });
        send
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        let new_agent =
            egui::Button::new(t("+ New Agent")).min_size(egui::vec2(ui.available_width(), 28.0));
        if ui.add_enabled(!self.running(), new_agent).clicked() {
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
                if ui.add_enabled(!self.running(), row).clicked() {
                    open = Some(index);
                }
            }
        });
        if let Some(index) = open {
            let chat = self.history.remove(index);
            self.show_chat(chat);
        }
    }

    fn messages(&self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.add_space(4.0);
                if self.chat.messages.is_empty() {
                    ui.colored_label(panels::MUTED, t("Ask the agent to change the project."));
                }
                let last = self.chat.messages.len().wrapping_sub(1);
                for (index, message) in self.chat.messages.iter().enumerate() {
                    if message.user {
                        egui::Frame::new()
                            .fill(ui.visuals().extreme_bg_color)
                            .corner_radius(8.0)
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(&message.text);
                            });
                    } else if message.text.is_empty() && index == last && self.running() {
                        ui.spinner();
                    } else {
                        ui.label(RichText::new(&message.text).color(panels::TEXT));
                    }
                    ui.add_space(8.0);
                }
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
                    model_picker(ui, settings, agents);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(run) = &self.run {
                            if ui.button(t("Stop")).clicked() {
                                run.stop();
                            }
                            return;
                        }
                        let can_send = blocked.is_none() && !self.prompt.trim().is_empty();
                        let button = ui.add_enabled(can_send, egui::Button::new(t("Send")));
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

pub fn model_label(model: &str) -> &str {
    if model.is_empty() {
        t("CLI default")
    } else {
        model
    }
}

/// One list of every provider's models: each provider's name and how it
/// would be reached, then its models by name alone.
fn model_picker(ui: &mut Ui, settings: &mut AgentSettings, agents: &Agents) {
    let provider = settings.provider;
    let selected = format!(
        "{} · {}",
        provider.label(),
        model_label(settings.model(provider))
    );
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
                        let current =
                            settings.provider == provider && settings.model(provider) == model;
                        if ui.selectable_label(current, model_label(model)).clicked() {
                            settings.provider = provider;
                            settings.models.insert(provider, model.into());
                        }
                    }
                });
            }
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
        let (program, args, env) = invocation(Provider::Claude, &Route::Cli, "opus", &[]);
        assert_eq!(program, "claude");
        assert_eq!(
            args,
            [
                "-p",
                "--permission-mode",
                "acceptEdits",
                "--allowedTools",
                "mcp__besfa",
                "--model",
                "opus"
            ]
        );
        assert!(env.is_empty());

        let (program, args, env) = invocation(Provider::OpenAi, &Route::Key("sk".into()), "", &[]);
        assert_eq!(program, "codex");
        assert_eq!(args[0], "exec");
        assert_eq!(env, [("CODEX_API_KEY".to_string(), "sk".to_string())]);

        let (_, args, _) = invocation(Provider::Gemini, &Route::Cli, "", &[]);
        assert_eq!(args, ["--prompt", "", "--approval-mode", "auto_edit"]);
    }

    #[test]
    fn lets_each_cli_reach_the_attached_files() {
        let files = [
            PathBuf::from(r"C:\My Art\ref.PNG"),
            PathBuf::from(r"C:\My Art\notes.txt"),
            PathBuf::from(r"C:\My Art\a,b.png"),
        ];
        let (_, args, _) = invocation(Provider::Claude, &Route::Cli, "", &files);
        assert_eq!(args[5..], ["--add-dir", r"C:\My Art"]);
        let (_, args, _) = invocation(Provider::Gemini, &Route::Cli, "", &files);
        assert_eq!(args[4..], ["--include-directories", r"C:\My Art"]);
        let (_, args, _) = invocation(Provider::OpenAi, &Route::Cli, "", &files);
        assert_eq!(args[4..], [r"--image=C:\My Art\ref.PNG"]);
    }

    #[test]
    fn sends_the_chat_so_far_with_the_next_prompt() {
        let chat = Chat {
            messages: vec![
                Message {
                    user: true,
                    text: "Make the tower taller.".into(),
                },
                Message {
                    user: false,
                    text: "Done.".into(),
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
    }

    fn tab(blocked: Option<&'static str>) -> Harness<'static, Tab> {
        Harness::builder().build_ui_state(
            |ui, tab: &mut Tab| {
                let blocked = tab.blocked;
                if let Some(prompt) = tab.panel.ui(ui, &mut tab.settings, &tab.agents, blocked) {
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
            },
        )
    }

    #[test]
    fn keeps_earlier_chats_and_brings_one_back() {
        let said = |text: &str| Chat {
            messages: vec![Message {
                user: true,
                text: text.into(),
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
        panel.start(quiet, "Hi", Path::new("."), "none".into(), &ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while panel.poll().is_none() && std::time::Instant::now() < deadline {
            thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(panel.chat.messages[1].text, "Done.");
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
    }
}
