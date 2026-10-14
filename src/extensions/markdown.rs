//! Shared response typography. Images are not fetched and local links copy their path.
use eframe::egui::{self, RichText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use std::ops::Range;

fn code_blocks(text: &str) -> Vec<(Range<usize>, String, String)> {
    let mut blocks = Vec::new();
    let mut current = None;
    for (event, range) in Parser::new(text).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                    CodeBlockKind::Indented => String::new(),
                };
                current = Some((range.start, language, String::new()));
            }
            Event::Text(value) => {
                if let Some((_, _, content)) = &mut current {
                    content.push_str(&value);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((start, language, content)) = current.take() {
                    blocks.push((start..range.end, language, content));
                }
            }
            _ => {}
        }
    }
    blocks
}

fn prose(ui: &mut egui::Ui, cache: &mut CommonMarkCache, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    cache.link_hooks_clear();
    for event in Parser::new(text) {
        if let Event::Start(Tag::Link { dest_url, .. }) = event
            && !dest_url.starts_with("https://")
            && !dest_url.starts_with("http://")
        {
            cache.add_link_hook(dest_url.to_string());
        }
    }
    CommonMarkViewer::new().show(ui, cache, text);
    for (path, clicked) in cache.link_hooks() {
        if *clicked {
            ui.ctx().copy_text(path.clone());
        }
    }
}

pub(super) fn show(ui: &mut egui::Ui, cache: &mut CommonMarkCache, text: &str) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        let mut end = 0;
        for (index, (range, language, content)) in code_blocks(text).into_iter().enumerate() {
            prose(ui, cache, &text[end..range.start]);
            ui.push_id(("response_code", index), |ui| {
                egui::Frame::new()
                    .fill(ui.visuals().faint_bg_color)
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .corner_radius(12)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if language.is_empty() {
                                    "code"
                                } else {
                                    &language
                                })
                                .small()
                                .strong(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    super::copy_button(ui, "code_copy", &content);
                                },
                            );
                        });
                        let theme =
                            egui_extras::syntax_highlighting::CodeTheme::from_style(ui.style());
                        let mut job = egui_extras::syntax_highlighting::highlight(
                            ui.ctx(),
                            ui.style(),
                            &theme,
                            content.trim_end_matches('\n'),
                            &language,
                        );
                        job.wrap.max_width = ui.available_width();
                        ui.add(egui::Label::new(job).wrap().selectable(true));
                    });
            });
            end = range.end;
        }
        prose(ui, cache, &text[end..]);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_copy_preserves_unicode_and_handles_partial_streams() {
        let text = "Antes **bien**.\n\n```sh\necho caf\u{e9}\n```\n\nDespu\u{e9}s\n\n~~~python\nprint('ok')";
        let blocks = code_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].1, "sh");
        assert_eq!(blocks[0].2, "echo caf\u{e9}\n");
        assert_eq!(blocks[1].2, "print('ok')");
        assert!(text[blocks[0].0.end..blocks[1].0.start].contains("Despu\u{e9}s"));
    }
}
