//! Host-only access to the real interactive PTY. Nothing is exported over RPC.
use crate::terminal::{Profile, Proxy};
use alacritty_terminal::{
    grid::Dimensions,
    term::{Term, TermMode},
    vte::{Parser, Perform},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
    time::{Duration, Instant},
};

const LIMIT: usize = 65536;
#[derive(Clone)]
pub struct Console {
    input: SyncSender<Vec<u8>>,
    term: Arc<Mutex<Term<Proxy>>>,
    pub stream: Arc<Mutex<Stream>>,
    pub alive: Arc<AtomicBool>,
    shell: Profile,
    submitted: Arc<Mutex<Vec<String>>>,
}
#[derive(Default)]
pub struct Stream {
    parser: Parser,
    capture: Capture,
    hidden: Vec<u8>,
    visible_started: bool,
}
#[derive(Default)]
struct Capture {
    nonce: String,
    started: bool,
    finished: Option<i32>,
    text: String,
    truncated: bool,
    interrupted: bool,
    line: String,
    display: Option<String>,
}
impl Perform for Capture {
    fn print(&mut self, c: char) {
        if self.nonce.is_empty() || self.finished.is_some() {
            return;
        }
        self.line.push(c);
        let start = format!("__XPEED_{}_START__", self.nonce);
        if self.line.ends_with(&start) {
            self.started = true;
            self.text.clear();
            self.line.clear();
            return;
        }
        let end = format!("__XPEED_{}_END_", self.nonce);
        if let Some(index) = self.line.find(&end)
            && let Some(code) = self.line[index + end.len()..]
                .strip_suffix("__")
                .and_then(|s| s.parse().ok())
            && self.started
        {
            let line = std::mem::take(&mut self.line);
            for c in line[..index].chars() {
                self.append(c);
            }
            self.finished = Some(code);
            return;
        }
        if self.line.len() > 1024 {
            let line = std::mem::take(&mut self.line);
            for c in line.chars() {
                self.append(c);
            }
        }
    }
    fn execute(&mut self, byte: u8) {
        if byte == b'\n' {
            let line = std::mem::take(&mut self.line);
            if line == format!("__XPEED_{}_START__", self.nonce) {
                self.started = true;
                self.text.clear();
            } else if let Some(code) = line
                .strip_prefix(&format!("__XPEED_{}_END_", self.nonce))
                .and_then(|s| s.strip_suffix("__"))
                .and_then(|s| s.parse().ok())
            {
                if self.started {
                    self.finished = Some(code);
                }
            } else {
                for c in line.chars() {
                    self.append(c);
                }
                self.append('\n');
            }
        } else if byte == b'\t' {
            self.print('\t');
        }
    }
    fn osc_dispatch(&mut self, params: &[&[u8]], _: bool) {
        if params.len() < 4
            || !matches!(params[0], b"0" | b"2")
            || params[1] != b"XPEED"
            || params[2] != self.nonce.as_bytes()
            || self.nonce.is_empty()
        {
            return;
        }
        if params[3] == b"start" {
            self.started = true;
        } else if params[3] == b"end" && self.started {
            self.finished = params
                .get(4)
                .and_then(|v| std::str::from_utf8(v).ok()?.parse().ok());
        }
    }
}
impl Capture {
    fn append(&mut self, c: char) {
        if !self.started || self.finished.is_some() {
            return;
        }
        if self.text.len() + c.len_utf8() <= LIMIT {
            self.text.push(c);
        } else {
            self.truncated = true;
        }
    }
}
impl Stream {
    pub fn advance(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.parser.advance(&mut self.capture, bytes);
        // Remove only the generated protocol marker, keeping real command output.
        // Prefix matching is streaming and does not delay an ordinary shell prompt.
        let prefix = format!("__XPEED_{}_", self.capture.nonce);
        if self.capture.nonce.is_empty() {
            return bytes.to_vec();
        }
        let mut visible = Vec::with_capacity(bytes.len());
        if let Some(command) = self.capture.display.take() {
            visible.extend_from_slice(command.as_bytes());
            visible.extend_from_slice(b"\r\n");
        }
        for &b in bytes {
            self.hidden.push(b);
            if !self.visible_started {
                if self.hidden.starts_with(prefix.as_bytes()) {
                    if self.hidden.ends_with(b"_START__") {
                        self.visible_started = true;
                        self.hidden.clear();
                    } else if self.hidden.len() > prefix.len() + 40 {
                        self.hidden.clear();
                    }
                } else if !prefix.as_bytes().starts_with(&self.hidden) {
                    self.hidden.clear();
                }
                continue;
            }
            if self.hidden.starts_with(prefix.as_bytes()) {
                if b == b'\n' {
                    self.hidden.clear();
                } else if self.hidden.len() > prefix.len() + 40 {
                    visible.append(&mut self.hidden);
                }
            } else if !prefix.as_bytes().starts_with(&self.hidden) {
                visible.append(&mut self.hidden);
            }
        }
        visible
    }
}
impl Console {
    pub fn new(input: SyncSender<Vec<u8>>, term: Arc<Mutex<Term<Proxy>>>, shell: Profile) -> Self {
        Self {
            input,
            term,
            shell,
            submitted: Arc::default(),
            stream: Arc::default(),
            alive: Arc::new(AtomicBool::new(true)),
        }
    }
    pub fn busy(&self) -> bool {
        self.stream
            .lock()
            .is_ok_and(|s| !s.capture.nonce.is_empty())
    }
    pub fn take_commands(&self) -> Vec<String> {
        self.submitted
            .lock()
            .map(|mut commands| std::mem::take(&mut *commands))
            .unwrap_or_default()
    }
    pub fn interrupt(&self) {
        if let Ok(mut s) = self.stream.lock() {
            s.capture.interrupted = true;
        }
        let _ = self.input.try_send(vec![3]);
    }
    pub fn snapshot(&self) -> Result<String, String> {
        if !self.alive.load(Ordering::Acquire) {
            return Err("Terminal is closed.".into());
        }
        let term = self.term.lock().map_err(|e| e.to_string())?;
        let mut text = String::new();
        for row in 0..term.screen_lines() {
            let line: String = (0..term.columns())
                .map(|col| {
                    term.grid()[alacritty_terminal::index::Point::new(
                        alacritty_terminal::index::Line(row as i32),
                        alacritty_terminal::index::Column(col),
                    )]
                    .c
                })
                .collect();
            text.push_str(line.trim_end());
            text.push('\n');
        }
        text.truncate(text.floor_char_boundary(24000));
        Ok(text)
    }
    pub fn directory(&self) -> Option<String> {
        crate::directory::terminal_directory(&*self.term.lock().ok()?)
    }
    pub fn execute(
        &self,
        command: &str,
        cwd: &str,
        timeout: Duration,
        cancel: &AtomicBool,
        mut progress: impl FnMut(&str, bool),
    ) -> Result<serde_json::Value, String> {
        if self.shell == Profile::Cmd {
            return Err("Visible execution in Command Prompt is not supported yet. Open a PowerShell session.".into());
        }
        if command
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
            || cwd.chars().any(char::is_control)
        {
            return Err("Control characters are not allowed in terminal commands or paths.".into());
        }
        let mut state = self.stream.lock().map_err(|e| e.to_string())?;
        if !state.capture.nonce.is_empty() {
            return Err("This terminal already has an agent command running.".into());
        }
        if !self.alive.load(Ordering::Acquire) {
            return Err("Terminal is closed.".into());
        }
        let term = self.term.lock().map_err(|e| e.to_string())?;
        let editor = crate::terminal_view::command_editor(&term, term.columns());
        if term.mode().contains(TermMode::ALT_SCREEN)
            || !editor.is_some_and(|e| e.text.iter().all(|c| c.is_whitespace()))
        {
            return Err("Terminal must be at an empty shell prompt. Finish the running command or clear the input first.".into());
        }
        // Reject a nested shell of another type rather than sending incompatible syntax.
        let row = term.grid().cursor.point.line;
        let line: String = (0..term.columns())
            .map(|col| {
                term.grid()[alacritty_terminal::index::Point::new(
                    row,
                    alacritty_terminal::index::Column(col),
                )]
                .c
            })
            .collect();
        let powershell = line.starts_with("PS ") || line.starts_with("[PS ");
        if powershell != (self.shell == Profile::PowerShell) {
            return Err("The current prompt does not match this session's shell. Use a dedicated session for nested shells.".into());
        }
        drop(term);
        let nonce = format!("{:016x}", rand::random::<u64>());
        let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
        let script = if powershell {
            let command = command
                .split('\n')
                .map(|line| format!("'{}'", line.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join("+[char]10+");
            let cwd = cwd.replace('\'', "''");
            format!(
                "[Console]::WriteLine([char]10+'__XPEED_'+'{nonce}'+'_START__'); $global:LASTEXITCODE=0; try {{ Set-Location -LiteralPath '{cwd}' -ErrorAction Stop; Invoke-Expression ({command}); if ($?) {{ $xpeed_{nonce}=0 }} else {{ $xpeed_{nonce}=1 }}; if ($LASTEXITCODE) {{ $xpeed_{nonce}=$LASTEXITCODE }} }} catch {{ Write-Output $_; $xpeed_{nonce}=1 }}; [Console]::WriteLine([char]10+'__XPEED_'+'{nonce}'+'_END_'+$xpeed_{nonce}+'__'); Remove-Variable xpeed_{nonce}\r"
            )
        } else {
            let cwd = if cwd == "~" {
                "\"$HOME\"".into()
            } else if let Some(path) = cwd.strip_prefix("~/") {
                format!("\"$HOME\"/{}", quote(path))
            } else {
                quote(cwd)
            };
            format!(
                "printf '\\n__XPEED_%s_START__\\n' '{nonce}'; if cd -- {}; then eval {}; xpeed_{nonce}=$?; else xpeed_{nonce}=$?; fi; printf '\\n__XPEED_%s_END_%s__\\n' '{nonce}' \"$xpeed_{nonce}\"; unset xpeed_{nonce}\r",
                cwd,
                quote(command)
            )
        };
        state.capture = Capture {
            nonce,
            display: Some(command.to_owned()),
            ..Default::default()
        };
        state.visible_started = false;
        state.hidden.clear();
        self.input.try_send(script.into_bytes()).map_err(|e| {
            state.capture = Capture::default();
            e.to_string()
        })?;
        if let Ok(mut submitted) = self.submitted.lock() {
            submitted.push(command.to_owned());
        }
        drop(state);
        let started = Instant::now();
        let mut interrupted = None;
        loop {
            if (cancel.load(Ordering::Acquire) || started.elapsed() >= timeout)
                && interrupted.is_none()
            {
                self.interrupt();
                interrupted = Some(Instant::now());
            }
            let mut stream = self.stream.lock().map_err(|e| e.to_string())?;
            if stream.capture.interrupted && interrupted.is_none() {
                interrupted = Some(Instant::now());
            }
            progress(&stream.capture.text, stream.capture.truncated);
            let closed = !self.alive.load(Ordering::Acquire);
            let exhausted = interrupted.is_some_and(|i| i.elapsed() > Duration::from_secs(2));
            if stream.capture.finished.is_some() || closed || exhausted {
                let capture = std::mem::take(&mut stream.capture);
                let state = if cancel.load(Ordering::Acquire)
                    || capture.interrupted && started.elapsed() < timeout
                {
                    "cancelled"
                } else if started.elapsed() >= timeout {
                    "timedOut"
                } else if capture.finished.is_none() {
                    "failed"
                } else {
                    "completed"
                };
                return Ok(
                    serde_json::json!({"state":state,"stdout":capture.text,"stderr":"","exitCode":capture.finished,"outputTruncated":capture.truncated,"executionMode":"visibleTerminal","streamsCombined":true,"remoteMayBeRunning":capture.finished.is_none(),"error":if closed {Some("Terminal closed during execution.")} else if capture.finished.is_none() {Some("Completion was not confirmed. Check the visible terminal before another command.")} else {None}}),
                );
            }
            drop(stream);
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Started by scripts/test-session-tools.py with isolated localhost SSH servers"]
    fn visible_ssh_console_preserves_environment_and_returns_actual_output() {
        let config: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var("XPEED_TOOL_TEST_CONFIG").unwrap()).unwrap(),
        )
        .unwrap();
        let item = &config[0];
        let mut session = crate::terminal::Session::spawn_ssh_fixture(
            serde_json::from_value(item["connection"].clone()).unwrap(),
            std::path::Path::new(item["knownHosts"].as_str().unwrap()),
        );
        let started = Instant::now();
        loop {
            session.poll();
            let term = session.term.lock().unwrap();
            if crate::terminal_view::command_editor(&term, term.columns())
                .is_some_and(|e| e.text.is_empty())
            {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "SSH fixture did not reach a shell prompt"
            );
            drop(term);
            std::thread::sleep(Duration::from_millis(20));
        }
        for command in [
            "xpeed_fixture_var=41; cat marker.txt",
            "printf '%s\\n' \"$((xpeed_fixture_var + 1))\"; ls",
        ] {
            let console = session.console.clone();
            let command = command.to_owned();
            let worker = std::thread::spawn(move || {
                console.execute(
                    &command,
                    "/test",
                    Duration::from_secs(8),
                    &AtomicBool::new(false),
                    |_, _| {},
                )
            });
            while !worker.is_finished() {
                session.poll();
                std::thread::sleep(Duration::from_millis(20));
            }
            let result = worker.join().unwrap().unwrap();
            assert_eq!(result["state"], "completed", "{result}");
            assert_eq!(result["exitCode"], 0, "{result}");
            let text = result["stdout"].as_str().unwrap();
            assert!(
                text.contains("SERVER-A") || text.contains("42") && text.contains("marker.txt"),
                "{result}"
            );
            std::thread::sleep(Duration::from_millis(150));
            session.poll();
        }
        assert!(session.console.snapshot().unwrap().contains("marker.txt"));
        assert_eq!(session.commands.len(), 2);
        assert_eq!(session.commands[0], "xpeed_fixture_var=41; cat marker.txt");
        assert!(!session.commands.iter().any(|c| c.contains("__XPEED_")));
    }
    #[cfg(windows)]
    #[test]
    fn visible_powershell_executes_once_preserves_variables_and_reports_output() {
        let cwd = std::env::temp_dir();
        let mut session = crate::terminal::Session::spawn(
            999,
            Profile::PowerShell,
            cwd.clone(),
            eframe::egui::Context::default(),
        )
        .unwrap();
        let ready = Instant::now();
        loop {
            session.poll();
            let term = session.term.lock().unwrap();
            if crate::terminal_view::command_editor(&term, term.columns())
                .is_some_and(|e| e.text.is_empty())
            {
                break;
            }
            assert!(
                ready.elapsed() < Duration::from_secs(15),
                "PowerShell did not reach a prompt"
            );
            drop(term);
            std::thread::sleep(Duration::from_millis(20));
        }
        let run = |session: &mut crate::terminal::Session, command: &str| {
            let console = session.console.clone();
            let path = cwd.to_string_lossy().into_owned();
            let command = command.to_owned();
            let worker = std::thread::spawn(move || {
                console.execute(
                    &command,
                    &path,
                    Duration::from_secs(8),
                    &AtomicBool::new(false),
                    |_, _| {},
                )
            });
            while !worker.is_finished() {
                session.poll();
                std::thread::sleep(Duration::from_millis(20));
            }
            worker.join().unwrap().unwrap()
        };
        let result = run(
            &mut session,
            "$xpeedTest = 41; Write-Output 'xpeed-visible-first'",
        );
        assert_eq!(result["state"], "completed", "{result}");
        assert_eq!(result["exitCode"], 0, "{result}");
        assert!(
            result["stdout"]
                .as_str()
                .unwrap()
                .contains("xpeed-visible-first"),
            "{result}"
        );
        std::thread::sleep(Duration::from_millis(120));
        session.poll();
        let result = run(&mut session, "Write-Output ($xpeedTest + 1)");
        assert!(
            result["stdout"].as_str().unwrap().contains("42"),
            "{result}"
        );
        assert!(session.console.snapshot().unwrap().contains("42"));
        session.poll();
        assert_eq!(
            session.commands,
            vec![
                "$xpeedTest = 41; Write-Output 'xpeed-visible-first'",
                "Write-Output ($xpeedTest + 1)",
            ]
        );
        session.commands.clear();
        session.poll();
        assert!(session.commands.is_empty());
        std::thread::sleep(Duration::from_millis(150));
        session.poll();
        session.record_commands = false;
        let result = run(
            &mut session,
            "Write-Output 'línea ñ'; Write-Output 'segunda'\nthrow 'fixture-error'",
        );
        assert_eq!(result["exitCode"], 1, "{result}");
        assert!(
            result["stdout"].as_str().unwrap().contains("fixture-error"),
            "{result}"
        );
        std::thread::sleep(Duration::from_millis(150));
        session.poll();
        assert!(session.commands.is_empty());
        session.record_commands = true;
        session.send_user(b"echo draft".to_vec());
        std::thread::sleep(Duration::from_millis(200));
        session.poll();
        assert!(
            session
                .console
                .execute(
                    "echo forbidden",
                    &cwd.to_string_lossy(),
                    Duration::from_secs(1),
                    &AtomicBool::new(false),
                    |_, _| {}
                )
                .is_err()
        );
        session.send_user(vec![3]);
        std::thread::sleep(Duration::from_millis(200));
        session.poll();
        let console = session.console.clone();
        let path = cwd.to_string_lossy().into_owned();
        let worker = std::thread::spawn(move || {
            console.execute(
                "Start-Sleep -Seconds 10",
                &path,
                Duration::from_millis(300),
                &AtomicBool::new(false),
                |_, _| {},
            )
        });
        while !worker.is_finished() {
            session.poll();
            std::thread::sleep(Duration::from_millis(20));
        }
        let result = worker.join().unwrap().unwrap();
        assert_eq!(result["state"], "timedOut", "{result}");
        assert!(
            result["exitCode"].is_null(),
            "Interrupted completion must not invent an exit code: {result}"
        );
    }
    #[test]
    fn markers_and_utf8_survive_split_reads_and_ignore_echo_and_other_titles() {
        let mut stream = Stream {
            capture: Capture {
                nonce: "test".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let data = b"echo command\r\n__XPEED_test_START__\r\n\x1b[31mhola\x1b[0m\r\n\xc3\xb1\x1b]0;XPEED;other;end;0\x07\n__XPEED_test_END_7__\r\nprompt";
        for b in data {
            stream.advance(&[*b]);
        }
        assert_eq!(stream.capture.text.trim_start_matches('\n'), "hola\nñ\n");
        assert_eq!(stream.capture.finished, Some(7));
    }
}
