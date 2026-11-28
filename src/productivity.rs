//! User-facing configuration transfer and asynchronous remote text editing.
use crate::{
    connections::{Connection, ConnectionStore},
    settings::Settings,
    sftp::RemoteExplorer,
};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};
use xpeed_i18n::t;
#[derive(Serialize, Deserialize)]
struct Backup {
    version: u32,
    settings: Settings,
    connections: ConnectionStore,
}
fn read_backup(path: &std::path::Path) -> Result<Backup, String> {
    if path.metadata().map_err(|e| e.to_string())?.len() > 2 * 1024 * 1024 {
        return Err(t("Backup exceeds 2 MiB.").into());
    }
    let mut backup: Backup =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if backup.version != 1 {
        return Err(t("Unsupported backup version.").into());
    }
    if backup.connections.saved.len() > 1000 || backup.connections.recent.len() > 1000 {
        return Err(t("Too many connections in the backup.").into());
    }
    for c in backup
        .connections
        .saved
        .iter()
        .chain(&backup.connections.recent)
    {
        if c.name.len() > 256 {
            return Err(t("Connection name is too long.").into());
        }
        if let crate::connections::Target::Ssh(ssh) = &c.target {
            // Private key paths may refer to another machine; never import credentials.
            let mut config = ssh.clone();
            config.identity.clear();
            config.validate()?;
        }
    }
    if !backup.settings.font_size.is_finite() {
        return Err(t("Invalid font size.").into());
    }
    backup.settings.font_size = backup.settings.font_size.clamp(11., 24.);
    backup.settings.scrollback_lines = backup.settings.scrollback_lines.clamp(100, 50000);
    backup.settings.accent = backup.settings.accent.min(2);
    backup.settings.dark_theme = backup.settings.dark_theme.min(2);
    backup.settings.light_theme = backup.settings.light_theme.min(2);
    Ok(backup)
}
#[derive(Default)]
pub struct State {
    pub backup_open: bool,
    pub preferences_changed: bool,
    backup_path: String,
    backup_message: String,
    import_preview: Option<Backup>,
    editors: BTreeMap<(u64, String), Editor>,
    download: Option<(u64, String, String)>,
}
struct Editor {
    config: crate::connections::SshConfig,
    password: Option<crate::credentials::Secret>,
    original: String,
    text: String,
    receiver: Option<Receiver<Result<EditorResult, String>>>,
    comparison: Option<String>,
    conflict: Option<String>,
    loaded: bool,
    confirm_close: bool,
    error: Option<String>,
    cancel: Arc<AtomicBool>,
}

enum EditorResult {
    Content { text: String, saved: bool },
    Conflict(String),
}

impl Editor {
    fn save(&mut self, ctx: &egui::Context, path: &str, expected: String) {
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.error = None;
        let config = self.config.clone();
        let password = self.password.clone();
        let path = path.to_owned();
        let text = self.text.clone();
        let cancel = self.cancel.clone();
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let result =
                crate::sftp::replace_text(&config, password, cancel, &path, &expected, &text, None)
                    .map(|outcome| match outcome {
                        crate::sftp::ReplaceOutcome::Written => {
                            EditorResult::Content { text, saved: true }
                        }
                        crate::sftp::ReplaceOutcome::Conflict(server) => {
                            EditorResult::Conflict(server)
                        }
                    });
            let _ = tx.send(result);
            wake.request_repaint();
        });
    }
}

