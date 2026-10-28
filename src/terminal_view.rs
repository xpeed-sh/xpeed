use crate::{
    input::encode_key,
    palette::Palette,
    terminal::{Session, Size},
};
use alacritty_terminal::{
    index::{Column, Line, Point, Side},
    selection::{Selection, SelectionType},
    term::{TermMode, cell::Flags},
};
use eframe::egui::{self, Align2, Event, FontId, Pos2, Rect, Sense, Stroke, Vec2};

pub fn terminal_view(
    ui: &mut egui::Ui,
    session: &mut Session,
    font_size: f32,
    palette: Palette,
    focused: bool,
    keyboard: bool,
    search: &str,
) -> bool {
    let accent = palette.accent;
    let available = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(available, Sense::click_and_drag());
    if session
        .input_id
        .is_some_and(|id| id != response.id && ui.ctx().memory(|m| m.focused() == Some(id)))
    {
        response.request_focus();
    }
    session.input_id = Some(response.id);
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if response.has_focus()
        && ui.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|position| !response.rect.contains(position))
        })
    {
        response.surrender_focus();
    }
    if focused && keyboard && ui.ctx().memory(|m| m.focused().is_none()) {
        response.request_focus();
    }
    ui.memory_mut(|memory| {
        if focused && keyboard && memory.has_focus(response.id) {
            // Cancel navigation computed before the first focused frame's filter was installed.
            memory.move_focus(egui::FocusDirection::None);
        }
        memory.set_focus_lock_filter(
            response.id,
            egui::EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            },
        );
    });
    let rect = rect.shrink2(Vec2::new(18.0, 14.0));
    let font = FontId::monospace(font_size);
    let cell_width = ui.fonts_mut(|fonts| fonts.glyph_width(&font, 'M'));
    let cell_height = (font_size * 1.45).ceil();
    let cols = ((rect.width() / cell_width).floor() as usize).clamp(2, 500);
    let rows = ((rect.height() / cell_height).floor() as usize).clamp(1, 200);
    // Dock sizing passes can temporarily offer almost no space. Sending those
    // dimensions to the PTY makes the shell print banners in two-column lines.
    if !ui.is_sizing_pass()
        && rect.width() >= cell_width * 12.0
        && rect.height() >= cell_height * 2.0
    {
        session.resize(Size { cols, rows });
    }
    let cols = session.size.cols;
    let rows = session.size.rows;
    let painter = ui.painter().with_clip_rect(rect);
    let mut copy = None;
    let mut outgoing = Vec::new();
    let mut scroll = 0;
    let mut editor = None;
    {
        let Ok(mut term) = session.term.lock() else {
            return false;
        };
        let offset = term.grid().display_offset();
        let point_at = |pos: Pos2| {
            Point::new(
                Line(
                    ((pos.y - rect.top()) / cell_height)
                        .floor()
                        .clamp(0.0, rows as f32 - 1.0) as i32
                        - offset as i32,
                ),
                Column(
                    ((pos.x - rect.left()) / cell_width)
                        .floor()
                        .clamp(0.0, cols as f32 - 1.0) as usize,
                ),
            )
        };
        if (response.drag_started() || response.double_clicked())
            && let Some(pos) = response.interact_pointer_pos()
        {
            term.selection = Some(Selection::new(
                if response.double_clicked() {
                    SelectionType::Semantic
                } else {
                    SelectionType::Simple
                },
                point_at(pos),
                Side::Left,
            ));
        }
        if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
            && let Some(selection) = &mut term.selection
        {
            selection.update(point_at(pos), Side::Right);
            let edge_scroll = selection_drag_scroll(pos.y, rect, cell_height);
            if edge_scroll != 0 {
                scroll += edge_scroll;
                ui.ctx().request_repaint();
            }
        }
        if response.clicked() && !response.double_clicked() {
            term.selection = None;
        }
        if response.hovered() {
            session.scroll_remainder += ui.input(|i| i.smooth_scroll_delta.y);
            scroll = (session.scroll_remainder / cell_height).trunc() as i32;
            session.scroll_remainder -= scroll as f32 * cell_height;
        }
        let mode = *term.mode();
        let accepts_keyboard = focused && keyboard && response.has_focus();
        if accepts_keyboard && session.record_commands && !mode.contains(TermMode::ALT_SCREEN) {
            editor = command_editor(&term, cols);
        }
        if accepts_keyboard {
            let events = ui.input(|i| i.events.clone());
            let modifiers = ui.input(|i| i.modifiers);
            for event in events {
                match event {
                    Event::Copy
                        if modifiers.shift
                            || modifiers.mac_cmd
                            || modifiers.ctrl && modifiers.alt =>
                    {
                        copy = term.selection_to_string()
                    }
                    Event::Copy => outgoing.push(vec![3]),
                    Event::Cut => outgoing.push(vec![24]),
                    Event::Paste(text) => {
                        let text = text.replace('\u{1b}', "");
                        outgoing.push(if mode.contains(TermMode::BRACKETED_PASTE) {
                            format!("\x1b[200~{text}\x1b[201~").into_bytes()
                        } else {
                            text.replace("\r\n", "\n").replace('\n', "\r").into_bytes()
                        });
                    }
                    Event::Text(text)
                        if !modifiers.mac_cmd && (!modifiers.ctrl || modifiers.alt) =>
                    {
                        let mut bytes = text.into_bytes();
                        if modifiers.alt && !modifiers.ctrl {
                            bytes.insert(0, 27);
                        }
                        outgoing.push(bytes);
                    }
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        if modifiers.ctrl && modifiers.shift && key == egui::Key::C {
                            copy = term.selection_to_string();
                        } else if modifiers.shift && key == egui::Key::PageUp {
                            scroll = rows as i32 - 1;
                        } else if modifiers.shift && key == egui::Key::PageDown {
                            scroll = -(rows as i32 - 1);
                        } else if let Some(bytes) =
                            encode_key(key, modifiers, mode.contains(TermMode::APP_CURSOR))
                        {
                            outgoing.push(bytes);
                        }
                    }
                    _ => {}
                }
            }
        }
        let content = term.renderable_content();
        let selection = content.selection;
        let mut matches = Vec::new();
        if !search.is_empty() {
            for line in 0..rows {
                let grid_line = Line(line as i32 - offset as i32);
                let text: String = (0..cols)
                    .map(|col| term.grid()[Point::new(grid_line, Column(col))].c)
                    .collect();
                for (start, end) in search_columns(&text, search) {
                    matches.push((line, start, end));
                }
            }
        }
        for indexed in content.display_iter {
            let screen_row = indexed.point.line.0 + offset as i32;
            if screen_row < 0 || screen_row >= rows as i32 {
                continue;
            }
            let col = indexed.point.column.0;
            let cell = indexed.cell;
            let pos =
                rect.min + Vec2::new(col as f32 * cell_width, screen_row as f32 * cell_height);
            let cell_rect = Rect::from_min_size(pos, Vec2::new(cell_width, cell_height));
            let mut fg = palette.color(cell.fg);
            let mut bg = palette.color(cell.bg);
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if cell.flags.contains(Flags::DIM) {
                fg = fg.lerp_to_gamma(palette.terminal, 0.35);
            }
            if cell.flags.contains(Flags::BOLD) {
                fg = fg.lerp_to_gamma(
                    if palette.mode == crate::palette::Mode::Dark {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::BLACK
                    },
                    0.12,
                );
            }
            if bg != palette.terminal {
                painter.rect_filled(cell_rect, 0, bg);
            }
            if selection
                .as_ref()
                .is_some_and(|s| s.contains(indexed.point))
            {
                painter.rect_filled(cell_rect, 0, accent.lerp_to_gamma(palette.terminal, 0.78));
            }
            if matches
                .iter()
                .any(|&(row, start, end)| row == screen_row as usize && col >= start && col < end)
            {
                painter.rect_filled(cell_rect, 0, accent.lerp_to_gamma(palette.terminal, 0.68));
            }
            if cell.flags.intersects(
                Flags::HIDDEN | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER,
            ) {
                continue;
            }
            if cell.c != ' ' {
                painter.text(pos, Align2::LEFT_TOP, cell.c, font.clone(), fg);
                if let Some(extra) = cell.zerowidth() {
                    for c in extra {
                        painter.text(pos, Align2::LEFT_TOP, c, font.clone(), fg);
                    }
                }
            }
            if cell
                .flags
                .intersects(Flags::UNDERLINE | Flags::DOUBLE_UNDERLINE)
            {
                painter.line_segment(
                    [
                        cell_rect.left_bottom() - Vec2::new(0.0, 3.0),
                        cell_rect.right_bottom() - Vec2::new(0.0, 3.0),
                    ],
                    Stroke::new(1.0_f32, fg),
                );
            }
        }
        if focused && offset == 0 && mode.contains(TermMode::SHOW_CURSOR) && !session.exited {
            let cursor = content.cursor.point;
            let pos = rect.min
                + Vec2::new(
                    cursor.column.0 as f32 * cell_width,
                    cursor.line.0 as f32 * cell_height,
                );
            let caret = Rect::from_min_size(pos, Vec2::new(2.0, cell_height - 2.0));
            painter.rect_filled(caret, 1, accent);
        }
    }
    if let Some(text) = copy {
        ui.ctx().copy_text(text);
    }
    if scroll != 0 {
        session.scroll(scroll);
    }
    if !outgoing.is_empty() {
        session.bottom();
    }
    for bytes in outgoing {
        crate::history::record_input(&mut editor, &bytes, &mut session.commands);
        session.send_user(bytes);
    }
    response.context_menu(|ui| {
        let selection = session
            .term
            .lock()
            .ok()
            .and_then(|term| term.selection_to_string());
        if ui
            .add_enabled(
                selection.as_ref().is_some_and(|s| !s.is_empty()),
                egui::Button::new(xpeed_i18n::t("Insert into chat")),
            )
            .clicked()
        {
            session.chat_selection = selection;
            ui.close();
        }
        if ui
            .add(
                egui::Button::new(xpeed_i18n::t("Copiar selección"))
                    .shortcut_text(egui::RichText::new("Ctrl+Shift+C").small()),
            )
            .clicked()
        {
            if let Ok(term) = session.term.lock()
                && let Some(text) = term.selection_to_string()
            {
                ui.ctx().copy_text(text);
            }
            ui.close();
        }
        if ui
            .add(
                egui::Button::new(xpeed_i18n::t("Pegar"))
                    .shortcut_text(egui::RichText::new("Ctrl+V").small()),
            )
            .clicked()
        {
            response.request_focus();
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            ui.close();
        }
        if ui.button(xpeed_i18n::t("Ir al final")).clicked() {
            session.bottom();
            ui.close();
        }
    });
    response.clicked() || response.drag_started()
}

