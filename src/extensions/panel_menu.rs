use eframe::egui::{self, Event, Id, PointerButton, TextBuffer};
use xpeed_i18n::t;

fn opening_editor() -> Id {
    Id::new("xpeed_opening_extension_editor_menu")
}

#[derive(Clone)]
struct Editor {
    id: Id,
    range: Option<egui::text::CCursorRange>,
    text: String,
}

#[derive(Clone, Default)]
struct Selection {
    text: String,
    editor: Option<Editor>,
}

// egui resolves selections spanning labels at the end of the pass. Capture
// its copy command on menu open, without overwriting the system clipboard.
#[derive(Default)]
struct LabelCapture {
    pending: Option<(Id, usize)>,
    preserve_selection: bool,
    end_pointer: Option<egui::PointerState>,
}

impl egui::Plugin for LabelCapture {
    fn debug_name(&self) -> &'static str {
        "XpeedPanelMenu"
    }

    fn setup(&mut self, ctx: &egui::Context) {
        // This callback runs before egui's label selection plugin. The
        // secondary press belongs to the menu, so it must not clear the range.
        ctx.on_end_pass(
            "Preserve extension text selection",
            std::sync::Arc::new(|ctx| {
                let plugin = ctx.plugin::<Self>();
                let mut capture = plugin.lock();
                if std::mem::take(&mut capture.preserve_selection) {
                    capture.end_pointer =
                        Some(ctx.input_mut(|input| std::mem::take(&mut input.pointer)));
                }
            }),
        );
    }

    fn on_end_pass(&mut self, ctx: &egui::Context) {
        if let Some(pointer) = self.end_pointer.take() {
            ctx.input_mut(|input| input.pointer = pointer);
        }
        let Some((id, first_command)) = self.pending.take() else {
            return;
        };
        let text = ctx.output_mut(|output| {
            let index = output
                .commands
                .get(first_command..)?
                .iter()
                .rposition(|command| matches!(command, egui::OutputCommand::CopyText(_)))?;
            match output.commands.remove(first_command + index) {
                egui::OutputCommand::CopyText(text) => Some(text),
                _ => None,
            }
        });
        if let Some(text) = text {
            ctx.data_mut(|data| {
                let mut selection = data.get_temp::<Selection>(id).unwrap_or_default();
                if selection.editor.is_none() {
                    selection.text = text;
                    data.insert_temp(id, selection);
                }
            });
            ctx.request_repaint();
        }
    }
}

pub(super) struct Menu {
    id: Id,
    opening: bool,
    tracking: bool,
    capture_pointer: Option<egui::PointerState>,
    synthetic_copy: bool,
}

impl Menu {
    pub(super) fn begin(ui: &mut egui::Ui, enabled: bool) -> Option<Self> {
        ui.style_mut().interaction.selectable_labels = true;
        if !enabled {
            return None;
        }
        let ctx = ui.ctx();
        let id = ui.id().with("extension_panel_menu");
        let inside = ui.rect_contains_pointer(ui.available_rect_before_wrap());
        let pressed =
            inside && ui.input(|input| input.pointer.button_pressed(PointerButton::Secondary));
        let opening =
            inside && ui.input(|input| input.pointer.button_clicked(PointerButton::Secondary));
        let tracking = pressed || opening;
        let secondary_only = inside
            && ui.input(|input| {
                input.pointer.button_down(PointerButton::Secondary)
                    && !input.pointer.button_down(PointerButton::Primary)
            });
        let mut synthetic_copy = false;
        if tracking {
            let pos = ui.input(|input| input.pointer.interact_pos());
            ctx.data_mut(|data| {
                if pressed {
                    data.insert_temp(id, Selection::default());
                }
                if let Some(pos) = pos {
                    data.insert_temp(opening_editor(), (id, pos));
                }
            });
            if ctx
                .plugin::<egui::text_selection::LabelSelectionState>()
                .lock()
                .has_selection()
            {
                let plugin = ctx.plugin_or_default::<LabelCapture>();
                let mut capture = plugin.lock();
                capture.preserve_selection |= pressed;
                if pressed && !ui.input(|input| input.events.contains(&Event::Copy)) {
                    let first_command = ctx.output(|output| output.commands.len());
                    capture.pending = Some((id, first_command));
                    synthetic_copy = true;
                }
            }
        }
        // Keep the right click out of the text widgets. The popup handles
        // both press and release after the original pointer is restored.
        let capture_pointer = (tracking || secondary_only).then(|| {
            ui.input_mut(|input| {
                if synthetic_copy {
                    input.events.push(Event::Copy);
                }
                std::mem::take(&mut input.pointer)
            })
        });
        Some(Self {
            id,
            opening,
            tracking,
            capture_pointer,
            synthetic_copy,
        })
    }

