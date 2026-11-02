//! Which AI agent the editor's prompts go to: a provider, reached through its
//! command-line tool or through its API with a key from an environment
//! variable. The key itself is never written to the settings file. The
//! Agent tab sends a prompt to the agent's command-line tool, run in the
//! project.

use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use eframe::egui::{self, Color32, Key, Ui};
use serde::{Deserialize, Serialize};

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
            Provider::OpenAi => "ChatGPT (OpenAI)",
            Provider::Gemini => "Gemini",
        }
    }

    fn default_setting(self) -> Setting {
        let (command, key_env, model) = match self {
            Provider::Claude => ("claude", "ANTHROPIC_API_KEY", "claude-sonnet-5-5"),
            Provider::OpenAi => ("codex", "OPENAI_API_KEY", ""),
            Provider::Gemini => ("gemini", "GEMINI_API_KEY", ""),
        };
        Setting {
            command: command.into(),
            key_env: key_env.into(),
            model: model.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Access {
    #[default]
    Cli,
    Api,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Setting {
    /// The command-line program that runs the agent.
    pub command: String,
    /// The environment variable that holds the API key.
    pub key_env: String,
    /// The model to ask for; empty for the provider's default.
    pub model: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct AgentSettings {
    pub provider: Provider,
    pub access: Access,
    /// Per-provider settings. A provider missing here uses its defaults.
    pub settings: BTreeMap<Provider, Setting>,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            provider: Provider::default(),
            access: Access::default(),
            settings: BTreeMap::new(),
        }
    }
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

    /// `provider`'s settings as they are, filled in with the defaults.
    pub fn setting(&self, provider: Provider) -> Setting {
        self.settings
            .get(&provider)
            .cloned()
            .unwrap_or_else(|| provider.default_setting())
    }

    /// The program and arguments that run the chosen agent headless in the
    /// project with the prompt on stdin, or why it cannot run.
    pub fn invocation(&self) -> Result<(String, Vec<String>), &'static str> {
        if self.access == Access::Api {
            return Err(
                "Sending with an API key is not supported yet. Choose the command-line tool in File > Agent Settings.",
            );
        }
        let setting = self.setting(self.provider);
        // Nobody can answer a permission prompt, so each may edit files in
        // the project without asking and nothing more.
        let args: &[&str] = match self.provider {
            Provider::Claude => &["-p", "--permission-mode", "acceptEdits"],
            Provider::OpenAi => &[
                "exec",
                "--sandbox",
                "workspace-write",
                "--skip-git-repo-check",
            ],
            // The prompt flag makes it headless; stdin comes before it.
            Provider::Gemini => &["--prompt", "", "--approval-mode", "auto_edit"],
        };
        let mut args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        let model = setting.model.trim();
        if !model.is_empty() {
            args.extend(["--model".into(), model.into()]);
        }
        Ok((setting.command.trim().into(), args))
    }
}

/// The Agent tab: a prompt box and the latest run's output.
#[derive(Default)]
pub struct AgentPanel {
    prompt: String,
    run: Option<CliProcess>,
    output: VecDeque<String>,
    /// A file the run may change, and when it was changed before the run.
    watched: Option<(PathBuf, Option<SystemTime>)>,
}

impl AgentPanel {
    pub fn running(&self) -> bool {
        self.run.is_some()
    }

    /// Runs the agent on `prompt` in `directory`. `poll` tells once it
    /// exits whether it changed `watch`.
    pub fn start(
        &mut self,
        settings: &AgentSettings,
        prompt: &str,
        directory: &Path,
        watch: PathBuf,
        ctx: &egui::Context,
    ) {
        self.output.clear();
        self.output
            .extend(prompt.lines().map(|line| format!("> {line}")));
        let (program, args) = match settings.invocation() {
            Ok(invocation) => invocation,
            Err(reason) => return self.output.push_back(reason.into()),
        };
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        match CliProcess::start(directory, &program, &args, &[], ctx) {
            Ok(mut run) => {
                run.send(prompt);
                run.end_input();
                self.watched = Some((watch.clone(), modified(&watch)));
                self.run = Some(run);
            }
            Err(error) => self
                .output
                .push_back(format!("Could not start {program}: {error}")),
        }
    }

    /// Takes what the run wrote. Once it exits, returns whether it changed
    /// the watched file.
    pub fn poll(&mut self) -> Option<bool> {
        for output in self.run.as_ref()?.poll() {
            match output {
                Output::Line(line) => self.output.push_back(line),
                Output::Exit(code) => {
                    self.output.push_back(match code {
                        0 => "Done.".into(),
                        code => format!("Exited with code {code}."),
                    });
                    self.run = None;
                    let (file, before) = self.watched.take()?;
                    return Some(modified(&file) != before);
                }
            }
        }
        None
    }

    /// Shows the tab. Returns the prompt to send, unless `blocked` says why
    /// it cannot go.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        settings: &AgentSettings,
        blocked: Option<&str>,
    ) -> Option<String> {
        let mut send = None;
        egui::Panel::bottom("agent_prompt").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let can_send =
                    blocked.is_none() && !self.running() && !self.prompt.trim().is_empty();
                let button = ui.add_enabled(can_send, egui::Button::new("Send"));
                let field = ui.add(
                    egui::TextEdit::multiline(&mut self.prompt)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .hint_text("Ask the agent to change the project. Ctrl+Enter sends."),
                );
                let shortcut = field.has_focus()
                    && ui.input(|input| input.modifiers.command && input.key_pressed(Key::Enter));
                if can_send && (button.clicked() || shortcut) {
                    send = Some(std::mem::take(&mut self.prompt).trim().to_string());
                }
            });
            ui.horizontal(|ui| {
                let access = match settings.access {
                    Access::Cli => "command-line tool",
                    Access::Api => "API key",
                };
                let label = format!("{} with its {access}", settings.provider.label());
                ui.label(egui::RichText::new(label).color(panels::MUTED).small());
                if let Some(run) = &self.run {
                    ui.spinner();
                    if ui.small_button("Stop").clicked() {
                        run.stop();
                    }
                }
                if let Some(reason) = blocked {
                    ui.colored_label(panels::WARNING, reason);
                }
            });
        });
        panels::log(ui, &self.output);
        send
    }
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

