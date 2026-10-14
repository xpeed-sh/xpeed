//! Read directory reports without running commands or changing remote configuration.
#[derive(Default)]
pub struct Reports {
    state: u8,
    bytes: Vec<u8>,
}
impl Reports {
    pub fn advance(&mut self, input: &[u8]) -> Option<String> {
        let mut latest = None;
        for &b in input {
            match self.state {
                0 if b == 27 => self.state = 1,
                1 if b == b']' => {
                    self.bytes.clear();
                    self.state = 2;
                }
                1 => self.state = if b == 27 { 1 } else { 0 },
                2 if b == 7 => {
                    latest = self.report().or(latest);
                    self.state = 0;
                }
                2 if b == 27 => self.state = 3,
                2 => {
                    if self.bytes.len() < 8192 {
                        self.bytes.push(b);
                    } else {
                        self.bytes.clear();
                        self.state = 0;
                    }
                }
                3 if b == b'\\' => {
                    latest = self.report().or(latest);
                    self.state = 0;
                }
                3 => {
                    self.bytes.clear();
                    self.state = if b == 27 { 1 } else { 0 };
                }
                _ => {}
            }
        }
        latest
    }
    fn report(&self) -> Option<String> {
        let value = std::str::from_utf8(&self.bytes)
            .ok()?
            .strip_prefix("7;file://")?;
        let start = value.find('/')?;
        let encoded = &value.as_bytes()[start..];
        let mut decoded = Vec::new();
        let mut i = 0;
        while i < encoded.len() {
            if encoded[i] == b'%' {
                let h = (*encoded.get(i + 1)? as char).to_digit(16)?;
                let l = (*encoded.get(i + 2)? as char).to_digit(16)?;
                decoded.push((h * 16 + l) as u8);
                i += 3;
            } else {
                decoded.push(encoded[i]);
                i += 1;
            }
        }
        let path = String::from_utf8(decoded).ok()?;
        (!path.chars().any(char::is_control)).then_some(path)
    }
}
pub fn terminal_directory<T: alacritty_terminal::event::EventListener>(
    term: &alacritty_terminal::term::Term<T>,
) -> Option<String> {
    use alacritty_terminal::{
        grid::Dimensions,
        index::{Column, Line, Point},
        term::cell::Flags,
    };
    let cursor = term.grid().cursor.point;
    let cols = term.columns();
    let mut first = cursor.line.0;
    for _ in 0..7 {
        if first <= 0
            || !term.grid()[Point::new(Line(first - 1), Column(cols - 1))]
                .flags
                .contains(Flags::WRAPLINE)
        {
            break;
        }
        first -= 1;
    }
    // Join only wrapped rows of the current input; skip wide glyph padding cells.
    let mut text = String::new();
    for row in first..=cursor.line.0 {
        let end = if row == cursor.line.0 {
            cursor.column.0
        } else {
            cols
        };
        for col in 0..end {
            let cell = &term.grid()[Point::new(Line(row), Column(col))];
            if !cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                text.push(cell.c);
            }
        }
    }
    prompt_directory(&text)
}

/// Recognize explicit paths in common Unix prompts. Never predict a successful `cd`.
pub fn prompt_directory(line: &str) -> Option<String> {
    let end = crate::history::prompt_end(line)?;
    if let Some(path) = line[..end]
        .strip_prefix("PS ")
        .and_then(|s| s.strip_suffix('>'))
    {
        return Some(path.trim().to_owned());
    }
    let prompt = line[..end]
        .trim_end()
        .trim_end_matches(['$', '#', '%'])
        .trim_end();
    let path = if let Some((identity, path)) = prompt.split_once(':') {
        if !identity.contains('@') {
            return None;
        }
        path
    } else {
        prompt
    };
    ((path.starts_with('/') || path == "~" || path.starts_with("~/"))
        && !path.chars().any(char::is_control))
    .then(|| path.to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streamed_reports_decode_paths_and_ignore_other_osc() {
        let mut parser = Reports::default();
        assert_eq!(parser.advance(b"output\x1b]7;file://host/home/"), None);
        assert_eq!(parser.advance(b"carpeta%20%C3%B1\x1b"), None);
        assert_eq!(parser.advance(b"\\"), Some("/home/carpeta ñ".into()));
        assert_eq!(parser.advance(b"\x1b]0;title\x07"), None);
        assert_eq!(parser.advance(b"\x1b]7;file://host/\x07"), Some("/".into()));
        assert_eq!(parser.advance(b"\x1b]7;file://host/%00\x07"), None);
        assert_eq!(parser.advance(b"\x1b]7;file://host/%zz\x07"), None);
    }
    #[test]
    fn terminal_paths_survive_wrapping_and_wide_characters() {
        struct Listener;
        impl alacritty_terminal::event::EventListener for Listener {
            fn send_event(&self, _: alacritty_terminal::event::Event) {}
        }
        let mut term = alacritty_terminal::term::Term::new(
            alacritty_terminal::term::Config::default(),
            &crate::terminal::Size { cols: 24, rows: 8 },
            Listener,
        );
        let mut parser: alacritty_terminal::vte::ansi::Processor =
            alacritty_terminal::vte::ansi::Processor::new();
        parser.advance(
            &mut term,
            "user@host:/carpeta/中文/archivo:uno$ cd ..".as_bytes(),
        );
        assert_eq!(
            terminal_directory(&term).as_deref(),
            Some("/carpeta/中文/archivo:uno")
        );
        parser.advance(&mut term, b"\r\nPassword: ");
        assert_eq!(terminal_directory(&term), None);
    }
    #[test]
    fn prompts_report_actual_paths_not_typed_commands() {
        for (line, path) in [
            ("root@host:~# cd /tmp", "~"),
            ("root@host:/tmp# ", "/tmp"),
            ("user@host:~/carpeta ñ$ ", "~/carpeta ñ"),
            ("/var/log % ", "/var/log"),
        ] {
            assert_eq!(prompt_directory(line).as_deref(), Some(path));
        }
        for line in [
            "Password: ",
            "error: no such directory",
            "[user@host folder]$ ",
            "log@host: garbage# ",
        ] {
            assert_eq!(prompt_directory(line), None);
        }
    }
}