fn selection_drag_scroll(pointer_y: f32, rect: Rect, cell_height: f32) -> i32 {
    let edge = (cell_height * 1.5).max(18.0);
    if pointer_y < rect.top() + edge {
        (((rect.top() + edge - pointer_y) / cell_height).ceil() as i32).clamp(1, 6)
    } else if pointer_y > rect.bottom() - edge {
        -(((pointer_y - (rect.bottom() - edge)) / cell_height).ceil() as i32).clamp(1, 6)
    } else {
        0
    }
}

pub(crate) fn command_editor<T: alacritty_terminal::event::EventListener>(
    term: &alacritty_terminal::term::Term<T>,
    cols: usize,
) -> Option<crate::history::Editor> {
    let cursor = term.grid().cursor.point;
    for row in (0..=cursor.line.0).rev().take(8) {
        let text: String = (row..=cursor.line.0)
            .flat_map(|line| {
                (0..cols).map(move |col| term.grid()[Point::new(Line(line), Column(col))].c)
            })
            .collect();
        if let Some(end) = crate::history::prompt_end(&text) {
            let prefix = text[..end].chars().count();
            let command: String = text[end..].to_string();
            let text: Vec<char> = command.trim_end().chars().collect();
            let position = ((cursor.line.0 - row) as usize * cols + cursor.column.0)
                .saturating_sub(prefix)
                .min(text.len());
            return Some(crate::history::Editor {
                text,
                cursor: position,
            });
        }
        // Search preceding rows only when this is a wrapped input line.
        // A password prompt on a new line must never reuse an earlier shell prompt.
        if row == 0
            || !term.grid()[Point::new(Line(row - 1), Column(cols - 1))]
                .flags
                .contains(Flags::WRAPLINE)
        {
            break;
        }
    }

    None
}

