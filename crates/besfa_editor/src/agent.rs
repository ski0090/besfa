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
    fn models(self) -> &'static [&'static str] {
        match self {
            Provider::Claude => &["fable", "opus", "sonnet", "haiku"],
            Provider::OpenAi | Provider::Gemini => &[],
        }
    }

    /// Headless, with the prompt on stdin. Nobody can answer a permission
    /// prompt, so each may edit files in the project without asking and
    /// nothing more.
    fn args(self) -> &'static [&'static str] {
        match self {
            Provider::Claude => &["-p", "--permission-mode", "acceptEdits"],
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
        (_, Cli::Checking) => Err("Checking the command-line tool."),
        (_, Cli::Missing) => Err("Install its command-line tool first."),
        (Access::Cli, Cli::LoggedIn) => Ok(Route::Cli),
        (Access::Cli, Cli::LoggedOut) => Err(
            "Log in to its command-line tool, or switch it to an API key in File > Agent Settings.",
        ),
        (Access::Key, _) if key.trim().is_empty() => {
            Err("Enter its API key in File > Agent Settings.")
        }
        (Access::Key, _) => Ok(Route::Key(key.trim().into())),
    }
}

/// The program, arguments and environment that send a prompt.
pub fn invocation(
    provider: Provider,
    route: &Route,
    model: &str,
) -> (String, Vec<String>, Vec<(String, String)>) {
    let mut args: Vec<String> = provider.args().iter().map(|arg| arg.to_string()).collect();
    if !model.is_empty() {
        args.extend(["--model".into(), model.into()]);
    }
    let env = match route {
        Route::Cli => Vec::new(),
        Route::Key(key) => vec![(provider.key_env().into(), key.clone())],
    };
    (provider.command().into(), args, env)
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
    egui::Window::new("Agent Settings")
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
                ui.selectable_value(&mut access, Access::Key, "API key");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match agents.ready(provider, access) {
                        None => ui.colored_label(panels::MUTED, "checking"),
                        Some(true) => ui.colored_label(panels::GOOD, "ready"),
                        Some(false) => ui.colored_label(panels::WARNING, "can't send yet"),
                    };
                });
            });
            let command = provider.command();
            let note = |ui: &mut Ui, text: String| {
                ui.label(RichText::new(text).color(panels::MUTED).small());
            };
            match (access, agents.cli(provider)) {
                (_, Cli::Checking) => note(ui, format!("Checking `{command}`.")),
                (_, Cli::Missing) => note(ui, format!("`{command}` is not installed.")),
                (Access::Cli, Cli::LoggedOut) => note(
                    ui,
                    format!(
                        "`{command}` is not logged in. Run `{command}` in a terminal to log in."
                    ),
                ),
                (Access::Cli, Cli::LoggedIn) => note(ui, format!("`{command}` is logged in.")),
                (Access::Key, _) => {}
            }
            if access == Access::Key {
                let key = agents.keys.entry(provider).or_default();
                let field = ui.add(
                    egui::TextEdit::singleline(key)
                        .password(true)
                        .hint_text(format!("{} API key", provider.label()))
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
            RichText::new("Keys stay in the Windows Credential Manager.")
                .color(panels::MUTED)
                .small(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Check again").clicked() {
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
}

impl Chat {
    fn title(&self) -> String {
        self.messages
            .iter()
            .find(|message| message.user)
            .and_then(|message| message.text.lines().next())
            .map_or_else(|| "New chat".into(), |line| line.chars().take(30).collect())
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
@assets/prefabs/rock-pile.scn.ron",
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
                run.send(prompt);
                run.end_input();
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
    fn show_chat(&mut self, chat: Chat) {
        let current = std::mem::replace(&mut self.chat, chat);
        if !current.messages.is_empty() {
            self.history.push(current);
        }
    }

    /// Shows the window's contents, where the provider and model are picked
    /// too. Returns the prompt to send, unless `blocked` says why it cannot
    /// go.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        settings: &mut AgentSettings,
        agents: &Agents,
        blocked: Option<&str>,
    ) -> Option<String> {
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
                    "Show history"
                } else {
                    "Hide history"
                };
                if ui.small_button(label).clicked() {
                    self.sidebar_closed = !self.sidebar_closed;
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
            egui::Button::new("+ New Agent").min_size(egui::vec2(ui.available_width(), 28.0));
        if ui.add_enabled(!self.running(), new_agent).clicked() {
            self.new_chat();
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new("History")
                .small()
                .strong()
                .color(panels::MUTED),
        );
        let mut open = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.history.is_empty() {
                ui.colored_label(panels::MUTED, "No earlier chats.");
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
                    ui.colored_label(panels::MUTED, "Ask the agent to change the project.");
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
    /// it. Returns the prompt to send.
    fn prompt_card(
        &mut self,
        ui: &mut Ui,
        settings: &mut AgentSettings,
        agents: &Agents,
        blocked: Option<&str>,
    ) -> Option<String> {
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
                        .hint_text("Ask the agent to change the project. Ctrl+Enter sends."),
                );
                let shortcut = field.has_focus()
                    && ui.input(|input| input.modifiers.command && input.key_pressed(Key::Enter));
                ui.horizontal(|ui| {
                    if ui.button("+ Attach").clicked()
                        && let Some(files) = rfd::FileDialog::new().pick_files()
                    {
                        self.attached.extend(files);
                    }
                    model_picker(ui, settings, agents);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(run) = &self.run {
                            if ui.button("Stop").clicked() {
                                run.stop();
                            }
                            return;
                        }
                        let can_send = blocked.is_none() && !self.prompt.trim().is_empty();
                        let button = ui.add_enabled(can_send, egui::Button::new("Send"));
                        if can_send && (button.clicked() || shortcut) {
                            let mut prompt = std::mem::take(&mut self.prompt).trim().to_string();
                            // The CLIs read an `@path` as a file to look at.
                            for file in self.attached.drain(..) {
                                prompt.push_str(&format!("\n@{}", file.display()));
                            }
                            send = Some(prompt);
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

fn model_label(model: &str) -> &str {
    if model.is_empty() {
        "CLI default"
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
                    (None, _) => ("checking", panels::MUTED),
                    (Some(false), _) => ("can't send yet", panels::WARNING),
                    (Some(true), Access::Cli) => ("CLI", panels::MUTED),
                    (Some(true), Access::Key) => ("API key", panels::MUTED),
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
        let (program, args, env) = invocation(Provider::Claude, &Route::Cli, "opus");
        assert_eq!(program, "claude");
        assert_eq!(
            args,
            ["-p", "--permission-mode", "acceptEdits", "--model", "opus"]
        );
        assert!(env.is_empty());

        let (program, args, env) = invocation(Provider::OpenAi, &Route::Key("sk".into()), "");
        assert_eq!(program, "codex");
        assert_eq!(args[0], "exec");
        assert_eq!(env, [("CODEX_API_KEY".to_string(), "sk".to_string())]);

        let (_, args, _) = invocation(Provider::Gemini, &Route::Cli, "");
        assert_eq!(args, ["--prompt", "", "--approval-mode", "auto_edit"]);
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
    }

    fn tab(blocked: Option<&'static str>) -> Harness<'static, Tab> {
        Harness::builder().build_ui_state(
            |ui, tab: &mut Tab| {
                let blocked = tab.blocked;
                if let Some(prompt) = tab.panel.ui(ui, &mut tab.settings, &tab.agents, blocked) {
                    tab.sent = Some(prompt);
                }
            },
            Tab {
                panel: AgentPanel::default(),
                settings: AgentSettings::default(),
                agents: agents(Cli::LoggedIn),
                blocked,
                sent: None,
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
