//! The editor's language setting. The languages themselves are in
//! `besfa_ui::i18n`, which the panels a game draws read too.

use std::{
    fs,
    path::{Path, PathBuf},
};

pub use besfa_ui::i18n::t;
use besfa_ui::i18n::{is_korean, set_korean};
use eframe::egui::Ui;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    Korean,
}

impl Language {
    const ALL: [Self; 2] = [Self::English, Self::Korean];

    /// Each language named in itself, so anyone can find theirs.
    fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Korean => "한국어",
        }
    }

    pub fn current() -> Self {
        if is_korean() {
            Self::Korean
        } else {
            Self::English
        }
    }

    fn set(self) {
        set_korean(self == Self::Korean);
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
}

impl Settings {
    /// The settings file, next to the recent projects list.
    fn file() -> PathBuf {
        let base = std::env::var("APPDATA").or_else(|_| std::env::var("HOME"));
        Path::new(&base.unwrap_or_else(|_| ".".into())).join("Besfa/settings.ron")
    }

    fn read(file: &Path) -> Self {
        fs::read_to_string(file)
            .ok()
            .and_then(|text| ron::from_str::<Self>(&text).ok())
            .unwrap_or_default()
    }

    fn write(&self, file: &Path) -> Result<(), String> {
        if let Some(folder) = file.parent() {
            fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        let text = ron::to_string(self).map_err(|error| error.to_string())?;
        fs::write(file, text).map_err(|error| error.to_string())
    }
}

/// Shows the editor in the language the settings file keeps.
pub fn load() {
    Settings::read(&Settings::file()).language.set();
}

/// The Settings menu: picks the language and keeps it, then shows `more`.
/// Returns why the language could not be kept.
pub fn settings_menu(ui: &mut Ui, more: impl FnOnce(&mut Ui)) -> Option<String> {
    let mut error = None;
    ui.menu_button(t("Settings"), |ui| {
        ui.menu_button(t("Language"), |ui| {
            for language in Language::ALL {
                let current = Language::current() == language;
                if ui.radio(current, language.label()).clicked() && !current {
                    language.set();
                    error = Settings { language }.write(&Settings::file()).err();
                    ui.close();
                }
            }
        });
        more(ui);
    });
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let file = std::env::temp_dir().join(format!("besfa_settings_{}.ron", std::process::id()));
        let settings = Settings {
            language: Language::Korean,
        };
        settings.write(&file).unwrap();
        assert_eq!(Settings::read(&file), settings);
        let _ = fs::remove_file(&file);
        assert_eq!(Settings::read(&file), Settings::default());
    }
}