fn line_changes(before: &str, after: &str) -> String {
    use std::fmt::Write as _;
    let before: Vec<_> = before.split_inclusive('\n').collect();
    let after: Vec<_> = after.split_inclusive('\n').collect();
    let prefix = before
        .iter()
        .zip(&after)
        .take_while(|(old, new)| old == new)
        .count();
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take_while(|(old, new)| old == new)
        .count();
    let old = &before[prefix..before.len() - suffix];
    let new = &after[prefix..after.len() - suffix];
    let width = new.len() + 1;
    let mut output = String::new();
    let mut row = |marker, line: usize, text: &str| {
        let _ = writeln!(
            output,
            "{marker} {:>5}  {}",
            prefix + line + 1,
            text.strip_suffix('\n').unwrap_or(text)
        );
    };
    if (old.len() + 1).saturating_mul(width) > 1_000_000 {
        for (index, text) in old.iter().enumerate() {
            row('-', index, text);
        }
        for (index, text) in new.iter().enumerate() {
            row('+', index, text);
        }
        return output;
    }
    let mut shared = vec![0u32; (old.len() + 1) * width];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            shared[i * width + j] = if old[i] == new[j] {
                1 + shared[(i + 1) * width + j + 1]
            } else {
                shared[(i + 1) * width + j].max(shared[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            i += 1;
            j += 1;
        } else if j == new.len()
            || (i < old.len() && shared[(i + 1) * width + j] >= shared[i * width + j + 1])
        {
            row('-', i, old[i]);
            i += 1;
        } else {
            row('+', j, new[j]);
            j += 1;
        }
    }
    output
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

fn editor_language(path: &str) -> Option<(&'static str, &'static str)> {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    match extension {
        Some("rs") => Some(("rust", "Rust")),
        Some("py") => Some(("python", "Python")),
        Some("toml") => Some(("toml", "TOML")),
        Some("c" | "h" | "cc" | "cpp" | "cxx" | "hpp") => Some(("cpp", "C/C++")),
        // The built-in lightweight highlighter recognizes comments, strings and
        // common keywords through its Python/C++ tokenizers without adding syntect.
        Some("sh" | "bash" | "zsh" | "bashrc" | "profile") => Some(("python", "Shell")),
        Some("ps1" | "yaml" | "yml") => Some(("python", "Script")),
        Some("js" | "jsx" | "ts" | "tsx" | "json" | "php" | "java" | "css") => {
            Some(("cpp", "Code"))
        }
        _ => None,
    }
}

impl State {
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        settings: &mut Settings,
        connections: &mut ConnectionStore,
        remotes: &mut BTreeMap<u64, RemoteExplorer>,
        sources: &BTreeMap<u64, Connection>,
        ephemeral: bool,
    ) {
        if self.backup_open {
            if self.backup_path.is_empty() {
                self.backup_path = dirs::document_dir()
                    .unwrap_or_default()
                    .join("xpeed-backup.json")
                    .to_string_lossy()
                    .into_owned();
            }
            let mut open = true;
            egui::Window::new(t("Export / import configuration")).open(&mut open).default_width(480.).show(ctx, |ui| {
                ui.label(t("Includes connections, groups and settings. Passwords and Codex sign-in are excluded."));
                ui.add(egui::TextEdit::singleline(&mut self.backup_path).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    if ui.add_enabled(!ephemeral, egui::Button::new(t("Export"))).clicked() {
                        let result = (|| {
                            let backup = Backup { version: 1, settings: settings.clone(), connections: connections.clone() };
                            let bytes = serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?;
                            // Exclusive creation avoids silently overwriting a backup.
                            use std::io::Write;
                            let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&self.backup_path).map_err(|e| e.to_string())?;
                            file.write_all(&bytes).map_err(|e| e.to_string())
                        })();
                        self.backup_message = result.map_or_else(|e| e, |_| t("Backup exported").into());
                    }
                    if ui.add_enabled(!ephemeral, egui::Button::new(t("Read backup"))).clicked() {
                        self.import_preview = None;
                        match read_backup(std::path::Path::new(&self.backup_path)) {
                            Ok(backup) => { self.import_preview = Some(backup); self.backup_message.clear(); },
                            Err(e) => self.backup_message = e,
                        }
                    }
                });
                if let Some(backup) = &self.import_preview {
                    ui.label(format!("{}: {}", t("Conexiones"), backup.connections.saved.len()));
                    ui.weak(t("Import merges profiles and replaces settings. Open terminals keep running."));
                    if ui.button(t("Import backup")).clicked() {
                        let backup = self.import_preview.take().unwrap();
                        let mut merged = connections.clone();
                        for c in backup.connections.saved {
                            if !merged.saved.iter().any(|old| old.name == c.name && old.description() == c.description()) { merged.saved.push(c); }
                        }
                        for c in backup.connections.recent {
                            if !merged.recent.iter().any(|old| old.name == c.name && old.description() == c.description()) { merged.recent.push(c); }
                        }
                        merged.recent.truncate(20);
                        merged.groups.extend(backup.connections.groups);
                        let old_settings = settings.clone();
                        let result = backup.settings.save().and_then(|()| merged.save());
                        if result.is_ok() { *settings = backup.settings; *connections = merged; self.preferences_changed = true; }
                        else { let _ = old_settings.save(); }
                        self.backup_message = result.map_or_else(|e| e, |_| t("Backup imported").into());
                    }
                }
                ui.add(egui::Label::new(&self.backup_message).wrap());
            });
            self.backup_open = open;
        }
        for (&id, remote) in remotes.iter_mut() {
            if let Some(path) = remote.edit_requested.take() {
                let key = (id, path.clone());
                self.editors.entry(key).or_insert_with(|| {
                    let config = remote.connection();
                    let password = remote
                        .monitoring_access()
                        .or_else(|| crate::credentials::load(&config));
                    let cancel = Arc::new(AtomicBool::new(false));
                    let (tx, rx) = mpsc::channel();
                    let cfg = config.clone();
                    let secret = password.clone();
                    let stop = cancel.clone();
                    let wake = ctx.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(
                            crate::sftp::tool_read(&cfg, secret, stop, &path, None)
                                .map(|text| EditorResult::Content { text, saved: false }),
                        );
                        wake.request_repaint();
                    });
                    Editor {
                        config,
                        password,
                        original: String::new(),
                        text: String::new(),
                        receiver: Some(rx),
                        comparison: None,
                        conflict: None,
                        loaded: false,
                        confirm_close: false,
                        error: None,
                        cancel,
                    }
                });
            }
            if let Some(path) = remote.download_requested.take() {
                let filename = path.rsplit('/').next().unwrap_or("download");
                self.download = Some((
                    id,
                    path.clone(),
                    dirs::download_dir()
                        .unwrap_or_default()
                        .join(filename)
                        .to_string_lossy()
                        .into_owned(),
                ));
            }
        }
        let mut closed = Vec::new();
        for ((id, path), editor) in &mut self.editors {
            if let Some(rx) = &editor.receiver {
                let result = match rx.try_recv() {
                    Ok(r) => Some(r),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err(t("The operation ended without a result.").into()))
                    }
                    _ => None,
                };
                if let Some(result) = result {
                    match result {
                        Ok(EditorResult::Content { text, saved }) => {
                            editor.original = text.clone();
                            editor.text = text;
                            editor.loaded = true;
                            editor.error = None;
                            editor.comparison = None;
                            editor.conflict = None;
                            if saved && let Some(remote) = remotes.get_mut(id) {
                                remote.refresh();
                            }
                        }
                        Ok(EditorResult::Conflict(server)) => {
                            editor.comparison = Some(line_changes(&server, &editor.text));
                            editor.conflict = Some(server);
                        }
                        Err(error) => editor.error = Some(error),
                    }
                    editor.receiver = None;
                }
            }
            let mut open = true;
            let name = sources.get(id).map_or("SSH", |c| c.name.as_str());
            egui::Window::new(format!("{name}: {path}"))
                .id(egui::Id::new(("remote_editor", id, path)))
                .open(&mut open)
                .default_size([650., 440.])
                .show(ctx, |ui| {
                    let dirty = editor.text != editor.original;
                    let language = editor_language(path);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                editor.loaded
                                    && editor.receiver.is_none()
                                    && editor.conflict.is_none()
                                    && dirty
                                    && editor.text.len() <= 65536,
                                egui::Button::new(t("Guardar")),
                            )
                            .clicked()
                        {
                            editor.save(ctx, path, editor.original.clone());
                        }
                        if ui.add_enabled(dirty || editor.conflict.is_some(), egui::Button::new(t("View changes"))).clicked() {
                            editor.comparison = Some(line_changes(editor.conflict.as_deref().unwrap_or(&editor.original), &editor.text));
                        }
                        let status = t(if dirty {
                            "Unsaved changes"
                        } else {
                            "UTF-8 - up to 64 KiB"
                        });
                        ui.weak(match language {
                            Some((_, label)) => format!("{label} · {status}"),
                            None => status.into(),
                        });
                        if editor.receiver.is_some() {
                            ui.spinner();
                        }
                    });
                    if let Some(error) = &editor.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if editor.conflict.is_some() {
                        ui.add(egui::Label::new(t("The remote file changed while you were editing. Review your draft against the current server version.")).wrap());
                    }
                    if editor.comparison.is_some() {
                        let mut back = false;
                        let mut replace = false;
                        ui.horizontal_wrapped(|ui| {
                            back = ui.button(t("Back to editor")).clicked();
                            if editor.conflict.is_some() {
                                replace = ui.add_enabled(
                                    editor.receiver.is_none() && editor.text.len() <= 65536,
                                    egui::Button::new(t("Replace reviewed server version")),
                                ).clicked();
                            }
                        });
                        if replace && let Some(server) = editor.conflict.clone() {
                            editor.save(ctx, path, server);
                        }
                        if back {
                            editor.comparison = None;
                            return;
                        }
                        let comparison = editor.comparison.as_deref().unwrap_or_default();
                        if comparison.is_empty() {
                            ui.weak(t("No differences."));
                        } else {
                            let lines: Vec<_> = comparison.lines().collect();
                            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                            egui::ScrollArea::both().id_salt("remote_comparison")
                                .scroll_source(egui::scroll_area::ScrollSource {
                                    drag: false, ..egui::scroll_area::ScrollSource::ALL
                                })
                                .show_rows(ui, row_height, lines.len(), |ui, range| {
                                    for line in &lines[range] {
                                        let (color, hint) = if line.starts_with('-') {
                                            (ui.visuals().error_fg_color, "Removed from server version")
                                        } else {
                                            (ui.visuals().hyperlink_color, "Added in your draft")
                                        };
                                        ui.add(egui::Label::new(egui::RichText::new(*line).monospace().color(color))
                                            .selectable(true).extend()).on_hover_text(t(hint));
                                    }
                                });
                        }
                        return;
                    }
                    egui::ScrollArea::both().show(ui, |ui| {
                        let mut layouter =
                            |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                                let mut job = if let Some((language, _)) = language {
                                    let theme =
                                        egui_extras::syntax_highlighting::CodeTheme::from_style(
                                            ui.style(),
                                        );
                                    egui_extras::syntax_highlighting::highlight(
                                        ui.ctx(),
                                        ui.style(),
                                        &theme,
                                        text.as_str(),
                                        language,
                                    )
                                } else {
                                    egui::text::LayoutJob::simple(
                                        text.as_str().into(),
                                        egui::TextStyle::Monospace.resolve(ui.style()),
                                        ui.visuals().text_color(),
                                        f32::INFINITY,
                                    )
                                };
                                job.wrap.max_width = wrap_width;
                                ui.fonts_mut(|fonts| fonts.layout_job(job))
                            };
                        ui.add_enabled(
                            editor.loaded && editor.receiver.is_none(),
                            egui::TextEdit::multiline(&mut editor.text)
                                .font(egui::TextStyle::Monospace)
                                .code_editor()
                                .layouter(&mut layouter)
                                .desired_width(f32::INFINITY)
                                .desired_rows(20),
                        );
                    });
                });
            if !open {
                if editor.text == editor.original {
                    closed.push((*id, path.clone()));
                } else {
                    editor.confirm_close = true;
                }
            }
            if editor.confirm_close {
                egui::Modal::new(egui::Id::new(("discard_editor", id, path))).show(ctx, |ui| {
                    ui.heading(t("Unsaved changes"));
                    ui.label(path);
                    ui.horizontal(|ui| {
                        if ui.button(t("Discard changes and close")).clicked() {
                            closed.push((*id, path.clone()));
                        }
                        if ui.button(t("Cancelar")).clicked() {
                            editor.confirm_close = false;
                        }
                    });
                });
            }
        }
        for key in closed {
            self.editors.remove(&key);
        }
        if let Some((id, remote_path, local)) = &mut self.download {
            let mut open = true;
            let mut start = false;
            egui::Window::new(t("Download file..."))
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(remote_path.as_str());
                    ui.add(egui::TextEdit::singleline(local).desired_width(420.));
                    if ui.button(t("Download")).clicked() {
                        start = true;
                    }
                });
            if start {
                if let Some(remote) = remotes.get_mut(id) {
                    remote.transfer(
                        vec![(PathBuf::from(local.as_str()), remote_path.clone())],
                        true,
                    );
                }
                self.download = None;
            } else if !open {
                self.download = None;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparison_lists_changed_lines_with_their_original_line_numbers() {
        let before = "unchanged\nold value\nkeep this\nsecond old value\nend\n";
        let after = "unchanged\nnew value\nkeep this\nsecond new value\nend\n";
        assert_eq!(
            line_changes(before, after),
            "-     2  old value\n+     2  new value\n-     4  second old value\n+     4  second new value\n"
        );
        assert!(line_changes(before, before).is_empty());
    }

    #[test]
    fn remote_conflict_preserves_the_draft_and_opens_the_server_comparison() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        let path = (7, "/edit.txt".to_owned());
        let (tx, rx) = mpsc::channel();
        state.editors.insert(
            path.clone(),
            Editor {
                config: Default::default(),
                password: None,
                original: "original\n".into(),
                text: "draft\n".into(),
                receiver: Some(rx),
                comparison: None,
                conflict: None,
                loaded: true,
                confirm_close: false,
                error: None,
                cancel: Arc::new(AtomicBool::new(false)),
            },
        );
        tx.send(Ok(EditorResult::Conflict("changed on server\n".into())))
            .unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            state.show(
                ctx,
                &mut Settings::default(),
                &mut ConnectionStore::default(),
                &mut BTreeMap::new(),
                &BTreeMap::new(),
                true,
            )
        });
        let editor = &state.editors[&path];
        assert_eq!(editor.text, "draft\n");
        assert_eq!(editor.original, "original\n");
        assert_eq!(editor.conflict.as_deref(), Some("changed on server\n"));
        let comparison = editor.comparison.as_deref().unwrap();
        assert!(comparison.contains("-     1  changed on server"));
        assert!(comparison.contains("+     1  draft"));
    }
    #[test]
    fn editor_selects_lightweight_syntax_from_remote_path() {
        assert_eq!(
            editor_language("/srv/install.sh"),
            Some(("python", "Shell"))
        );
        assert_eq!(editor_language("/srv/main.rs"), Some(("rust", "Rust")));
        assert_eq!(editor_language("/srv/config.toml"), Some(("toml", "TOML")));
        assert_eq!(editor_language("/srv/image.png"), None);
    }

    #[test]
    fn unsaved_close_confirmation_survives_repaints_until_discard() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        state.editors.insert(
            (7, "/edit.txt".into()),
            Editor {
                config: Default::default(),
                password: None,
                original: "old".into(),
                text: "new".into(),
                receiver: None,
                comparison: None,
                conflict: None,
                loaded: true,
                confirm_close: true,
                error: None,
                cancel: Arc::new(AtomicBool::new(false)),
            },
        );
        let mut settings = Settings::default();
        let mut connections = ConnectionStore::default();
        let mut remotes = BTreeMap::new();
        let sources = BTreeMap::new();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                ..Default::default()
            },
            |ctx| {
                state.show(
                    ctx,
                    &mut settings,
                    &mut connections,
                    &mut remotes,
                    &sources,
                    true,
                )
            },
        );
        let mut target = egui::Pos2::ZERO;
        for _ in 0..3 {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    state.show(
                        ctx,
                        &mut settings,
                        &mut connections,
                        &mut remotes,
                        &sources,
                        true,
                    )
                },
            );
            target = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(text)
                        if text.galley.job.text == t("Discard changes and close") =>
                    {
                        Some(text.pos + egui::vec2(10., 5.))
                    }
                    _ => None,
                })
                .expect("confirmation must remain visible");
            assert_eq!(state.editors.len(), 1);
        }
        for pressed in [true, false] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(target),
                        egui::Event::PointerButton {
                            pos: target,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ctx| {
                    state.show(
                        ctx,
                        &mut settings,
                        &mut connections,
                        &mut remotes,
                        &sources,
                        true,
                    )
                },
            );
        }
        assert!(state.editors.is_empty());
    }
    #[test]
    fn backup_rejects_invalid_hosts_and_never_contains_authentication() {
        let path = std::env::temp_dir().join(format!(
            "xpeed-invalid-backup-{}.json",
            rand::random::<u64>()
        ));
        let mut backup = Backup {
            version: 1,
            settings: Settings::default(),
            connections: ConnectionStore::default(),
        };
        backup.connections.saved.push(Connection {
            name: "bad".into(),
            target: crate::connections::Target::Ssh(crate::connections::SshConfig {
                host: "-oProxyCommand=bad".into(),
                ..Default::default()
            }),
        });
        let bytes = serde_json::to_vec(&backup).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert!(read_backup(&path).is_err());
        let json = String::from_utf8(bytes).unwrap();
        assert!(!json.contains("password") && !json.contains("token"));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn backup_roundtrip_and_validation() {
        let path =
            std::env::temp_dir().join(format!("xpeed-backup-{}.json", rand::random::<u64>()));
        let backup = Backup {
            version: 1,
            settings: Settings::default(),
            connections: ConnectionStore::default(),
        };
        std::fs::write(&path, serde_json::to_vec(&backup).unwrap()).unwrap();
        assert_eq!(read_backup(&path).unwrap().settings.scrollback_lines, 5000);
        let mut bad = serde_json::to_value(&backup).unwrap();
        bad["version"] = 99.into();
        std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(read_backup(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
