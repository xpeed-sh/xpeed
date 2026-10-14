use super::{ChatMessage, Runtime, approvals_ui, session_tools};
use eframe::egui::{self, RichText};
use serde_json::json;
use std::collections::BTreeMap;
use xpeed_i18n::t;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    SignIn,
    Welcome,
    Conversations,
    Chat,
}

fn title(messages: &[ChatMessage]) -> String {
    messages.iter().find(|m| m.role == "user").map_or_else(
        || t("Nueva conversación").into(),
        |m| {
            let line = m.text.lines().next().unwrap_or_default();
            let mut result: String = line.chars().take(52).collect();
            if line.chars().count() > 52 {
                result.push('…');
            }
            result
        },
    )
}

fn new_conversation(runtime: &mut Runtime) {
    runtime.request("conversation/reset", json!({}));
    runtime.archive_conversation();
    runtime.last_config.clear();
    runtime.error = None;
    runtime.draft.clear();
    runtime.chat_page = Page::Chat;
}

pub(super) fn show(
    runtime: &mut Runtime,
    ui: &mut egui::Ui,
    targets: &BTreeMap<u64, session_tools::Target>,
    fragment: Option<&(u64, String)>,
) {
    if runtime.auto_connect && runtime.process.is_none() {
        runtime.connect(ui.ctx());
    }
    let authenticated = runtime.ready && runtime.account.is_some();
    if !authenticated {
        runtime.chat_page = Page::SignIn;
    } else if runtime.chat_page == Page::SignIn {
        runtime.chat_page = Page::Welcome;
    }
    if runtime.login_after_connect
        && runtime.ready
        && !runtime.pending.values().any(|p| p.method == "account/read")
    {
        runtime.login_after_connect = false;
        if runtime.account.is_none() {
            runtime.request("account/login", json!({}));
        }
    }
    match runtime.chat_page {
        Page::SignIn | Page::Welcome => onboarding(runtime, ui),
        Page::Conversations => conversations(runtime, ui, targets, fragment),
        Page::Chat => conversation(runtime, ui, targets, fragment),
    }
}

fn onboarding(runtime: &mut Runtime, ui: &mut egui::Ui) {
    let top_space = (ui.available_height() * 0.24).max(24.0);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(top_space);
            ui.vertical_centered(|ui| {
                ui.heading("Codex");
                ui.add_space(16.0);
                let welcome = runtime.chat_page == Page::Welcome;
                ui.label(
                    RichText::new(t(if welcome {
                        "Welcome to Codex"
                    } else {
                        "Sign in to get started"
                    }))
                    .size(19.0),
                );
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(t(if welcome {
                        "Your account is connected. Choose a conversation or start a new one."
                    } else {
                        "Connect your ChatGPT account to work with your terminals and servers."
                    }))
                    .wrap(),
                );
                ui.add_space(22.0);
                if welcome {
                    if ui
                        .add_sized([160.0, 38.0], egui::Button::new(t("Continue")))
                        .clicked()
                    {
                        runtime.chat_page = Page::Conversations;
                    }
                } else {
                    let waiting = !runtime.pending.is_empty() || runtime.login.is_some();
                    if ui
                        .add_enabled(
                            !waiting,
                            egui::Button::new(t("Conectar")).min_size(egui::vec2(160.0, 38.0)),
                        )
                        .clicked()
                    {
                        if runtime.process.is_none() {
                            runtime.connect(ui.ctx());
                            runtime.login_after_connect = true;
                        } else if runtime.ready {
                            runtime.request("account/login", json!({}));
                        }
                    }
                    if waiting {
                        ui.add_space(12.0);
                        ui.spinner();
                        ui.label(&runtime.status);
                    }
                    if let Some(login) = runtime.login.clone() {
                        if let Some(url) = login["authUrl"]
                            .as_str()
                            .or(login["verificationUrl"].as_str())
                            && url.starts_with("https://")
                        {
                            ui.hyperlink_to(t("Abrir inicio de sesión en el navegador"), url);
                        }
                        if let Some(code) = login["userCode"].as_str() {
                            ui.label(code);
                        }
                        if ui.button(t("Actualizar sesión")).clicked() {
                            runtime.request("account/read", json!({}));
                        }
                        if ui.button(t("Cancelar inicio de sesión")).clicked() {
                            runtime.request(
                                "account/login/cancel",
                                json!({"loginId":login["loginId"]}),
                            );
                            runtime.login = None;
                        }
                    }
                }
                error(runtime, ui);
            });
        });
}

fn conversations(
    runtime: &mut Runtime,
    ui: &mut egui::Ui,
    targets: &BTreeMap<u64, session_tools::Target>,
    fragment: Option<&(u64, String)>,
) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.heading(t("Chats"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            selected_sessions(runtime, ui, targets, false);
            ui.menu_button("...", |ui| account_settings(runtime, ui));
        });
    });
    ui.add_space(10.0);
    let mut resume = None;
    let mut delete = None;
    let can_delete = !runtime.busy && runtime.deferred_actions.is_empty();
    let footer_id = ui.id().with(("chat_list_footer", &runtime.manifest.id));
    let footer_height = ui
        .ctx()
        .data(|data| data.get_temp::<f32>(footer_id))
        .unwrap_or(340.0);
    egui::ScrollArea::vertical()
        .max_height((ui.available_height() - footer_height).max(40.0))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if !runtime.messages.is_empty() || !runtime.draft.is_empty() || runtime.busy {
                let name = if runtime.chat_title.is_empty() {
                    title(&runtime.messages)
                } else {
                    runtime.chat_title.clone()
                };
                let (open, remove) = chat_row(ui, "current_chat", &name, can_delete);
                if remove {
                    delete = Some(None);
                } else if open {
                    runtime.chat_page = Page::Chat;
                }
            }
            for (index, (key, messages, _)) in runtime.archives.iter().enumerate().rev() {
                let name = runtime
                    .saved_titles
                    .get(key)
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_else(|| title(messages));
                let (open, remove) = chat_row(ui, key, &name, can_delete);
                if remove {
                    delete = Some(Some(index));
                } else if open {
                    resume = Some(index);
                }
            }
            if runtime.messages.is_empty() && runtime.archives.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(t("No conversations yet. Start your first conversation."));
                });
            }
            error(runtime, ui);
        });
    if let Some(index) = delete {
        runtime.delete_chat(index);
    } else if let Some(index) = resume {
        runtime.resume_saved(index);
    }
    let footer_start = ui.cursor().min.y;
    ui.separator();
    usage_limits(runtime, ui);
    ui.add_space(8.0);
    composer(runtime, ui, targets, fragment, true);
    let footer_height = ui.cursor().min.y - footer_start + ui.spacing().item_spacing.y;
    ui.ctx()
        .data_mut(|data| data.insert_temp(footer_id, footer_height));
}