fn search_columns(text: &str, needle: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let length = needle.chars().count();
    if length == 0 {
        return Vec::new();
    }
    let needle = needle.to_lowercase();
    chars
        .windows(length)
        .enumerate()
        .filter_map(|(column, chunk)| {
            (chunk.iter().collect::<String>().to_lowercase() == needle)
                .then_some((column, column + length))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn macos_prompt_is_recognized_when_hostname_wraps_across_rows() {
        struct Listener;
        impl alacritty_terminal::event::EventListener for Listener {
            fn send_event(&self, _: alacritty_terminal::event::Event) {}
        }
        for cols in [32, 80, 120] {
            let mut term = alacritty_terminal::term::Term::new(
                alacritty_terminal::term::Config::default(),
                &Size { cols, rows: 24 },
                Listener,
            );
            let mut parser: alacritty_terminal::vte::ansi::Processor = Default::default();
            parser.advance(&mut term, b"iad11-ei415-fe68b8d1-3463-469c-99dd-9c62d1a2c44b-BA495D41329B:xpeed runner$ echo hello");
            let editor = command_editor(&term, cols).expect("wrapped macOS prompt");
            assert_eq!(editor.text.iter().collect::<String>(), "echo hello");
            assert_eq!(editor.cursor, "echo hello".len());
        }
    }
    #[test]
    fn command_capture_does_not_reuse_a_prompt_before_a_password_question() {
        struct Listener;
        impl alacritty_terminal::event::EventListener for Listener {
            fn send_event(&self, _: alacritty_terminal::event::Event) {}
        }
        let mut term = alacritty_terminal::term::Term::new(
            alacritty_terminal::term::Config::default(),
            &Size { cols: 80, rows: 24 },
            Listener,
        );
        let mut parser: alacritty_terminal::vte::ansi::Processor =
            alacritty_terminal::vte::ansi::Processor::new();
        parser.advance(
            &mut term,
            b"PS C:\\test> ssh server\r\nuser@server's password: ",
        );
        assert!(command_editor(&term, 80).is_none());
        parser.advance(&mut term, b"\r\nPS C:\\test> echo hola");
        let editor = command_editor(&term, 80).unwrap();
        assert_eq!(editor.text.iter().collect::<String>().trim(), "echo hola");
    }
    #[test]
    fn search_uses_terminal_columns_instead_of_utf8_byte_offsets() {
        assert_eq!(search_columns("áé Xpeed ñ", "XPEED"), vec![(3, 8)]);
        assert_eq!(search_columns("İ ABC", "abc"), vec![(2, 5)]);
        assert_eq!(search_columns("aa aa", "aa"), vec![(0, 2), (3, 5)]);
    }
    #[test]
    fn dragging_selection_near_terminal_edges_scrolls_in_both_directions() {
        let rect = Rect::from_min_max(Pos2::new(0.0, 10.0), Pos2::new(200.0, 210.0));
        assert!(selection_drag_scroll(10.0, rect, 20.0) > 0);
        assert_eq!(selection_drag_scroll(100.0, rect, 20.0), 0);
        assert!(selection_drag_scroll(210.0, rect, 20.0) < 0);
        assert_eq!(selection_drag_scroll(-500.0, rect, 20.0), 6);
        assert_eq!(selection_drag_scroll(500.0, rect, 20.0), -6);
    }
}