    pub(super) fn show(self, ui: &mut egui::Ui, actions: impl FnOnce(&mut egui::Ui)) {
        if self.tracking {
            ui.data_mut(|data| data.remove::<(Id, egui::Pos2)>(opening_editor()));
        }
        if let Some(pointer) = self.capture_pointer {
            ui.input_mut(|input| {
                input.pointer = pointer;
                if self.synthetic_copy {
                    input.events.retain(|event| *event != Event::Copy);
                }
            });
        }
        let ctx = ui.ctx().clone();
        let selection = ctx
            .data(|data| data.get_temp::<Selection>(self.id))
            .unwrap_or_default();
        let response = ui.response();
        egui::Popup::context_menu(&response)
            .id(self.id.with("popup"))
            .open_memory(self.opening.then_some(egui::SetOpenCommand::Bool(true)))
            .show(|ui| {
                if ui
                    .add_enabled(!selection.text.is_empty(), egui::Button::new(t("Copiar")))
                    .clicked()
                {
                    ctx.copy_text(selection.text.clone());
                    ui.close();
                }
                let editor = selection.editor.as_ref();
                if ui
                    .add_enabled(
                        editor.is_some_and(|editor| !editor.text.is_empty()),
                        egui::Button::new(t("Cortar")),
                    )
                    .clicked()
                    && let Some(editor) = editor
                {
                    ctx.data_mut(|data| {
                        data.insert_temp(editor.id.with("panel_edit"), Some((editor.clone(), true)))
                    });
                    ui.close();
                }
                if ui
                    .add_enabled(editor.is_some(), egui::Button::new(t("Pegar")))
                    .clicked()
                    && let Some(editor) = editor
                {
                    ctx.data_mut(|data| {
                        data.insert_temp(
                            editor.id.with("panel_edit"),
                            Some((editor.clone(), false)),
                        )
                    });
                    ui.close();
                }
                actions(ui);
            });
    }
}

pub(super) const TEXT_SCROLL: egui::scroll_area::ScrollSource = egui::scroll_area::ScrollSource {
    drag: false,
    ..egui::scroll_area::ScrollSource::ALL
};

pub(super) fn before_editor(ui: &egui::Ui, id: Id, text: &mut String) {
    let action = ui
        .data_mut(|data| data.remove_temp::<Option<(Editor, bool)>>(id.with("panel_edit")))
        .flatten();
    let Some((editor, cut)) = action else { return };
    let mut range = editor.range;
    if cut {
        if let Some(selected) = range
            && text.char_range(selected.as_sorted_char_range()) == editor.text
        {
            ui.ctx().copy_text(editor.text);
            text.delete_char_range(selected.as_sorted_char_range());
            range = Some(egui::text::CCursorRange::one(selected.sorted_cursors()[0]));
        }
    } else {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
    }
    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    state.cursor.set_char_range(range);
    state.store(ui.ctx(), id);
    ui.memory_mut(|memory| memory.request_focus(id));
}