fn chat_row(ui: &mut egui::Ui, key: &str, name: &str, can_delete: bool) -> (bool, bool) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::click());
    let hovered = ui.rect_contains_pointer(rect);
    if hovered {
        ui.painter()
            .rect_filled(rect, 6.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let text_rect = egui::Rect::from_min_max(
        rect.min + egui::vec2(4.0, 0.0),
        rect.max - egui::vec2(36.0, 0.0),
    );
    ui.put(
        text_rect,
        egui::Label::new(name)
            .truncate()
            .halign(egui::Align::Min)
            .selectable(false)
            .sense(egui::Sense::hover()),
    );
    let mut removed = false;
    if hovered {
        let delete_rect = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 16.0, rect.center().y),
            egui::vec2(26.0, 26.0),
        );
        let delete_response = ui.interact(
            delete_rect,
            ui.id().with(("delete_chat", key)),
            if can_delete {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        let color = if delete_response.hovered() && can_delete {
            ui.visuals().error_fg_color
        } else {
            ui.visuals().weak_text_color()
        };
        draw_trash(ui.painter(), delete_rect.center(), color);
        removed = can_delete
            && (delete_response.on_hover_text(t("Delete chat")).clicked()
                || (response.clicked()
                    && ui.input(|i| {
                        i.pointer
                            .interact_pos()
                            .is_some_and(|pos| delete_rect.contains(pos))
                    })));
    }
    (
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
            && !removed,
        removed,
    )
}

fn draw_trash(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.3_f32, color);
    painter.rect_stroke(
        egui::Rect::from_min_max(
            center + egui::vec2(-5.0, -4.0),
            center + egui::vec2(5.0, 7.0),
        ),
        2.0,
        stroke,
        egui::StrokeKind::Inside,
    );
    for (a, b) in [
        ((-7.0, -6.0), (7.0, -6.0)),
        ((-3.0, -8.0), (3.0, -8.0)),
        ((-2.0, -1.0), (-2.0, 4.0)),
        ((2.0, -1.0), (2.0, 4.0)),
    ] {
        painter.line_segment(
            [center + egui::vec2(a.0, a.1), center + egui::vec2(b.0, b.1)],
            stroke,
        );
    }
}

fn conversation(
    runtime: &mut Runtime,
    ui: &mut egui::Ui,
    targets: &BTreeMap<u64, session_tools::Target>,
    fragment: Option<&(u64, String)>,
) {
    if runtime.chat_title.is_empty() && !runtime.messages.is_empty() {
        runtime.chat_title = title(&runtime.messages);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let side = 66.0;
        let title_width = (ui.available_width() - side * 2.0 - 8.0).max(10.0);
        ui.allocate_ui_with_layout(
            egui::vec2(side, 28.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(side);
                if arrow_button(ui, false)
                    .on_hover_text(t("Conversations"))
                    .clicked()
                {
                    runtime.chat_page = Page::Conversations;
                }
            },
        );
        ui.add_sized(
            [title_width, 28.0],
            egui::TextEdit::singleline(&mut runtime.chat_title)
                .frame(false)
                .horizontal_align(egui::Align::Center)
                .hint_text(t("New conversation")),
        );
        ui.allocate_ui_with_layout(
            egui::vec2(side, 28.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.set_min_width(side);
                ui.menu_button("...", |ui| account_settings(runtime, ui));
                selected_sessions(runtime, ui, targets, true);
            },
        );
    });
    ui.add_space(8.0);
    let reserve = 145.0 + runtime.attachments.reserved_height();
    let awaiting = runtime
        .tools
        .actions
        .iter()
        .any(|action| action.state == "awaitingApproval");
    let body_height =
        (ui.available_height() - reserve - if awaiting { 155.0 } else { 0.0 }).max(40.0);
    egui::ScrollArea::vertical()
        .id_salt(("codex_messages", runtime.thread_id.clone()))
        .max_height(body_height)
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.set_min_height(body_height);
            if runtime.messages.is_empty() {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.heading(t("What would you like to do?"));
                    ui.label(t("Choose your sessions below and describe your task."));
                });
            }
            messages_ui(ui, runtime);
            approvals_ui(runtime, ui);
            error(runtime, ui);
        });
    ui.add_space(8.0);
    pending_approvals(runtime, ui, targets);
    composer(runtime, ui, targets, fragment, false);
}