/// Shows the settings window while `open`. Returns true when a value
/// changed, so the caller saves the file.
pub fn window(ctx: &egui::Context, open: &mut bool, settings: &mut AgentSettings) -> bool {
    let mut changed = false;
    egui::Window::new("Agent Settings")
        .open(open)
        .resizable(false)
        .show(ctx, |ui| {
            changed = show(ui, settings);
        });
    changed
}

fn show(ui: &mut Ui, settings: &mut AgentSettings) -> bool {
    let before = settings.clone();
    ui.horizontal(|ui| {
        ui.label("Provider");
        for provider in Provider::ALL {
            ui.radio_value(&mut settings.provider, provider, provider.label());
        }
    });
    ui.horizontal(|ui| {
        ui.label("Connect with");
        ui.radio_value(&mut settings.access, Access::Cli, "Command-line tool");
        ui.radio_value(&mut settings.access, Access::Api, "API key");
    });
    ui.separator();

    let provider = settings.provider;
    let mut setting = settings.setting(provider);
    egui::Grid::new("agent_setting")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("CLI command");
            ui.add(egui::TextEdit::singleline(&mut setting.command).desired_width(260.0));
            ui.end_row();
            ui.label("");
            cli_status(ui, &setting.command);
            ui.end_row();

            ui.label("API key variable");
            ui.add(egui::TextEdit::singleline(&mut setting.key_env).desired_width(260.0));
            ui.end_row();
            ui.label("");
            key_status(ui, &setting.key_env);
            ui.end_row();

            ui.label("Model");
            ui.add(
                egui::TextEdit::singleline(&mut setting.model)
                    .hint_text("empty for the default")
                    .desired_width(260.0),
            );
            ui.end_row();
        });
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "The API key is read from that environment variable when a prompt is sent. It is not saved here.",
        )
        .color(panels::MUTED)
        .small(),
    );

    settings.settings.insert(provider, setting);
    *settings != before
}

fn cli_status(ui: &mut Ui, command: &str) {
    let found = process::find_program_on_path(command.trim()).is_some();
    status(ui, found, "found on PATH", "not found on PATH");
}

fn key_status(ui: &mut Ui, key_env: &str) {
    let set = std::env::var(key_env.trim()).is_ok_and(|value| !value.is_empty());
    status(ui, set, "set", "not set");
}

fn status(ui: &mut Ui, good: bool, yes: &str, no: &str) {
    let (text, color): (&str, Color32) = if good {
        (yes, panels::GOOD)
    } else {
        (no, panels::WARNING)
    };
    ui.colored_label(color, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_provider_falls_back_to_its_defaults() {
        let settings = AgentSettings::default();
        assert_eq!(settings.setting(Provider::Claude).command, "claude");
        assert_eq!(settings.setting(Provider::Gemini).key_env, "GEMINI_API_KEY");
    }

    #[test]
    fn keeps_the_choices_and_settings_across_a_restart() {
        let mut settings = AgentSettings {
            provider: Provider::OpenAi,
            access: Access::Api,
            ..Default::default()
        };
        settings.settings.insert(
            Provider::OpenAi,
            Setting {
                command: "codex".into(),
                key_env: "MY_KEY".into(),
                model: "m".into(),
            },
        );
        let text = ron::to_string(&settings).unwrap();
        let back: AgentSettings = ron::from_str(&text).unwrap();
        assert_eq!(back, settings);
    }

    #[test]
    fn runs_each_cli_headless_with_the_model_when_one_is_set() {
        let mut settings = AgentSettings::default();
        let (program, args) = settings.invocation().unwrap();
        assert_eq!(program, "claude");
        assert_eq!(
            args,
            [
                "-p",
                "--permission-mode",
                "acceptEdits",
                "--model",
                "claude-sonnet-5-5"
            ]
        );

        settings.provider = Provider::Gemini;
        let (program, args) = settings.invocation().unwrap();
        assert_eq!(program, "gemini");
        assert_eq!(args, ["--prompt", "", "--approval-mode", "auto_edit"]);

        settings.provider = Provider::OpenAi;
        assert_eq!(settings.invocation().unwrap().1[0], "exec");

        settings.access = Access::Api;
        assert!(settings.invocation().is_err());
    }

    #[test]
    fn sends_the_prompt_only_when_nothing_blocks_it() {
        use egui_kittest::{Harness, kittest::Queryable};

        struct State {
            panel: AgentPanel,
            blocked: Option<&'static str>,
            sent: Option<String>,
        }
        let mut harness = Harness::builder().build_ui_state(
            |ui, state: &mut State| {
                let settings = AgentSettings::default();
                if let Some(prompt) = state.panel.ui(ui, &settings, state.blocked) {
                    state.sent = Some(prompt);
                }
            },
            State {
                panel: AgentPanel::default(),
                blocked: Some("Save the scene first."),
                sent: None,
            },
        );
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
    fn picking_the_api_key_access_updates_the_settings() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut harness = Harness::builder().build_ui_state(
            |ui, settings: &mut AgentSettings| {
                show(ui, settings);
            },
            AgentSettings::default(),
        );
        harness.run();
        harness.get_by_label("API key").click();
        harness.run();
        assert_eq!(harness.state().access, Access::Api);
    }
}
