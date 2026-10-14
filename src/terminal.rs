use alacritty_terminal::{
    event::{Event, EventListener, WindowSize},
    grid::{Dimensions, Scroll},
    term::{Config, Term, TermMode},
    vte::ansi::Processor,
};
use eframe::egui;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum Profile {
    PowerShell,
    Cmd,
    Wsl,
    System,
}

impl Profile {
    pub fn available() -> Vec<Self> {
        if cfg!(windows) {
            vec![Self::PowerShell, Self::Cmd, Self::Wsl]
        } else {
            vec![Self::System]
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::PowerShell => "PowerShell",
            Self::Cmd => "Command Prompt",
            Self::Wsl => "WSL",
            Self::System => "Shell",
        }
    }
    pub fn default_shell() -> Self {
        if cfg!(windows) {
            Self::PowerShell
        } else {
            Self::System
        }
    }
    pub fn command(self) -> CommandBuilder {
        match self {
            Self::PowerShell => {
                let mut c = CommandBuilder::new("powershell.exe");
                c.args(["-NoLogo", "-NoExit"]);
                c
            }
            Self::Cmd => CommandBuilder::new("cmd.exe"),
            Self::Wsl => CommandBuilder::new("wsl.exe"),
            Self::System => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
                let mut c = CommandBuilder::new(shell);
                c.arg("-l");
                c
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct Size {
    pub cols: usize,
    pub rows: usize,
}
impl Dimensions for Size {
    fn columns(&self) -> usize {
        self.cols
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn total_lines(&self) -> usize {
        self.rows
    }
}

#[derive(Clone)]
pub struct Proxy {
    tx: Sender<Event>,
}
impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        if matches!(&event, Event::Title(title) if title.starts_with("XPEED;")) {
            return;
        }
        let _ = self.tx.send(event);
    }
}

pub struct Session {
    pub id: u64,
    pub shell: Profile,
    pub title: String,
    pub cwd: PathBuf,
    pub term: Arc<Mutex<Term<Proxy>>>,
    pub console: crate::terminal_agent::Console,
    master: Box<dyn MasterPty + Send>,
    child: Option<Box<dyn Child + Send + Sync>>,
    input: Option<SyncSender<Vec<u8>>>,
    events: Receiver<Event>,
    failures: Receiver<String>,
    pub error: Option<String>,
    pub exited: bool,
    pub unexpected_disconnect: bool,
    pub reconnecting: bool,
    pub size: Size,
    pub started: Instant,
    pub scroll_remainder: f32,
    pub remote: Option<crate::connections::SshConfig>,
    pub palette: crate::palette::Palette,
    pub input_id: Option<egui::Id>,
    pub commands: Vec<String>,
    pub chat_selection: Option<String>,
    search_position: Option<(String, i32)>,
    pub record_commands: bool,
    pub history_key: String,
    pub submitted_password: Option<crate::credentials::Secret>,
    pub saved_password: Option<crate::credentials::Secret>,
    pub auth_complete: bool,
    directory_report: Arc<Mutex<Option<String>>>,
    password_draft: crate::credentials::Secret,
}

impl Session {
    #[cfg(test)]
    pub fn spawn_isolated_test_shell(id: u64, cwd: PathBuf, ctx: egui::Context) -> Self {
        let mut command = if cfg!(windows) {
            let mut command = CommandBuilder::new("powershell.exe");
            command.args(["-NoLogo", "-NoProfile", "-NoExit", "-Command",
                "Import-Module PSReadLine; Set-PSReadLineOption -HistorySaveStyle SaveNothing; [Microsoft.PowerShell.PSConsoleReadLine]::ClearHistory(); function global:prompt { 'PS XPEED_TEST> ' }"]);
            command
        } else {
            let mut command = CommandBuilder::new("/bin/bash");
            command.args(["--noprofile", "--norc", "-i"]);
            command.env("PS1", "XPEED_TEST:workspace runner$ ");
            command.env("HISTFILE", "/dev/null");
            command.env("BASH_SILENCE_DEPRECATION_WARNING", "1");
            command
        };
        command.env("TERM", "xterm-256color");
        Self::spawn_command(id, Profile::default_shell(), cwd, ctx, command, None).unwrap()
    }

    pub fn spawn(
        id: u64,
        shell: Profile,
        cwd: PathBuf,
        ctx: egui::Context,
    ) -> Result<Self, String> {
        Self::spawn_command(id, shell, cwd, ctx, shell.command(), None)
    }

    pub fn spawn_ssh(
        id: u64,
        config: crate::connections::SshConfig,
        cwd: PathBuf,
        ctx: egui::Context,
    ) -> Result<Self, String> {
        let command = config.command()?;
        Self::spawn_command(id, Profile::System, cwd, ctx, command, Some(config))
    }
    #[cfg(test)]
    pub fn spawn_ssh_fixture(
        config: crate::connections::SshConfig,
        hosts: &std::path::Path,
    ) -> Self {
        Self::spawn_command(
            999,
            Profile::System,
            std::env::temp_dir(),
            egui::Context::default(),
            config.command_with_hosts(Some(hosts)).unwrap(),
            Some(config),
        )
        .unwrap()
    }

    fn spawn_command(
        id: u64,
        shell: Profile,
        cwd: PathBuf,
        ctx: egui::Context,
        mut command: CommandBuilder,
        remote: Option<crate::connections::SshConfig>,
    ) -> Result<Self, String> {
        let size = Size {
            cols: 100,
            rows: 28,
        };
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: size.rows as u16,
                cols: size.cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| {
                xpeed_i18n::format(
                    "No se pudo crear la terminal: {e}",
                    &[("e", format!("{}", e))],
                )
            })?;
        command.cwd(&cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("TERM_PROGRAM", "xpeed");
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let child = pair.slave.spawn_command(command).map_err(|e| {
            xpeed_i18n::format(
                "No se pudo abrir {}: {e}",
                &[("0", shell.name().to_string()), ("e", format!("{}", e))],
            )
        })?;
        drop(pair.slave);
        let (event_tx, events) = mpsc::channel();
        let term = Arc::new(Mutex::new(Term::new(
            Config::default(),
            &size,
            Proxy { tx: event_tx },
        )));
        // Bounded input queue keeps a stalled child from growing memory indefinitely.
        let (input, inputs) = mpsc::sync_channel::<Vec<u8>>(256);
        let (fail_tx, failures) = mpsc::channel();
        let writer_ctx = ctx.clone();
        let writer_fail = fail_tx.clone();
        thread::Builder::new()
            .name(format!("xpeed-input-{id}"))
            .spawn(move || {
                while let Ok(bytes) = inputs.recv() {
                    if let Err(e) = writer.write_all(&bytes).and_then(|_| writer.flush()) {
                        let _ = writer_fail.send(xpeed_i18n::format(
                            "No se pudo enviar la entrada: {e}",
                            &[("e", format!("{}", e))],
                        ));
                        writer_ctx.request_repaint();
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let reader_term = term.clone();
        let console = crate::terminal_agent::Console::new(input.clone(), term.clone(), shell);
        let reader_console = console.clone();
        let directory_report = Arc::new(Mutex::new(None));
        let reader_directory = directory_report.clone();
        thread::Builder::new()
            .name(format!("xpeed-output-{id}"))
            .spawn(move || {
                let mut parser: Processor = Processor::new();
                let mut directories = crate::directory::Reports::default();
                let mut buffer = [0u8; 16384];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            let visible = reader_console
                                .stream
                                .lock()
                                .map(|mut stream| stream.advance(&buffer[..n]))
                                .unwrap_or_else(|_| buffer[..n].to_vec());
                            if let Some(path) = directories.advance(&buffer[..n])
                                && let Ok(mut report) = reader_directory.lock()
                            {
                                *report = Some(path);
                            }
                            if let Ok(mut term) = reader_term.lock() {
                                parser.advance(&mut *term, &visible);
                            } else {
                                break;
                            }
                            ctx.request_repaint();
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) => {
                            let _ = fail_tx.send(xpeed_i18n::format(
                                "La lectura de la terminal terminó: {e}",
                                &[("e", format!("{}", e))],
                            ));
                            break;
                        }
                    }
                }
                reader_console
                    .alive
                    .store(false, std::sync::atomic::Ordering::Release);
                ctx.request_repaint();
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            id,
            shell,
            title: remote
                .as_ref()
                .map(|c| c.label())
                .unwrap_or_else(|| shell.name().into()),
            cwd,
            term,
            console,
            master: pair.master,
            child: Some(child),
            input: Some(input),
            events,
            failures,
            error: None,
            exited: false,
            unexpected_disconnect: false,
            reconnecting: false,
            size,
            started: Instant::now(),
            scroll_remainder: 0.0,
            remote,
            commands: Vec::new(),
            chat_selection: None,
            search_position: None,
            record_commands: true,
            history_key: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .to_string(),
            palette: crate::palette::Palette::default(),
            input_id: None,
            submitted_password: None,
            saved_password: None,
            auth_complete: false,
            directory_report,
            password_draft: crate::credentials::Secret::new(String::new()),
        })
    }

    pub(crate) fn at_password_prompt(&self) -> bool {
        if self.remote.is_none() || self.auth_complete || self.exited {
            return false;
        }
        self.term.lock().ok().is_some_and(|term| {
            let row = term.grid().cursor.point.line;
            let text: String = (0..self.size.cols)
                .map(|col| {
                    term.grid()[alacritty_terminal::index::Point::new(
                        row,
                        alacritty_terminal::index::Column(col),
                    )]
                    .c
                })
                .collect();
            crate::credentials::password_prompt(&text)
        })
    }
    pub fn send_user(&mut self, bytes: Vec<u8>) {
        if self.console.busy() {
            if bytes == [3] {
                self.console.interrupt();
            }
            return;
        }
        if self.at_password_prompt() {
            match bytes.as_slice() {
                b"\r" => {
                    if !self.password_draft.is_empty() {
                        self.submitted_password = Some(std::mem::replace(
                            &mut self.password_draft,
                            crate::credentials::Secret::new(String::new()),
                        ));
                        self.saved_password = None;
                    }
                }
                b"\x7f" | b"\x08" => {
                    self.password_draft.pop();
                }
                b"\x15" | b"\x03" => self.password_draft.clear(),
                _ if bytes.iter().all(|b| *b >= 32 && *b != 127) => {
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        self.password_draft.push_str(text);
                    }
                }
                _ => {
                    self.password_draft.clear();
                }
            }
        } else {
            self.password_draft.clear();
        }
        self.send(bytes);
    }
    pub fn remote_directory(&self) -> Option<String> {
        if self.remote.is_none() || self.exited {
            return None;
        }
        let term = self.term.lock().ok()?;
        if term.mode().contains(TermMode::ALT_SCREEN) {
            return None;
        }
        if let Some(path) = self.directory_report.lock().ok()?.as_ref() {
            return Some(path.clone());
        }
        crate::directory::terminal_directory(&term)
    }

    pub fn has_shell_prompt(&self) -> bool {
        if self.remote.is_none() || self.exited {
            return false;
        }
        self.term.lock().ok().is_some_and(|term| {
            let row = term.grid().cursor.point.line;
            let text: String = (0..self.size.cols)
                .map(|col| {
                    term.grid()[alacritty_terminal::index::Point::new(
                        row,
                        alacritty_terminal::index::Column(col),
                    )]
                    .c
                })
                .collect();
            crate::history::prompt_end(&text).is_some()
        })
    }

    pub fn send(&mut self, bytes: impl Into<Vec<u8>>) {
        if self.exited {
            return;
        }
        if let Some(input) = &self.input
            && let Err(error) = input.try_send(bytes.into())
        {
            self.error = Some(match error {
                mpsc::TrySendError::Full(_) => {
                    xpeed_i18n::t("La consola no está recibiendo entrada; inténtalo de nuevo")
                        .into()
                }
                mpsc::TrySendError::Disconnected(_) => {
                    xpeed_i18n::t("La entrada de esta sesión está cerrada").into()
                }
            });
        }
    }

    pub fn paste(&mut self, text: &str) {
        if self.console.busy() {
            return;
        }
        let bracketed = self
            .term
            .lock()
            .map(|t| t.mode().contains(TermMode::BRACKETED_PASTE))
            .unwrap_or(false);
        let clean = text.replace('\u{1b}', "");
        if bracketed {
            self.send(format!("\x1b[200~{clean}\x1b[201~").into_bytes());
        } else {
            self.send(clean.replace("\r\n", "\n").replace('\n', "\r").into_bytes());
        }
    }

    pub fn take_recorded_commands(&mut self) -> Vec<String> {
        let queued = self.console.take_commands();
        if self.record_commands {
            self.commands.extend(queued);
        }
        std::mem::take(&mut self.commands)
    }

    pub fn poll(&mut self) {
        let commands = self.console.take_commands();
        if self.record_commands {
            self.commands.extend(commands);
        }
        if self.at_password_prompt()
            && let Some(password) = self.saved_password.take()
        {
            self.send(format!("{}\r", password.as_str()).into_bytes());
            self.submitted_password = Some(password);
        }
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::PtyWrite(text) => self.send(text.into_bytes()),
                Event::Title(title) => self.title = title,
                Event::ResetTitle => self.title = self.shell.name().into(),
                Event::TextAreaSizeRequest(format) => self.send(
                    format(WindowSize {
                        num_lines: self.size.rows as u16,
                        num_cols: self.size.cols as u16,
                        cell_width: 0,
                        cell_height: 0,
                    })
                    .into_bytes(),
                ),
                Event::ColorRequest(index, format) => {
                    use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
                    let color = if index == NamedColor::Background as usize {
                        self.palette.terminal
                    } else if index == NamedColor::Foreground as usize {
                        self.palette.text
                    } else if index < 256 {
                        self.palette.color(Color::Indexed(index as u8))
                    } else {
                        self.palette.accent
                    };
                    self.send(
                        format(Rgb {
                            r: color.r(),
                            g: color.g(),
                            b: color.b(),
                        })
                        .into_bytes(),
                    );
                }
                _ => {}
            }
        }
        while let Ok(error) = self.failures.try_recv() {
            self.error = Some(error);
        }
        if !self.exited
            && let Some(child) = &mut self.child
        {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if self.remote.is_some() && !status.success() {
                        self.error = Some(xpeed_i18n::format(
                            "OpenSSH terminó con código {}. Revisa el último mensaje de la terminal para conocer el motivo.",
                            &[("0", format!("{}", status.exit_code()))],
                        ));
                    }
                    self.unexpected_disconnect =
                        self.remote.is_some() && status.exit_code() == 255 && self.auth_complete;
                    self.exited = true;
                    self.input.take();
                }
                Err(e) => self.error = Some(e.to_string()),
                _ => {}
            }
        }
    }

    pub fn resize(&mut self, size: Size) {
        if size.cols == self.size.cols && size.rows == self.size.rows {
            return;
        }
        if self.exited {
            if let Ok(mut term) = self.term.lock() {
                term.resize(size);
            }
            self.size = size;
            return;
        }
        if let Err(e) = self.master.resize(PtySize {
            rows: size.rows as u16,
            cols: size.cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            self.error = Some(xpeed_i18n::format(
                "No se pudo redimensionar: {e}",
                &[("e", format!("{}", e))],
            ));
            return;
        }
        if let Ok(mut term) = self.term.lock() {
            term.resize(size);
        }
        self.size = size;
    }

    pub fn find_text(&mut self, query: &str, previous: bool) -> (usize, usize) {
        use alacritty_terminal::index::{Column, Line, Point};
        let Ok(mut term) = self.term.lock() else {
            return (0, 0);
        };
        if query.is_empty() {
            return (0, 0);
        }
        let history = term.grid().history_size() as i32;
        let offset = term.grid().display_offset() as i32;
        let mut lines = Vec::new();
        let needle = query.to_lowercase();
        for line in -history..self.size.rows as i32 {
            let text: String = (0..self.size.cols)
                .map(|col| term.grid()[Point::new(Line(line), Column(col))].c)
                .collect();
            if text.to_lowercase().contains(&needle) {
                lines.push(line);
            }
        }
        let anchor = self
            .search_position
            .as_ref()
            .filter(|(old, _)| old == query)
            .map_or(-offset, |(_, line)| *line);
        let Some(&line) = (if previous {
            lines
                .iter()
                .rev()
                .find(|&&line| line < anchor)
                .or_else(|| lines.last())
        } else {
            lines
                .iter()
                .find(|&&line| line > anchor)
                .or_else(|| lines.first())
        }) else {
            return (0, 0);
        };
        self.search_position = Some((query.into(), line));
        let target = (-line).max(0);
        term.scroll_display(Scroll::Delta(target - offset));
        (
            lines.iter().position(|&l| l == line).unwrap_or(0) + 1,
            lines.len(),
        )
    }
    pub fn scroll(&mut self, lines: i32) {
        if let Ok(mut term) = self.term.lock() {
            term.scroll_display(Scroll::Delta(lines));
        }
    }

    pub fn bottom(&mut self) {
        if let Ok(mut term) = self.term.lock() {
            term.scroll_display(Scroll::Bottom);
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.console
            .alive
            .store(false, std::sync::atomic::Ordering::Release);
        self.input.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            // Reap away from the UI thread; closing a tab never waits for a process.
            let _ = thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}
pub fn ssh_smoke_test(
    config: &crate::connections::SshConfig,
    password: crate::credentials::Secret,
    hosts: &std::path::Path,
) -> Result<(), String> {
    let command = config.command_with_hosts(Some(hosts))?;
    let ctx = egui::Context::default();
    let mut session = Session::spawn_command(
        1,
        Profile::System,
        std::env::current_dir().map_err(|e| e.to_string())?,
        ctx.clone(),
        command,
        Some(config.clone()),
    )?;
    let start = Instant::now();
    while !session.at_password_prompt() && start.elapsed() < Duration::from_secs(8) {
        session.poll();
        std::thread::sleep(Duration::from_millis(20));
    }
    if !session.at_password_prompt() {
        return Err("No se mostró la pregunta de contraseña SSH".into());
    }
    session.send_user(b"x".to_vec());
    session.send_user(vec![127]);
    session.send_user(password.as_bytes().to_vec());
    session.send_user(b"\r".to_vec());
    if session.submitted_password.as_ref() != Some(&password) || !session.commands.is_empty() {
        return Err("La contraseña se capturó de forma incorrecta".into());
    }
    while !session.has_shell_prompt() && start.elapsed() < Duration::from_secs(12) {
        session.poll();
        std::thread::sleep(Duration::from_millis(20));
    }
    if !session.has_shell_prompt() {
        return Err("La sesión SSH no quedó autenticada".into());
    }
    for (command, expected) in [
        ("cd /carpeta ñ\r", "/carpeta ñ"),
        ("cd ..\r", "/"),
        ("cd\r", "~"),
    ] {
        session.send(command.as_bytes().to_vec());
        let deadline = Instant::now() + Duration::from_secs(5);
        while session.remote_directory().as_deref() != Some(expected) && Instant::now() < deadline {
            session.poll();
            thread::sleep(Duration::from_millis(20));
        }
        if session.remote_directory().as_deref() != Some(expected) {
            return Err(format!(
                "No se detectó el directorio remoto {expected}: {:?}",
                session.remote_directory()
            ));
        }
    }
    drop(session);
    let command = config.command_with_hosts(Some(hosts))?;
    let mut session = Session::spawn_command(
        2,
        Profile::System,
        std::env::current_dir().map_err(|e| e.to_string())?,
        ctx,
        command,
        Some(config.clone()),
    )?;
    session.saved_password = Some(password);
    let start = Instant::now();
    while !session.has_shell_prompt() && start.elapsed() < Duration::from_secs(12) {
        session.poll();
        std::thread::sleep(Duration::from_millis(20));
    }
    if !session.has_shell_prompt() {
        return Err("No se pudo reutilizar el acceso SSH".into());
    }
    Ok(())
}

pub fn smoke_test() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut session = Session::spawn(
        1,
        Profile::default_shell(),
        std::env::current_dir().map_err(|e| e.to_string())?,
        ctx,
    )?;
    let deadline = Instant::now() + Duration::from_secs(20);
    // Allow shell initialization, then exercise a real PTY round trip.
    thread::sleep(Duration::from_millis(1200));
    session.send(b"echo XPEED_PTY_OK\r".to_vec());
    while Instant::now() < deadline {
        session.poll();
        let output = {
            let term = session.term.lock().map_err(|e| e.to_string())?;
            term.grid()
                .display_iter()
                .map(|c| c.cell.c)
                .collect::<String>()
        };
        if output.matches("XPEED_PTY_OK").count() >= 2 {
            session.resize(Size { cols: 80, rows: 24 });
            println!("PTY OK: interactive shell, ANSI parser, output and resize");
            session.send(b"exit\r".to_vec());
            while Instant::now() < deadline {
                session.poll();
                if session.exited {
                    println!("PROCESS OK: shell exit detected");
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(30));
            }
            return Err("El proceso no terminó a tiempo".into());
        }
        thread::sleep(Duration::from_millis(30));
    }
    Err(format!(
        "La terminal no respondió a tiempo: {:?}",
        session.error
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn reconnect_only_classifies_authenticated_transport_failure() {
        for (code, authenticated, expected) in [
            (255, true, true),
            (255, false, false),
            (0, true, false),
            (1, true, false),
        ] {
            let mut command = if cfg!(windows) {
                CommandBuilder::new("cmd.exe")
            } else {
                CommandBuilder::new("/bin/sh")
            };
            command.args(if cfg!(windows) {
                vec!["/c".into(), format!("exit {code}")]
            } else {
                vec!["-c".into(), format!("exit {code}")]
            });
            let mut session = Session::spawn_command(
                1000 + code,
                Profile::System,
                std::env::temp_dir(),
                egui::Context::default(),
                command,
                Some(crate::connections::SshConfig::default()),
            )
            .unwrap();
            session.auth_complete = authenticated;
            let deadline = Instant::now() + Duration::from_secs(5);
            while !session.exited && Instant::now() < deadline {
                session.poll();
                thread::sleep(Duration::from_millis(10));
            }
            assert!(session.exited);
            assert_eq!(session.unexpected_disconnect, expected);
        }
    }
    #[test]
    fn search_cycles_through_history_and_visible_matches() {
        let mut session = Session::spawn(
            1500,
            Profile::default_shell(),
            std::env::temp_dir(),
            egui::Context::default(),
        )
        .unwrap();
        let marker = format!("search-{}", rand::random::<u64>());
        {
            let mut term = session.term.lock().unwrap();
            let mut parser: Processor = Processor::new();
            for n in 0..60 {
                parser.advance(&mut *term, format!("{marker}-{n}\r\n").as_bytes());
            }
        }
        let first = session.find_text(&marker, true);
        let second = session.find_text(&marker, true);
        assert_eq!(first.1, 60);
        assert_eq!(second.1, 60);
        assert_ne!(first.0, second.0);
        assert_eq!(session.find_text("nonexistent-marker", false), (0, 0));
    }
    #[test]
    fn ssh_session_reaches_the_configured_local_endpoint_and_detects_disconnect() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                if let Ok((mut socket, _)) = listener.accept() {
                    socket.set_nonblocking(false).unwrap();
                    socket
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    socket.write_all(b"SSH-2.0-XpeedTest\r\n").unwrap();
                    let mut banner = [0; 256];
                    let count = socket.read(&mut banner).unwrap();
                    return String::from_utf8_lossy(&banner[..count]).starts_with("SSH-2.0-");
                }
                thread::sleep(Duration::from_millis(20));
            }
            false
        });
        let mut session = Session::spawn_ssh(
            1,
            crate::connections::SshConfig {
                host: "127.0.0.1".into(),
                port,
                user: "xpeed-test".into(),
                identity: String::new(),
            },
            std::env::current_dir().unwrap(),
            egui::Context::default(),
        )
        .unwrap();
        // ConPTY/OpenSSH can request terminal capabilities before connecting.
        // Keep responding to terminal events, as the GUI does each frame.
        while !server.is_finished() {
            session.poll();
            thread::sleep(Duration::from_millis(20));
        }
        let reached = server.join().unwrap();
        session.poll();
        let transcript: String = session
            .term
            .lock()
            .unwrap()
            .grid()
            .display_iter()
            .map(|c| c.cell.c)
            .collect();
        assert!(
            reached,
            "OpenSSH must reach the chosen host and port: {transcript}; {:?}",
            session.error
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !session.exited && Instant::now() < deadline {
            session.poll();
            thread::sleep(Duration::from_millis(20));
        }
        assert!(session.exited);
        assert!(session.remote.is_some());
        assert!(
            session
                .term
                .lock()
                .unwrap()
                .grid()
                .display_iter()
                .any(|c| !c.cell.c.is_whitespace())
        );
    }
}