fn composer(
    runtime: &mut Runtime,
    ui: &mut egui::Ui,
    targets: &BTreeMap<u64, session_tools::Target>,
    _fragment: Option<&(u64, String)>,
    new_chat: bool,
) {
    runtime.attachments.show(ui, runtime.busy);
    let awaiting = runtime
        .tools
        .actions
        .iter()
        .any(|action| action.state == "awaitingApproval");
    if !runtime.tools.attached.is_empty() {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(t("Terminal selection attached"))
                    .small()
                    .weak(),
            );
            if ui
                .small_button("×")
                .on_hover_text(t("Remove attachment"))
                .clicked()
            {
                runtime.tools.attached.clear();
            }
        });
    }
    if awaiting || runtime.busy || !runtime.deferred_actions.is_empty() {
        let running = runtime.tools.actions.iter().any(|a| a.state == "running");
        ui.horizontal(|ui| {
            if !awaiting {
                ui.spinner();
            }
            ui.weak(t(if awaiting {
                "Waiting for approval"
            } else if running {
                "Executing in the terminal"
            } else {
                "Codex is working..."
            }));
        });
    }
    egui::Frame::group(ui.style())
        .corner_radius(22.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.add_enabled(
                !runtime.busy,
                egui::TextEdit::multiline(if new_chat {
                    &mut runtime.home_draft
                } else {
                    &mut runtime.draft
                })
                .frame(false)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text(t("Ask for changes or a task…")),
            );
            ui.horizontal(|ui| {
                let compact = ui.available_width() < 260.0;
                ui.spacing_mut().item_spacing.x = 4.0;
                let widgets = &mut ui.visuals_mut().widgets;
                widgets.inactive.corner_radius = 14.into();
                widgets.hovered.corner_radius = 14.into();
                widgets.active.corner_radius = 14.into();
                widgets.open.corner_radius = 14.into();
                let add = ui.add_enabled(
                    !runtime.busy,
                    egui::Button::new("+")
                        .corner_radius(14.0)
                        .min_size(egui::vec2(28.0, 28.0)),
                );
                egui::Popup::menu(&add).show(|ui| {
                    ui.weak(t("Add"));
                    if ui.button(t("Files and folders")).clicked() {
                        runtime.attachments.open();
                        ui.close();
                    }
                });
                task_access(runtime, ui, compact);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if runtime.busy || !runtime.deferred_actions.is_empty() {
                        if ui.button("■").on_hover_text(t("Detener")).clicked() {
                            runtime.tools.stop();
                            runtime.deferred_actions.clear();
                            if runtime.busy {
                                runtime.request("chat/interrupt", json!({}));
                            }
                        }
                    } else if ui
                        .add_enabled_ui(
                            !awaiting
                                && !(if new_chat {
                                    &runtime.home_draft
                                } else {
                                    &runtime.draft
                                })
                                .trim()
                                .is_empty()
                                && !runtime.tools.selected.is_empty(),
                            |ui| arrow_button(ui, true),
                        )
                        .inner
                        .on_hover_text(t("Enviar"))
                        .clicked()
                    {
                        if new_chat {
                            send_new(runtime, targets);
                        } else {
                            send(runtime, targets);
                        }
                    }
                    let model_width = (ui.available_width() - 12.0).clamp(30.0, 180.0);
                    egui::ComboBox::from_id_salt("codex_composer_model")
                        .width(model_width)
                        .truncate()
                        .selected_text(if runtime.model.is_empty() {
                            t("Predeterminado")
                        } else {
                            &runtime.model
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut runtime.model,
                                String::new(),
                                t("Predeterminado del proveedor"),
                            );
                            for (id, name) in &runtime.models {
                                ui.selectable_value(&mut runtime.model, id.clone(), name);
                            }
                        });
                });
            });
        });
    if runtime.tools.selected.is_empty() {
        ui.label(
            RichText::new(t("Choose a session to enable sending."))
                .small()
                .color(ui.visuals().warn_fg_color),
        );
    }
}

pub(super) fn task_access(
    runtime: &mut Runtime,
    ui: &mut egui::Ui,
    compact: bool,
) -> egui::Response {
    use crate::svg_icons::{self, Icon};
    let full = runtime.tools.approve_task;
    let color_for = |full: bool| {
        if full {
            if ui.visuals().dark_mode {
                egui::Color32::from_rgb(224, 184, 42)
            } else {
                egui::Color32::from_rgb(148, 109, 0)
            }
        } else {
            ui.visuals().text_color()
        }
    };
    let color = color_for(full);
    let label = t(if full {
        "Full access"
    } else {
        "Request approval"
    });
    let response = ui
        .add_enabled(
            !runtime.busy && runtime.deferred_actions.is_empty(),
            egui::Button::new(
                RichText::new(if compact {
                    "    ".to_owned()
                } else {
                    format!("     {label}")
                })
                .color(color),
            )
            .corner_radius(14.0)
            .min_size(egui::vec2(28.0, 28.0)),
        )
        .on_hover_text(label);
    svg_icons::draw(
        ui.painter(),
        egui::Rect::from_center_size(
            egui::pos2(response.rect.left() + 14.0, response.rect.center().y),
            egui::vec2(20.0, 20.0),
        ),
        if full {
            Icon::FullAccess
        } else {
            Icon::RequestApproval
        },
        color,
    );
    egui::Popup::menu(&response).show(|ui| {
        ui.set_width(300.0_f32.min(ui.ctx().content_rect().width() - 32.0));
        ui.weak(t("How should Codex actions be approved?"));
        ui.add_space(6.0);
        for full in [false, true] {
            let color = if full {
                if ui.visuals().dark_mode {
                    egui::Color32::from_rgb(224, 184, 42)
                } else {
                    egui::Color32::from_rgb(148, 109, 0)
                }
            } else {
                ui.visuals().text_color()
            };
            let selected = runtime.tools.approve_task == full;
            let row = ui
                .horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                    svg_icons::draw(
                        ui.painter(),
                        rect,
                        if full {
                            Icon::FullAccess
                        } else {
                            Icon::RequestApproval
                        },
                        color,
                    );
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 24.0).max(80.0));
                        ui.add(
                            egui::Label::new(
                                RichText::new(t(if full {
                                    "Full access"
                                } else {
                                    "Request approval"
                                }))
                                .color(color),
                            )
                            .selectable(false)
                            .sense(egui::Sense::hover()),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(t(if full {
                                    "Allow commands and edits for this task on selected sessions."
                                } else {
                                    "Confirm each command or edit on selected sessions."
                                }))
                                .small()
                                .color(color),
                            )
                            .selectable(false)
                            .sense(egui::Sense::hover()),
                        );
                    });
                    if selected {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                        svg_icons::draw(ui.painter(), rect, Icon::AccessSelected, color);
                    }
                })
                .response;
            let hit = ui
                .interact(row.rect, row.id.with("access_choice"), egui::Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if hit.hovered() {
                ui.painter().rect_stroke(
                    row.rect.expand(2.0),
                    8.0,
                    ui.visuals().widgets.hovered.bg_stroke,
                    egui::StrokeKind::Inside,
                );
            }
            if hit.clicked() && !runtime.busy && runtime.deferred_actions.is_empty() {
                runtime.tools.approve_task = full;
                runtime.persist_chat(true);
                ui.close();
            }
            ui.add_space(5.0);
        }
    });
    response
}

