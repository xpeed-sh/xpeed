use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Serialize, Deserialize)]
pub struct Command {
    pub time: u64,
    pub text: String,
}
#[derive(Default, Serialize, Deserialize)]
pub struct History {
    pub commands: BTreeMap<String, Vec<Command>>,
}
impl History {
    fn path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|p| p.join("xpeed/commands.json"))
    }
    pub fn load() -> Self {
        Self::path()
            .map(|p| Self::load_from(&p))
            .unwrap_or_default()
    }
    pub fn load_from(path: &std::path::Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn push(&mut self, key: String, text: String) {
        let records = self.commands.entry(key).or_default();
        records.push(Command {
            time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            text,
        });
        if records.len() > 500 {
            records.remove(0);
        }
    }
    pub fn save(&self) -> Result<(), String> {
        let path =
            Self::path().ok_or(xpeed_i18n::t("No se encontró la carpeta de configuración"))?;
        self.save_to(&path)
    }
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        std::fs::create_dir_all(path.parent().ok_or(xpeed_i18n::t("Ruta inválida"))?)
            .map_err(|e| e.to_string())?;
        let temp = path.with_extension("tmp");
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(temp, path).map_err(|e| e.to_string())
    }
}

/// Only recognize shell prompts; interactive password/app input is not a command.
pub fn prompt_end(line: &str) -> Option<usize> {
    if line.starts_with("PS ")
        || line.starts_with("[PS ")
        || (line.as_bytes().get(1) == Some(&b':') && line.contains('\\'))
    {
        return line.rfind('>').map(|i| i + 1);
    }
    for marker in ["$ ", "# ", "% "] {
        if let Some(i) = line.find(marker) {
            let prefix = &line[..i];
            if prefix.is_empty()
                || prefix.contains('@')
                || prefix.starts_with('~')
                || prefix.starts_with('/')
                || prefix.starts_with('[')
            {
                return Some(i + marker.len());
            }
        }
    }
    None
}
pub struct Editor {
    pub text: Vec<char>,
    pub cursor: usize,
}
pub fn record_input(editor: &mut Option<Editor>, bytes: &[u8], records: &mut Vec<String>) {
    let Some(line) = editor.as_mut() else {
        return;
    };
    match bytes {
        b"\x1b[D" | b"\x1bOD" => line.cursor = line.cursor.saturating_sub(1),
        b"\x1b[C" | b"\x1bOC" => line.cursor = (line.cursor + 1).min(line.text.len()),
        b"\x1b[H" | b"\x1bOH" | b"\x01" => line.cursor = 0,
        b"\x1b[F" | b"\x1bOF" | b"\x05" => line.cursor = line.text.len(),
        b"\x1b[3~" => {
            if line.cursor < line.text.len() {
                line.text.remove(line.cursor);
            }
        }
        b"\x7f" | b"\x08" => {
            if line.cursor > 0 {
                line.cursor -= 1;
                line.text.remove(line.cursor);
            }
        }
        b"\r" => {
            let command: String = line.text.iter().collect::<String>().trim().to_string();
            if !command.is_empty() {
                records.push(command);
            }
            *editor = None;
        }
        _ if bytes.starts_with(b"\x1b[200~") => {
            if let Ok(text) = std::str::from_utf8(bytes) {
                for c in text
                    .trim_start_matches("\x1b[200~")
                    .trim_end_matches("\x1b[201~")
                    .chars()
                {
                    if c == '\n' || c == '\r' {
                        *editor = None;
                        return;
                    }
                    line.text.insert(line.cursor, c);
                    line.cursor += 1;
                }
            }
        }
        _ if bytes.first() == Some(&27) || bytes.iter().any(|b| *b < 32) => *editor = None,
        _ => {
            if let Ok(text) = std::str::from_utf8(bytes) {
                for c in text.chars() {
                    line.text.insert(line.cursor, c);
                    line.cursor += 1;
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_saves_keep_commands_from_earlier_app_runs() {
        let root = std::env::temp_dir().join(format!("xpeed-history-{}", rand::random::<u64>()));
        let path = root.join("commands.json");
        let mut history = History::default();
        history.push("cloud :: old-run".into(), "ls".into());
        history.save_to(&path).unwrap();
        let mut reopened = History::load_from(&path);
        reopened.push("cloud :: new-run".into(), "pwd".into());
        reopened.push("other :: run".into(), "whoami".into());
        reopened.save_to(&path).unwrap();
        let restored = History::load_from(&path);
        assert_eq!(restored.commands["cloud :: old-run"][0].text, "ls");
        assert_eq!(restored.commands["cloud :: new-run"][0].text, "pwd");
        assert_eq!(restored.commands["other :: run"][0].text, "whoami");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn recognizes_prompts_and_excludes_authentication_input() {
        for line in [
            "PS C:\\Users\\me> echo hola",
            "C:\\Windows>dir",
            "root@host:~# ls",
            "$ pwd",
        ] {
            assert!(prompt_end(line).is_some(), "{line}");
        }
        for line in [
            "root@host's password: secret",
            "Password: secret",
            "Enter passphrase for key: secret",
            "Proceed? yes",
            "some output",
        ] {
            assert!(prompt_end(line).is_none(), "{line}");
        }
    }
}
