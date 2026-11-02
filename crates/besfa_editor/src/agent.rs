//! Which AI agent the editor's prompts go to: a provider, reached through its
//! command-line tool or through its API with a key from an environment
//! variable. The key itself is never written to the settings file.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use eframe::egui::{self, Color32, Ui};
use serde::{Deserialize, Serialize};

use crate::{panels, process};

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