fn selected_sessions(
    runtime: &mut Runtime,
    ui: &mut egui::Ui,
    targets: &BTreeMap<u64, session_tools::Target>,
    compact: bool,
) {
    ui.menu_button(if compact { runtime.tools.selected.len().to_string() } else { format!("{} ({})", t("Sessions"), runtime.tools.selected.len()) }, |ui| {
        ui.set_max_width(360.0);
        for (&id, target) in targets {
            let mut selected = runtime.tools.selected.contains(&id);
            if ui.add_enabled(!runtime.busy && target.connected, egui::Checkbox::new(&mut selected, format!("{} · #{} · {}", target.name, id, target.ssh.as_ref().map_or(target.cwd.as_str(), |ssh| ssh.host.as_str())))).changed() {
                if selected { runtime.tools.selected.insert(id); } else { runtime.tools.selected.remove(&id); }
            }
        }
        if targets.is_empty() { ui.label(t("Open a terminal session first.")); }
        ui.label(RichText::new(t("Approved commands run in the visible terminal. Keep it at an empty prompt; Ctrl+C interrupts execution.")).small());
    }).response.on_hover_text(t("Selected sessions"));
}

fn arrow_button(ui: &mut egui::Ui, up: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    let visuals = ui.style().interact(&response);
    ui.painter()
        .circle_filled(rect.center(), rect.width() / 2.0, visuals.bg_fill);
    let center = rect.center();
    let points = if up {
        [
            center + egui::vec2(-5.0, -1.0),
            center + egui::vec2(0.0, -6.0),
            center + egui::vec2(5.0, -1.0),
        ]
    } else {
        [
            center + egui::vec2(1.0, -5.0),
            center + egui::vec2(-4.0, 0.0),
            center + egui::vec2(1.0, 5.0),
        ]
    };
    let stroke = egui::Stroke::new(1.5_f32, visuals.text_color());
    ui.painter().add(egui::Shape::line(points.to_vec(), stroke));
    let line = if up {
        [
            center + egui::vec2(0.0, -5.0),
            center + egui::vec2(0.0, 6.0),
        ]
    } else {
        [
            center + egui::vec2(-4.0, 0.0),
            center + egui::vec2(7.0, 0.0),
        ]
    };
    ui.painter().line_segment(line, stroke);
    response
}

fn pending_approvals(
    runtime: &mut Runtime,
    ui: &mut egui::Ui,
    targets: &BTreeMap<u64, session_tools::Target>,
) {
    if !runtime
        .tools
        .actions
        .iter()
        .any(|action| action.state == "awaitingApproval")
    {
        return;
    }
    let mut approve = None;
    let mut decline = None;
    ui.label(
        RichText::new(t("Waiting for your approval"))
            .strong()
            .color(ui.visuals().warn_fg_color),
    );
    egui::ScrollArea::vertical()
        .id_salt("pinned_session_approvals")
        .max_height(105.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (index, action) in runtime
                .tools
                .actions
                .iter()
                .enumerate()
                .filter(|(_, action)| action.state == "awaitingApproval")
            {
                ui.push_id(index, |ui| {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(&action.target).strong()).wrap());
                        ui.horizontal(|ui| {
                            let width = ((ui.available_width() - ui.spacing().item_spacing.x)
                                / 2.0)
                                .max(35.0);
                            if ui
                                .add_sized([width, 26.0], egui::Button::new(t("Approve")))
                                .on_hover_text(t("Permitir esta acción"))
                                .clicked()
                            {
                                approve = Some(index);
                            }
                            if ui
                                .add_sized([width, 26.0], egui::Button::new(t("Rechazar")))
                                .clicked()
                            {
                                decline = Some(index);
                            }
                        });
                        if let Some(command) = action.args["command"].as_str() {
                            ui.add(egui::Label::new(RichText::new(command).monospace()).wrap());
                        }
                        if let Some(cwd) = action.args["cwd"].as_str() {
                            ui.label(RichText::new(cwd).small());
                        }
                        if action.tool == "xpeed_apply_change" {
                            ui.label(action.args["path"].as_str().unwrap_or_default());
                            ui.collapsing(t("File differences"), |ui| {
                                for (key, prefix) in [("expectedContent", "−"), ("newContent", "+")]
                                {
                                    ui.add(
                                        egui::Label::new(format!(
                                            "{prefix} {}",
                                            action.args[key].as_str().unwrap_or_default()
                                        ))
                                        .wrap(),
                                    );
                                }
                            });
                        }
                    });
                });
            }
        });
    if let Some(index) = approve {
        runtime.tools.approve(index, targets);
    }
    if let Some(index) = decline {
        runtime.tools.decline(index);
    }
    ui.add_space(6.0);
}