pub(super) fn remember_editor(ui: &egui::Ui, text: &str, output: &egui::text_edit::TextEditOutput) {
    let range = output
        .cursor_range
        .filter(|range| !range.is_empty())
        .or_else(|| {
            ui.data(|data| data.get_temp::<(Id, egui::Pos2)>(opening_editor()))
                .map(|(_, pos)| {
                    egui::text::CCursorRange::one(
                        output.galley.cursor_from_pos(pos - output.galley_pos),
                    )
                })
        });
    remember_editor_range(ui, text, &output.response, range);
}

pub(super) fn remember_editor_range(
    ui: &egui::Ui,
    text: &str,
    response: &egui::Response,
    range: Option<egui::text::CCursorRange>,
) {
    if !response.enabled() {
        return;
    }
    ui.data_mut(|data| {
        let Some((panel, pos)) = data.get_temp::<(Id, egui::Pos2)>(opening_editor()) else {
            return;
        };
        if !response.rect.intersect(ui.clip_rect()).contains(pos) {
            return;
        }
        let selected = range.map_or("", |range| text.char_range(range.as_sorted_char_range()));
        if selected.is_empty()
            && data.get_temp::<Selection>(panel).is_some_and(|previous| {
                previous
                    .editor
                    .is_some_and(|editor| editor.id == response.id && !editor.text.is_empty())
            })
        {
            return;
        }
        data.insert_temp(
            panel,
            Selection {
                text: selected.into(),
                editor: Some(Editor {
                    id: response.id,
                    range,
                    text: selected.into(),
                }),
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutting_uses_the_original_unicode_selection_and_paste_requests_the_clipboard() {
        let ctx = egui::Context::default();
        let id = Id::new("editable_draft");
        let editor = Editor {
            id,
            text: "漢á".into(),
            range: Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(1),
                egui::text::CCursor::new(3),
            )),
        };
        let mut draft = "a漢áz".to_owned();

        ctx.data_mut(|data| data.insert_temp(id.with("panel_edit"), Some((editor.clone(), true))));
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| before_editor(ui, id, &mut draft));
        });
        assert_eq!(draft, "az");
        assert!(
            output
                .platform_output
                .commands
                .contains(&egui::OutputCommand::CopyText("漢á".into()))
        );

        ctx.data_mut(|data| data.insert_temp(id.with("panel_edit"), Some((editor.clone(), false))));
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| before_editor(ui, id, &mut draft));
        });
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::RequestPaste))
        );
    }

    #[test]
    fn context_menu_copies_only_the_selected_label_text() {
        let ctx = egui::Context::default();
        ctx.options_mut(|options| options.max_passes = std::num::NonZeroUsize::new(1).unwrap());
        let mut menu_id = Id::NULL;
        let mut time = 0.0;
        let mut render = |events| {
            time += 0.4;
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(500.0, 320.0),
                    )),
                    events,
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let menu = Menu::begin(ui, true).unwrap();
                        menu_id = menu.id;
                        ui.label("Alpha bravo charlie delta");
                        menu.show(ui, |ui| {
                            ui.label("Extension action");
                        });
                    });
                },
            )
        };
        let button = |pos, button, pressed| Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let first = egui::pos2(8.0, 15.0);
        let last = egui::pos2(64.0, 15.0);
        render(vec![]);
        render(vec![
            Event::PointerMoved(first),
            button(first, PointerButton::Primary, true),
        ]);
        render(vec![Event::PointerMoved(last)]);
        render(vec![button(last, PointerButton::Primary, false)]);
        assert!(
            ctx.plugin::<egui::text_selection::LabelSelectionState>()
                .lock()
                .has_selection()
        );
        let keyboard = render(vec![Event::Copy]);
        let expected = keyboard
            .platform_output
            .commands
            .into_iter()
            .find_map(|command| {
                if let egui::OutputCommand::CopyText(text) = command {
                    Some(text)
                } else {
                    None
                }
            })
            .expect("a keyboard selection");
        assert!(
            expected.starts_with("Alpha") && !expected.contains("delta"),
            "{expected}"
        );
        let right = egui::pos2(45.0, 15.0);
        let opened = render(vec![
            Event::PointerMoved(right),
            button(right, PointerButton::Secondary, true),
        ]);
        assert!(
            !opened
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(_)))
        );
        render(vec![button(right, PointerButton::Secondary, false)]);
        let popup = render(vec![]);
        assert!(
            ctx.plugin::<egui::text_selection::LabelSelectionState>()
                .lock()
                .has_selection(),
            "opening the context menu must preserve the highlight"
        );
        let copy = popup
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == t("Copiar") => {
                    Some(text.pos + egui::vec2(10.0, 7.0))
                }
                _ => None,
            })
            .expect("clipboard menu");
        assert!(popup.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Extension action")));
        render(vec![
            Event::PointerMoved(copy),
            button(copy, PointerButton::Primary, true),
        ]);
        let finished = render(vec![button(copy, PointerButton::Primary, false)]);
        let selection = ctx
            .data(|data| data.get_temp::<Selection>(menu_id))
            .unwrap();
        assert_eq!(selection.text, expected);
        assert!(
            finished
                .platform_output
                .commands
                .contains(&egui::OutputCommand::CopyText(selection.text))
        );
    }

    #[test]
    fn codex_question_and_markdown_response_support_drag_selection() {
        for label in ["Inspect the application logs.", "The service is running. "] {
            let ctx = egui::Context::default();
            let mut extensions = super::super::Extensions::new(std::path::Path::new("."), true);
            let chat = extensions.performance_chat(1);
            let mut time = 0.0;
            let mut render = |events| {
                time += 0.1;
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(520.0, 670.0),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| extensions.view_ui(ui, chat));
                    },
                )
            };
            render(vec![]);
            let output = render(vec![]);
            let (start, end) = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(shape) if shape.galley.text().starts_with(label) => {
                        let y = shape.galley.size().y.min(18.0) / 2.0;
                        Some((
                            shape.pos + egui::vec2(2.0, y),
                            shape.pos + egui::vec2(52.0, y),
                        ))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("text was not rendered: {label:?}"));
            let press = |pos, pressed| Event::PointerButton {
                pos,
                pressed,
                button: PointerButton::Primary,
                modifiers: egui::Modifiers::NONE,
            };
            render(vec![Event::PointerMoved(start), press(start, true)]);
            render(vec![Event::PointerMoved(end)]);
            render(vec![press(end, false)]);
            let copied = render(vec![Event::Copy]);
            let selected = copied
                .platform_output
                .commands
                .iter()
                .find_map(|command| {
                    if let egui::OutputCommand::CopyText(text) = command {
                        Some(text)
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| panic!("drag selected no text in {label:?}"));
            assert!(
                !selected.trim().is_empty() && label.contains(selected.trim()),
                "{selected:?}"
            );
            assert!(
                selected.len() < label.len(),
                "copied more than the dragged fragment"
            );
            let word = start + egui::vec2(9.0, 0.0);
            render(vec![Event::PointerMoved(word)]);
            for _ in 0..2 {
                render(vec![press(word, true)]);
                render(vec![press(word, false)]);
            }
            let expected = label.split_whitespace().next().unwrap();
            let double_click = render(vec![Event::Copy]);
            assert!(
                double_click
                    .platform_output
                    .commands
                    .contains(&egui::OutputCommand::CopyText(expected.into()))
            );
            for pressed in [true, false] {
                render(vec![Event::PointerButton {
                    pos: word,
                    pressed,
                    button: PointerButton::Secondary,
                    modifiers: egui::Modifiers::NONE,
                }]);
                if pressed {
                    render(vec![]);
                }
            }
            let after_menu = render(vec![Event::Copy]);
            assert!(
                after_menu
                    .platform_output
                    .commands
                    .contains(&egui::OutputCommand::CopyText(expected.into())),
                "{label}: expected {expected:?}, got {:?}",
                after_menu.platform_output.commands
            );
        }
    }
}
