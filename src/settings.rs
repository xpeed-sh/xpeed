use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: xpeed_i18n::Language,
    pub font_size: f32,
    pub auto_reconnect: bool,
    pub scrollback_lines: usize,
    pub sidebar: bool,
    pub accent: usize,
    pub mode: crate::palette::Mode,
    pub dark_theme: usize,
    pub light_theme: usize,
    pub density: crate::palette::Density,
    pub follow_terminal: bool,
    pub record_commands: bool,
    pub attach_terminal_to_ai: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: xpeed_i18n::Language::English,
            font_size: 15.0,
            auto_reconnect: true,
            scrollback_lines: 5000,
            sidebar: true,
            accent: 0,
            mode: crate::palette::Mode::Dark,
            dark_theme: 0,
            light_theme: 0,
            density: crate::palette::Density::Comfortable,
            follow_terminal: true,
            record_commands: true,
            attach_terminal_to_ai: false,
        }
    }
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("xpeed").join("settings.json"))
    }

    pub fn load() -> Self {
        let mut settings: Self = Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        if !settings.font_size.is_finite() {
            settings.font_size = 15.0;
        }
        settings.font_size = settings.font_size.clamp(11.0, 24.0);
        settings.scrollback_lines = settings.scrollback_lines.clamp(100, 50000);
        settings.accent = settings.accent.min(2);
        settings.dark_theme = settings.dark_theme.min(2);
        settings.light_theme = settings.light_theme.min(2);
        settings
    }

    pub fn save(&self) -> Result<(), String> {
        let path =
            Self::path().ok_or(xpeed_i18n::t("No se encontró la carpeta de configuración"))?;
        let parent = path
            .parent()
            .ok_or(xpeed_i18n::t("Ruta de configuración inválida"))?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temporary = parent.join("settings.tmp");
        std::fs::write(
            &temporary,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(temporary, path).map_err(|e| e.to_string())
    }

    pub fn palette(&self) -> crate::palette::Palette {
        crate::palette::Palette::new(
            self.mode,
            if self.mode == crate::palette::Mode::Dark {
                self.dark_theme
            } else {
                self.light_theme
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_settings_default_to_english_without_losing_preferences() {
        let settings: Settings = serde_json::from_str(
            r#"{"font_size":18,"follow_terminal":false,"record_commands":false}"#,
        )
        .unwrap();
        assert_eq!(settings.language, xpeed_i18n::Language::English);
        assert_eq!(settings.font_size, 18.0);
        assert_eq!(settings.density, crate::palette::Density::Comfortable);
        assert!(!settings.follow_terminal && !settings.record_commands);
    }
    #[test]
    fn language_round_trips_and_unknown_locales_fall_back_to_english() {
        let settings = Settings {
            language: xpeed_i18n::Language::Spanish,
            font_size: 17.0,
            ..Default::default()
        };
        let json = serde_json::to_value(settings).unwrap();
        assert_eq!(json["language"], "es");
        let loaded: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(loaded.language, xpeed_i18n::Language::Spanish);
        let future: Settings = serde_json::from_str(r#"{"language":"fr","font_size":17}"#).unwrap();
        assert_eq!(future.language, xpeed_i18n::Language::English);
        assert_eq!(future.font_size, 17.0);
    }
}