fn turn_response(messages: &[ChatMessage], index: usize) -> Option<String> {
    if messages[index].role != "assistant"
        || messages[index + 1..]
            .iter()
            .take_while(|m| m.role != "user")
            .any(|m| m.role == "assistant")
    {
        return None;
    }
    let first = messages[..index]
        .iter()
        .rposition(|m| m.role == "user")
        .map_or(0, |i| i + 1);
    Some(
        messages[first..=index]
            .iter()
            .filter(|m| m.role == "assistant")
            .map(|m| m.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}

fn messages_ui(ui: &mut egui::Ui, runtime: &mut Runtime) {
    let last_assistant = runtime.messages.iter().rposition(|m| m.role == "assistant");
    for (index, message) in runtime.messages.iter().enumerate() {
        ui.push_id((&message.id, index), |ui| {
            if message.role == "user" {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    egui::Frame::new()
                        .fill(ui.visuals().faint_bg_color)
                        .corner_radius(18)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.set_max_width((ui.available_width() * 0.88).max(80.0));
                            ui.add(egui::Label::new(&message.text).wrap().selectable(true));
                        });
                });
            } else {
                let response_text = turn_response(&runtime.messages, index);
                let last_in_turn = response_text.is_some();
                let first_in_turn = index == 0 || runtime.messages[index - 1].role == "user";
                let turn_end = runtime.messages[index..]
                    .iter()
                    .take_while(|m| m.role != "user")
                    .last()
                    .unwrap_or(message);
                if first_in_turn && let Some(ms) = turn_end.elapsed_ms {
                    ui.add_space(4.0);
                    ui.weak(format!("{} {}", t("Worked for"), duration_text(ms)));
                    ui.separator();
                    ui.add_space(8.0);
                }
                response_ui(ui, &mut runtime.markdown_cache, &message.text);
                if last_in_turn {
                    let actions: Vec<_> = runtime
                        .tools
                        .actions
                        .iter()
                        .filter(|a| {
                            (!a.turn.is_empty() && a.turn == message.turn)
                                || (a.turn.is_empty() && Some(index) == last_assistant)
                        })
                        .collect();
                    actions_ui(ui, &actions);
                }
                if last_in_turn {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        super::copy_button(
                            ui,
                            "response_copy",
                            response_text.as_deref().unwrap_or_default(),
                        );
                        if let Some(date) = chrono::DateTime::from_timestamp(message.created_at, 0)
                            .filter(|_| message.created_at > 0)
                        {
                            ui.label(
                                RichText::new(
                                    date.with_timezone(&chrono::Local)
                                        .format("%d/%m/%Y %H:%M")
                                        .to_string(),
                                )
                                .small()
                                .weak(),
                            );
                        }
                    });
                }
            }
        });
        ui.add_space(18.0);
    }
    // Live actions are visible even before the first response arrives.
    if let Some(user) = runtime.messages.last().filter(|m| m.role == "user") {
        let actions: Vec<_> = runtime
            .tools
            .actions
            .iter()
            .filter(|a| a.turn == user.id)
            .collect();
        actions_ui(ui, &actions);
    }
}

fn duration_text(ms: u64) -> String {
    let seconds = ms / 1000;
    if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

fn actions_ui(ui: &mut egui::Ui, actions: &[&session_tools::Action]) {
    if actions.is_empty() {
        return;
    }
    ui.add_space(12.0);
    egui::Frame::group(ui.style()).corner_radius(12).inner_margin(12).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::CollapsingHeader::new(format!("{} ({})", t(if actions.iter().all(|a| a.turn.is_empty()) { "Earlier conversation actions" } else { "Actions performed" }), actions.len()))
            .default_open(actions.len() <= 4).show(ui, |ui| {
                for (index, action) in actions.iter().enumerate() {
                    ui.push_id(index, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(&action.target);
                            ui.weak(format!("{} \u{00b7} {}", action.tool, t(&action.state)));
                        });
                        if let Some(command) = action.args["command"].as_str() {
                            ui.add(egui::Label::new(RichText::new(command).monospace()).wrap().selectable(true));
                        }
                        if let Some(path) = action.args["path"].as_str() { ui.label(path); }
                        if action.tool == "xpeed_apply_change" {
                            ui.collapsing(t("File differences"), |ui| {
                                for (key, prefix, color) in [
                                    ("expectedContent", "-", ui.visuals().error_fg_color),
                                    ("newContent", "+", ui.visuals().hyperlink_color),
                                ] {
                                    for line in action.args[key].as_str().unwrap_or_default().lines() {
                                        ui.add(egui::Label::new(RichText::new(format!("{prefix} {line}")).monospace().color(color)).wrap());
                                    }
                                }
                            });
                        }
                        if let Some(result) = &action.result {
                            ui.collapsing(t("Result"), |ui| {
                                let text = result["stdout"].as_str().map(str::to_owned)
                                    .unwrap_or_else(|| serde_json::to_string_pretty(result).unwrap_or_default());
                                ui.add(egui::Label::new(RichText::new(&text).monospace()).wrap().selectable(true));
                                if let Some(stderr) = result["stderr"].as_str().filter(|s| !s.is_empty()) {
                                    ui.colored_label(ui.visuals().error_fg_color, stderr);
                                }
                                if let Some(code) = result["exitCode"].as_i64() { ui.weak(format!("{}: {code}", t("Exit code"))); }
                                if result["remoteMayBeRunning"] == true { ui.label(t("SSH channel closed. A detached remote process may still be running.")); }
                                super::copy_button(ui, "result_copy", &serde_json::to_string_pretty(result).unwrap_or_default());
                            });
                        }
                        if index + 1 < actions.len() { ui.separator(); }
                    });
                }
            });
    });
}

