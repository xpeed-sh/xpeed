use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{
        OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "es")]
    Spanish,
    #[default]
    #[serde(rename = "en", other)]
    English,
}
impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::Spanish];
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Spanish => "es",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Spanish => "Español",
        }
    }
    pub fn from_code(code: &str) -> Self {
        if code == "es" {
            Self::Spanish
        } else {
            Self::English
        }
    }
}
static CURRENT: AtomicU8 = AtomicU8::new(0);
pub fn language() -> Language {
    if CURRENT.load(Ordering::Relaxed) == 1 {
        Language::Spanish
    } else {
        Language::English
    }
}
pub fn set_language(value: Language) {
    CURRENT.store(
        if value == Language::Spanish { 1 } else { 0 },
        Ordering::Relaxed,
    );
}
fn catalog(language: Language) -> &'static BTreeMap<String, String> {
    static EN: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    static ES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    match language {
        Language::English => EN.get_or_init(|| {
            serde_json::from_str(include_str!("../locales/en.json")).expect("valid English catalog")
        }),
        Language::Spanish => ES.get_or_init(|| {
            serde_json::from_str(include_str!("../locales/es.json")).expect("valid Spanish catalog")
        }),
    }
}
/// Only pass application messages here; terminal output and user content are never translated.
pub fn t(message: &str) -> &str {
    t_in(language(), message)
}
pub fn t_in(language: Language, message: &str) -> &str {
    let en = catalog(Language::English);
    let key = if en.contains_key(message) {
        message
    } else {
        en.iter()
            .find(|(_, value)| value.as_str() == message)
            .map(|(key, _)| key.as_str())
            .unwrap_or(message)
    };
    catalog(language)
        .get(key)
        .or_else(|| en.get(key))
        .map(String::as_str)
        .unwrap_or(message)
}
/// Values have already been formatted by Rust. Substitution is one pass, preserving user data.
pub fn format(message: &str, values: &[(&str, String)]) -> String {
    format_in(language(), message, values)
}
pub fn format_in(language: Language, message: &str, values: &[(&str, String)]) -> String {
    let template = t_in(language, message);
    let mut output = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    let mut position = 0;
    while let Some(c) = chars.next() {
        if (c == '{' || c == '}') && chars.peek() == Some(&c) {
            chars.next();
            output.push(c);
        } else if c == '{' {
            let mut field = String::new();
            for next in chars.by_ref() {
                if next == '}' {
                    break;
                }
                field.push(next);
            }
            if field.is_empty() || field.starts_with(':') {
                field = format!("{position}{field}");
                position += 1;
            }
            if let Some((_, value)) = values.iter().find(|(key, _)| *key == field) {
                output.push_str(value);
            } else {
                output.push('{');
                output.push_str(&field);
                output.push('}');
            }
        } else {
            output.push(c);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogs_cover_identical_messages_and_preserve_placeholders() {
        let en = catalog(Language::English);
        let es = catalog(Language::Spanish);
        assert!(!en.is_empty());
        assert_eq!(en.keys().collect::<Vec<_>>(), es.keys().collect::<Vec<_>>());
        for key in en.keys() {
            let fields = |text: &str| {
                let mut values = Vec::new();
                let mut rest = text;
                while let Some(start) = rest.find('{') {
                    rest = &rest[start + 1..];
                    if let Some(end) = rest.find('}') {
                        values.push(rest[..end].to_owned());
                        rest = &rest[end + 1..];
                    } else {
                        break;
                    }
                }
                values.sort();
                values
            };
            assert_eq!(fields(key), fields(&en[key]), "{key}");
            assert_eq!(fields(key), fields(&es[key]), "{key}");
        }
    }
    #[test]
    fn english_defaults_and_future_codes_fall_back_without_losing_settings() {
        assert_eq!(Language::default(), Language::English);
        assert_eq!(
            serde_json::from_str::<Language>("\"future\"").unwrap(),
            Language::English
        );
        assert_eq!(serde_json::to_string(&Language::Spanish).unwrap(), "\"es\"");
        assert_eq!(t_in(Language::English, "Ajustes"), "Settings");
        assert_eq!(t_in(Language::Spanish, "Ajustes"), "Ajustes");
        assert_eq!(
            t_in(Language::Spanish, "serverMetrics"),
            "Métricas del servidor"
        );
        assert_eq!(t_in(Language::English, "user file.txt"), "user file.txt");
    }
    #[test]
    fn formatted_values_are_not_translated_or_interpreted_as_templates() {
        let path = "C:/carpeta/Archivos/{error}.txt";
        assert_eq!(
            format_in(
                Language::English,
                "Crear archivo: {path}",
                &[("path", path.into())]
            ),
            format!("Create file: {path}")
        );
        assert_eq!(
            format_in(
                Language::Spanish,
                "Crear archivo: {path}",
                &[("path", path.into())]
            ),
            format!("Crear archivo: {path}")
        );
        assert_eq!(
            format_in(
                Language::English,
                "PID {} · CPU {:.1}% del servidor",
                &[("0", "42".into()), ("1:.1", "12.3".into())]
            ),
            "PID 42 · CPU 12.3% of server"
        );
    }
}