fn response_ui(ui: &mut egui::Ui, cache: &mut egui_commonmark::CommonMarkCache, text: &str) {
    super::markdown::show(ui, cache, text);
}

fn usage_limits(runtime: &mut Runtime, ui: &mut egui::Ui) {
    egui::Frame::group(ui.style())
        .corner_radius(12.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.strong(t("Account usage limits"));
            ui.horizontal(|ui| {
                if let Some(plan) = runtime
                    .account
                    .as_ref()
                    .and_then(|a| a["planType"].as_str())
                {
                    ui.label(RichText::new(format!("ChatGPT · {plan}")).small().weak());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(t("Refresh")).clicked() {
                        runtime.request("account/limits", json!({}));
                    }
                });
            });
            if let Some(limits) = &runtime.limits {
                let snapshot = limits.get("rateLimits").unwrap_or(limits);
                let mut found = false;
                for key in ["primary", "secondary"] {
                    let window = &snapshot[key];
                    if let Some(used) = window["usedPercent"].as_f64() {
                        found = true;
                        let minutes = window["windowDurationMins"].as_u64().unwrap_or_default();
                        let name = match minutes {
                            10080 => t("Weekly").into(),
                            300 => "5 h".into(),
                            0 => t(key).into(),
                            _ => format!("{} h", minutes / 60),
                        };
                        let remaining = (100.0 - used).clamp(0.0, 100.0);
                        ui.add_space(6.0);
                        ui.label(format!("{name} · {remaining:.0}% {}", t("remaining")));
                        ui.add(
                            egui::ProgressBar::new((remaining / 100.0) as f32)
                                .desired_width(ui.available_width())
                                .desired_height(5.0),
                        );
                        if let Some(reset) = window["resetsAt"].as_u64() {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let mins = reset.saturating_sub(now).div_ceil(60);
                            ui.label(
                                RichText::new(format!(
                                    "{} {} h {} min",
                                    t("Resets in"),
                                    mins / 60,
                                    mins % 60
                                ))
                                .small()
                                .weak(),
                            );
                        }
                    }
                }
                if !found {
                    ui.label(t("Usage unavailable"));
                }
            } else {
                ui.label(t("Usage unavailable"));
            }
        });
}

fn account_settings(runtime: &mut Runtime, ui: &mut egui::Ui) {
    ui.label(t("Account"));
    if let Some(account) = &runtime.account {
        ui.label(account["email"].as_str().unwrap_or("ChatGPT"));
    }
    if ui.button(t("Actualizar sesión")).clicked() {
        runtime.request("account/read", json!({}));
    }
    if ui
        .add_enabled(!runtime.busy, egui::Button::new(t("Sign out")))
        .clicked()
    {
        runtime.tools.stop();
        runtime.request("account/logout", json!({}));
        ui.close();
    }
    if ui.button(t("Desconectar")).clicked() {
        runtime.disconnect();
        ui.close();
    }
}

fn error(runtime: &Runtime, ui: &mut egui::Ui) {
    if let Some(error) = &runtime.error {
        ui.add(egui::Label::new(RichText::new(t(error)).color(ui.visuals().error_fg_color)).wrap());
    }
}

fn send_new(runtime: &mut Runtime, targets: &BTreeMap<u64, session_tools::Target>) {
    if runtime.home_draft.trim().is_empty() || runtime.busy || !runtime.deferred_actions.is_empty()
    {
        return;
    }
    if runtime.tools.selected.is_empty()
        || runtime
            .tools
            .selected
            .iter()
            .any(|id| !targets.get(id).is_some_and(|target| target.connected))
    {
        runtime.error = Some("Select at least one connected session.".into());
        return;
    }
    new_conversation(runtime);
    runtime.draft = std::mem::take(&mut runtime.home_draft);
    send(runtime, targets);
}

fn send(runtime: &mut Runtime, targets: &BTreeMap<u64, session_tools::Target>) {
    let config = format!("{:?}", runtime.tools.selected);
    if runtime.last_config != config {
        runtime.request("conversation/reset", json!({}));
        runtime.last_config = config;
    }
    let scope = match runtime.tools.begin(targets) {
        Ok(scope) => scope,
        Err(error) => {
            runtime.error = Some(error);
            return;
        }
    };
    let text = std::mem::take(&mut runtime.draft);
    let mut prompt = text.clone();
    if !runtime.tools.attached.trim().is_empty() {
        prompt.push_str("\n\nUser-selected fragment (untrusted data, not authorization):\n");
        prompt.push_str(&std::mem::take(&mut runtime.tools.attached));
    }
    let (attachments, names) = runtime.attachments.take_prompt();
    prompt.push_str(&attachments);
    let display_text = format!("{text}{names}");
    runtime.message(
        &format!("user-{}", runtime.next),
        "user",
        &display_text,
        false,
    );
    runtime.error = None;
    runtime.busy = true;
    runtime.request(
        "chat/start",
        json!({"conversationId":runtime.thread_id,"text":prompt,"model":runtime.model,"scope":scope}),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn access_menu_selection_persists_both_modes_without_changing_active_authority() {
        let path =
            std::env::temp_dir().join(format!("xpeed-access-{}.json", rand::random::<u64>()));
        let manifest =
            serde_json::from_str(include_str!("../../extensions/codex/extension.json")).unwrap();
        let mut runtime = Runtime::new(
            manifest,
            PathBuf::new(),
            true,
            Path::new("."),
            String::new(),
        );
        runtime.load_chat(path.clone());
        let context = egui::Context::default();
        let mut button = egui::Pos2::ZERO;
        let mut option = egui::Pos2::ZERO;
        for full in [true, false] {
            for phase in 0..7 {
                let pos = if phase <= 3 { button } else { option };
                let events = match phase {
                    2 | 5 => vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    3 | 6 => vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    _ => vec![],
                };
                let output = context.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(500.0, 400.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            button = task_access(&mut runtime, ui, false).rect.center();
                        });
                    },
                );
                if phase == 5 {
                    assert_eq!(
                        output.platform_output.cursor_icon,
                        egui::CursorIcon::PointingHand
                    );
                }
                if phase == 4 {
                    option = output
                        .shapes
                        .iter()
                        .find_map(|shape| {
                            if let egui::Shape::Text(text) = &shape.shape
                                && text.galley.text()
                                    == t(if full {
                                        "Full access"
                                    } else {
                                        "Request approval"
                                    })
                                && text.pos.y > 35.0
                            {
                                Some(text.pos + egui::vec2(10.0, 6.0))
                            } else {
                                None
                            }
                        })
                        .expect("menu choice must be visible");
                }
            }
            assert_eq!(runtime.tools.approve_task, full);
            let manifest =
                serde_json::from_str(include_str!("../../extensions/codex/extension.json"))
                    .unwrap();
            let mut restored = Runtime::new(
                manifest,
                PathBuf::new(),
                true,
                Path::new("."),
                String::new(),
            );
            restored.load_chat(path.clone());
            assert_eq!(restored.tools.approve_task, full);
            assert!(restored.tools.token.is_empty() && restored.tools.scope.is_empty());
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn intermediate_messages_have_one_combined_copy_footer() {
        let mut runtime = Runtime::new(
            serde_json::from_str(include_str!("../../extensions/codex/extension.json")).unwrap(),
            PathBuf::new(),
            true,
            Path::new("."),
            String::new(),
        );
        runtime.message("u1", "user", "Do not copy this prompt", false);
        for (id, text) in [
            ("a1", "First update"),
            ("a2", "Second update"),
            ("a3", "Final **answer**"),
        ] {
            runtime.message(id, "assistant", text, false);
        }
        runtime.message("u2", "user", "Next prompt", false);
        runtime.message("b1", "assistant", "Next answer", false);
        assert!(turn_response(&runtime.messages, 1).is_none());
        assert!(turn_response(&runtime.messages, 2).is_none());
        assert_eq!(
            turn_response(&runtime.messages, 3).as_deref(),
            Some("First update\n\nSecond update\n\nFinal **answer**")
        );
        assert_eq!(
            turn_response(&runtime.messages, 5).as_deref(),
            Some("Next answer")
        );
    }

    #[test]
    fn response_actions_render_once_next_to_their_own_turn() {
        let mut runtime = Runtime::new(
            serde_json::from_str(include_str!("../../extensions/codex/extension.json")).unwrap(),
            PathBuf::new(),
            true,
            Path::new("."),
            String::new(),
        );
        for (turn, answer, command) in [
            ("user-a", "Answer A", "command-a"),
            ("user-b", "Answer B", "command-b"),
        ] {
            runtime.message(turn, "user", "Question", false);
            runtime.message(answer, "assistant", answer, false);
            runtime.receive(json!({"method":"chat/completed","params":{"status":"completed"}}));
            runtime.tools.actions.push(session_tools::Action {
                turn: turn.into(),
                request: serde_json::Value::Null,
                tool: "xpeed_exec".into(),
                args: json!({"command":command}),
                target: "Test server".into(),
                state: "completed".into(),
                result: None,
            });
        }
        let ctx = egui::Context::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(420.0, 1400.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| messages_ui(ui, &mut runtime));
            },
        );
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        let position = |value: &str| texts.iter().position(|s| *s == value).unwrap();
        assert!(position("Answer A") < position("command-a"));
        assert!(position("command-a") < position("Answer B"));
        assert!(position("Answer B") < position("command-b"));
        assert_eq!(texts.iter().filter(|s| **s == "command-a").count(), 1);
        assert_eq!(texts.iter().filter(|s| **s == "command-b").count(), 1);
        assert_eq!(duration_text(197_000), "3m 17s");
    }

    #[test]
    fn sending_from_chat_list_starts_a_fresh_chat_and_preserves_previous_draft() {
        let manifest =
            serde_json::from_str(include_str!("../../extensions/codex/extension.json")).unwrap();
        let mut runtime = Runtime::new(
            manifest,
            PathBuf::new(),
            true,
            Path::new("."),
            String::new(),
        );
        runtime.message("old-user", "user", "Previous conversation", false);
        runtime.thread_id = Some("previous-thread".into());
        runtime.draft = "Unsent follow-up".into();
        runtime.home_draft = "Start a new task".into();
        runtime.chat_page = Page::Conversations;
        // Invalid selection must not discard either draft or change the open thread.
        send_new(&mut runtime, &BTreeMap::new());
        assert_eq!(runtime.thread_id.as_deref(), Some("previous-thread"));
        assert_eq!(runtime.home_draft, "Start a new task");
        let targets = BTreeMap::from([(
            1,
            session_tools::Target {
                id: 1,
                incarnation: std::time::Instant::now(),
                name: "Test".into(),
                cwd: ".".into(),
                connected: true,
                shell: crate::terminal::Profile::default_shell(),
                ssh: None,
                password: None,
                hosts: None,
                console: None,
            },
        )]);
        runtime.tools.selected.insert(1);
        send_new(&mut runtime, &targets);
        assert_eq!(runtime.chat_page, Page::Chat);
        assert!(runtime.thread_id.is_none());
        assert!(runtime.home_draft.is_empty());
        assert_eq!(runtime.messages[0].text, "Start a new task");
        assert_eq!(runtime.archives[0].1[0].text, "Previous conversation");
        let key = &runtime.archives[0].0;
        assert_eq!(runtime.saved_threads[key], "previous-thread");
        assert_eq!(runtime.saved_drafts[key], "Unsent follow-up");
    }

    #[test]
    fn row_click_opens_chat_and_trash_click_only_deletes() {
        for delete in [false, true] {
            let context = egui::Context::default();
            let mut position = egui::Pos2::ZERO;
            let mut outcome = (false, false);
            for phase in 0..3 {
                let events = match phase {
                    1 => vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    2 => vec![egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    _ => vec![],
                };
                let _ = context.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(320.0, 100.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let row = egui::Rect::from_min_size(
                                ui.cursor().min,
                                egui::vec2(ui.available_width(), 34.0),
                            );
                            if phase == 0 {
                                position = egui::pos2(
                                    if delete {
                                        row.right() - 16.0
                                    } else {
                                        row.left() + 30.0
                                    },
                                    row.center().y,
                                );
                            }
                            outcome = chat_row(ui, "test-row", "Example conversation", true);
                        });
                    },
                );
            }
            assert_eq!(outcome, if delete { (false, true) } else { (true, false) });
        }
    }

    #[test]
    fn late_approval_executes_once_and_keeps_real_output_for_continuation() {
        let manifest =
            serde_json::from_str(include_str!("../../extensions/codex/extension.json")).unwrap();
        let mut runtime = Runtime::new(
            manifest,
            PathBuf::new(),
            true,
            Path::new("."),
            String::new(),
        );
        let cwd = std::env::temp_dir().to_string_lossy().into_owned();
        let targets = BTreeMap::from([(
            1,
            session_tools::Target {
                id: 1,
                incarnation: std::time::Instant::now(),
                name: "Approval fixture".into(),
                cwd: cwd.clone(),
                connected: true,
                shell: crate::terminal::Profile::default_shell(),
                ssh: None,
                password: None,
                hosts: None,
                console: None,
            },
        )]);
        runtime.tools.selected.insert(1);
        let scope = runtime.tools.begin(&targets).unwrap();
        runtime.tools.receive(json!({"id":"late-approval","params":{"scopeToken":scope["scopeToken"],"tool":"xpeed_exec","arguments":{"sessionId":1,"command":"echo xpeed-late-approval","cwd":cwd,"timeoutMs":5000}}}), &targets);
        runtime.receive(json!({"method":"chat/completed","params":{"status":"completed"}}));
        assert_eq!(runtime.tools.actions[0].state, "awaitingApproval");
        assert!(runtime.take_deferred_results().is_empty());
        runtime.tools.approve(0, &targets);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while runtime.tools.actions[0].state == "running" {
            runtime.tools.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let results = runtime.take_deferred_results();
        assert_eq!(results[0]["result"]["exitCode"], 0);
        assert!(
            results[0]["result"]["stdout"]
                .as_str()
                .unwrap()
                .contains("xpeed-late-approval")
        );
        assert!(runtime.take_deferred_results().is_empty());
        assert_eq!(runtime.tools.actions.len(), 1);
    }

    #[test]
    fn account_gates_navigation_and_composer_fits_a_narrow_panel() {
        let manifest =
            serde_json::from_str(include_str!("../../extensions/codex/extension.json")).unwrap();
        let mut runtime = Runtime::new(
            manifest,
            PathBuf::new(),
            true,
            Path::new("."),
            String::new(),
        );
        let ctx = egui::Context::default();
        let render = |runtime: &mut Runtime| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(300.0, 650.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        show(runtime, ui, &BTreeMap::new(), None);
                        assert!(ui.min_rect().is_finite());
                        assert!(
                            ui.min_rect().right() <= 300.0,
                            "Chat controls overflow horizontally: {:?}",
                            ui.min_rect()
                        );
                        assert!(
                            ui.min_rect().bottom() <= 650.0,
                            "Chat controls overflow the panel: {:?}",
                            ui.min_rect()
                        );
                    });
                },
            );
        };
        render(&mut runtime);
        assert_eq!(runtime.chat_page, Page::SignIn);
        runtime.ready = true;
        runtime.account = Some(json!({"type":"chatgpt"}));
        render(&mut runtime);
        assert_eq!(runtime.chat_page, Page::Welcome);
        runtime.chat_page = Page::Conversations;
        render(&mut runtime);
        assert_eq!(runtime.chat_page, Page::Conversations);
        runtime.chat_page = Page::Chat;
        runtime.draft = "Pending draft".into();
        render(&mut runtime);
        assert_eq!(runtime.draft, "Pending draft");
        runtime.tools.actions.push(session_tools::Action {
            turn: String::new(),
            request: json!("pending"),
            tool: "xpeed_exec".into(),
            args: json!({"command":"ls","cwd":"/root"}),
            target: "cloud · #1".into(),
            state: "awaitingApproval".into(),
            result: None,
        });
        runtime.busy = true;
        render(&mut runtime);
        runtime.receive(json!({"method":"chat/completed","params":{"status":"completed"}}));
        assert_eq!(runtime.tools.actions[0].state, "awaitingApproval");
        assert!(runtime.deferred_actions.contains(&0));
        assert!(runtime.take_deferred_results().is_empty());
        render(&mut runtime);
        runtime.tools.actions[0].state = "completed".into();
        runtime.tools.actions[0].result = Some(json!({"stdout":"file.txt","exitCode":0}));
        assert_eq!(
            runtime.take_deferred_results()[0]["result"]["stdout"],
            "file.txt"
        );
        assert!(runtime.take_deferred_results().is_empty());
        runtime.account = None;
        render(&mut runtime);
        assert_eq!(runtime.chat_page, Page::SignIn);
    }
}
