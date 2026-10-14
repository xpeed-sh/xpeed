use super::*;
use egui::{Event, Key, Modifiers, Pos2, RawInput, Rect};

#[test]
fn panel_control_svg_remains_square_at_different_display_scales() {
    let ctx = egui::Context::default();
    for scale in [1.0, 2.0, 1.25, 1.0] {
        ctx.set_pixels_per_point(scale);
        let output = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 34.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        small_icon_button(ui, Icon::PanelClose, Palette::default());
                    });
                });
            },
        );
        let mesh = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Mesh(mesh) = &shape.shape {
                    Some(mesh)
                } else {
                    None
                }
            })
            .unwrap();
        let mut bounds = Rect::NOTHING;
        for vertex in &mesh.vertices {
            bounds.extend_with(vertex.pos);
        }
        assert!((bounds.width() - bounds.height()).abs() < 0.05);
        assert!(bounds.width() > 17.0);
    }
}

#[test]
fn extension_menu_marks_the_active_tab_and_clears_when_hidden_or_changed() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    let view = Pane::ExtensionView(123);
    workspace::join_panel(&mut app.tree, Pane::ServerInfo, view);
    app.show(view);
    assert!(app.extension_active("example.monitor", &[123]));
    app.show(Pane::ServerInfo);
    assert!(!app.extension_active("example.monitor", &[123]));
    app.show(view);
    app.action(Action::Hide(view), &ctx);
    assert!(!app.extension_active("example.monitor", &[123]));
}

#[test]
fn extension_details_open_once_beside_sessions_without_restarting_them() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    app.action(Action::Local(Profile::default_shell()), &ctx);
    app.extensions.request_details("xpeed.codex");
    let key = app.extensions.details_requested.take().unwrap();
    app.open_extension_details(key);
    app.open_extension_details(key);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let terminal = workspace::find(&app.tree, Pane::Terminal(1)).unwrap();
    let detail = workspace::find(&app.tree, Pane::ExtensionDetails(key)).unwrap();
    assert_eq!(
        app.tree.tiles.parent_of(terminal),
        app.tree.tiles.parent_of(detail)
    );
    assert_eq!(
        app.tree
            .tiles
            .iter()
            .filter(|(_, tile)| **tile == Tile::Pane(Pane::ExtensionDetails(key)))
            .count(),
        1
    );
    assert_eq!(app.sessions.len(), 1);
    assert_eq!(app.focused, Some(1));
    app.tree.tiles.remove(detail);
    frame(&mut app, &ctx, vec![]);
    assert!(app.sessions.contains_key(&1));
    assert!(workspace::find(&app.tree, Pane::Terminal(1)).is_some());
}

#[test]
fn grouped_panel_expands_restores_and_closes_without_stopping_terminal() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    app.action(Action::Local(Profile::default_shell()), &ctx);
    workspace::join_panel(&mut app.tree, Pane::ServerInfo, Pane::AppInfo);
    app.show(Pane::ServerInfo);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let id = workspace::find(&app.tree, Pane::ServerInfo).unwrap();
    let initial = app.tree.tiles.rect(id).unwrap();
    let original = serde_json::to_value(&app.tree).unwrap();
    app.action(Action::ZoomPanel(Pane::ServerInfo), &ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let expanded = app.tree.tiles.rect(id).unwrap();
    assert!(expanded.width() > initial.width() + 200.);
    assert!(app.sessions.contains_key(&1));
    let terminal = workspace::find(&app.tree, Pane::Terminal(1)).unwrap();
    assert!(!app.tree.tiles.is_visible(terminal));
    app.action(Action::ZoomPanel(Pane::AppInfo), &ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    assert!(app.zoom.is_none());
    assert!(app.tree.tiles.is_visible(terminal));
    assert_eq!(serde_json::to_value(&app.tree).unwrap(), original);
    app.action(Action::Hide(Pane::ServerInfo), &ctx);
    for pane in [Pane::AppInfo, Pane::ServerInfo] {
        assert!(
            !app.tree
                .tiles
                .is_visible(workspace::find(&app.tree, pane).unwrap())
        );
    }
    app.show(Pane::AppInfo);
    for pane in [Pane::AppInfo, Pane::ServerInfo] {
        assert!(
            app.tree
                .tiles
                .is_visible(workspace::find(&app.tree, pane).unwrap())
        );
    }
    assert_eq!(app.sessions.len(), 1);
    app.close(1);
}

#[test]
fn monitor_follows_authenticated_selection_without_starting_when_no_extension_is_visible() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    for host in ["first.example", "second.example"] {
        app.action(Action::Local(Profile::default_shell()), &ctx);
        let session = app.sessions.get_mut(&app.focused.unwrap()).unwrap();
        session.remote = Some(SshConfig {
            host: host.into(),
            ..Default::default()
        });
        session.auth_complete = true;
    }
    app.focused = Some(1);
    app.sync_monitor(&ctx);
    assert_eq!(app.monitor_target.as_ref().unwrap().1.host, "first.example");
    let generation = app.monitor_generation;
    app.focused = Some(2);
    app.sync_monitor(&ctx);
    assert_eq!(
        app.monitor_target.as_ref().unwrap().1.host,
        "second.example"
    );
    assert!(app.monitor_generation > generation);
    assert!(app.monitor.is_none());
    app.sessions.get_mut(&2).unwrap().exited = true;
    app.sync_monitor(&ctx);
    assert!(app.monitor_target.is_none());
    app.close(1);
    app.close(2);
}

#[test]
fn right_panels_open_independently_and_share_space_without_tabs() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    let visible = |app: &Xpeed, pane| {
        app.tree
            .tiles
            .is_visible(workspace::find(&app.tree, pane).unwrap())
    };
    assert!(workspace::find(&app.tree, Pane::Assistant).is_none());
    assert!(!visible(&app, Pane::AppInfo));
    app.action(Action::ToggleAI, &ctx);
    assert!(visible(&app, Pane::AppInfo));
    assert!(workspace::find(&app.tree, Pane::Assistant).is_none());
    app.action(Action::ToggleAI, &ctx);
    app.show(Pane::ServerInfo);
    app.action(Action::ToggleAI, &ctx);
    assert!(!visible(&app, Pane::ServerInfo));
    app.action(Action::ToggleAI, &ctx);
    assert!(visible(&app, Pane::ServerInfo));
    assert!(!visible(&app, Pane::AppInfo));
    app.show(Pane::AppInfo);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let rectangles: Vec<_> = [Pane::AppInfo, Pane::ServerInfo]
        .into_iter()
        .map(|pane| {
            let id = workspace::find(&app.tree, pane).unwrap();
            assert!(app.tree.tiles.is_visible(id));
            app.tree.tiles.rect(id).unwrap()
        })
        .collect();
    for (i, rect) in rectangles.iter().enumerate() {
        assert!(rect.height() > 40.0);
        for other in &rectangles[i + 1..] {
            assert!(!rect.intersects(*other));
        }
    }
    app.action(Action::ToggleAI, &ctx);
    frame(&mut app, &ctx, vec![]);
    for pane in [Pane::AppInfo, Pane::ServerInfo] {
        assert!(!visible(&app, pane));
    }
    app.action(Action::ToggleAI, &ctx);
    assert!(visible(&app, Pane::AppInfo));
    assert!(!visible(&app, Pane::ServerInfo));
    assert!(app.sessions.is_empty());
}

#[test]
fn command_preferences_are_changed_in_settings_and_apply_to_new_sessions() {
    let old: Settings = serde_json::from_str("{\"font_size\":16}").unwrap();
    assert!(old.follow_terminal && old.record_commands);
    assert!(!old.attach_terminal_to_ai);
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    app.action(Action::Local(Profile::default_shell()), &ctx);
    app.action(Action::Settings, &ctx);
    let find_text = |app: &mut Xpeed, text: &str| {
        for _ in 0..25 {
            let output = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 820.0))),
                    ..Default::default()
                },
                |ctx| app.frame(ctx),
            );
            for shape in output.shapes {
                if let egui::Shape::Text(shape) = shape.shape
                    && shape.galley.job.text == xpeed_i18n::t(text)
                {
                    return shape.pos + Vec2::new(5.0, 5.0);
                }
            }
        }
        panic!("Settings control not found: {text}");
    };
    let click = |app: &mut Xpeed, pos| {
        frame(app, &ctx, vec![Event::PointerMoved(pos)]);
        frame(
            app,
            &ctx,
            vec![Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }],
        );
        frame(
            app,
            &ctx,
            vec![Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
    };
    let heading = find_text(&mut app, "Comandos e historial");
    click(&mut app, heading);
    let record = find_text(&mut app, "Registrar comandos");
    click(&mut app, record);
    assert!(!app.settings.record_commands);
    assert!(!app.sessions[&1].record_commands);
    app.action(Action::Local(Profile::default_shell()), &ctx);
    assert!(!app.sessions[&2].record_commands);
    app.settings.follow_terminal = false;
    app.settings.attach_terminal_to_ai = true;
    app.save_settings(&ctx);
    assert!(app.extensions.wants_terminal_context());
    let restored: Settings =
        serde_json::from_slice(&serde_json::to_vec(&app.settings).unwrap()).unwrap();
    assert!(!restored.follow_terminal && !restored.record_commands);
    assert!(restored.attach_terminal_to_ai);
    app.close(1);
    app.close(2);
}

#[test]
fn settings_panel_preserves_sidebar_and_keeps_the_terminal_usable() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    app.action(
        Action::Local(if cfg!(windows) {
            Profile::Cmd
        } else {
            Profile::System
        }),
        &ctx,
    );
    frame(&mut app, &ctx, vec![]);
    let active = app.focused.unwrap();
    let connections = workspace::find(&app.sidebar_tree, Pane::Connections).unwrap();
    let parent = app.sidebar_tree.tiles.parent_of(connections);
    app.action(Action::Settings, &ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    assert!(app.sidebar_view == SidebarView::Settings);
    assert!(app.sidebar_tree.tiles.is_visible(connections));
    let terminal = workspace::find(&app.tree, Pane::Terminal(active)).unwrap();
    assert!(app.tree.tiles.rect(terminal).unwrap().left() >= 280.0);
    let deadline = Instant::now() + Duration::from_secs(20);
    while output(&app.sessions[&active]).trim().is_empty() && Instant::now() < deadline {
        frame(&mut app, &ctx, vec![]);
        std::thread::sleep(Duration::from_millis(30));
    }
    frame(
        &mut app,
        &ctx,
        vec![
            Event::Text("echo XPEED_SETTINGS_OK".into()),
            key(Key::Enter, Modifiers::NONE),
        ],
    );
    while output(&app.sessions[&active])
        .matches("XPEED_SETTINGS_OK")
        .count()
        < 2
        && Instant::now() < deadline
    {
        frame(&mut app, &ctx, vec![]);
        std::thread::sleep(Duration::from_millis(30));
    }
    assert!(
        output(&app.sessions[&active])
            .matches("XPEED_SETTINGS_OK")
            .count()
            >= 2
    );
    app.action(Action::ToggleSidebar, &ctx);
    frame(&mut app, &ctx, vec![]);
    assert!(app.sidebar_view == SidebarView::Sessions);
    assert_eq!(app.sidebar_tree.tiles.parent_of(connections), parent);
    assert!(app.sidebar_tree.tiles.is_visible(connections));
    assert_eq!(app.focused, Some(active));
    assert_eq!(app.sessions.len(), 1);
}

fn frame(app: &mut Xpeed, ctx: &egui::Context, events: Vec<Event>) {
    let _ = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 820.0))),
            events,
            ..Default::default()
        },
        |ctx| app.frame(ctx),
    );
}
fn output(session: &Session) -> String {
    session
        .term
        .lock()
        .unwrap()
        .grid()
        .display_iter()
        .map(|c| c.cell.c)
        .collect()
}
fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn transient_tiny_terminal_layout_does_not_resize_shell_and_disconnect_card_stays_at_bottom() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    app.action(Action::Local(Profile::default_shell()), &ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let id = app.focused.unwrap();
    let before = app.sessions[&id].size;
    let palette = app.palette();
    let _ = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(50.0, 40.0))),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                terminal_view(
                    ui,
                    app.sessions.get_mut(&id).unwrap(),
                    15.0,
                    palette,
                    false,
                    false,
                    "",
                );
            });
        },
    );
    assert_eq!(app.sessions[&id].size.cols, before.cols);
    assert_eq!(app.sessions[&id].size.rows, before.rows);
    let session = app.sessions.get_mut(&id).unwrap();
    session.exited = true;
    session.error = Some("disconnect test detail".into());
    let mut status_position = None;
    for _ in 0..4 {
        let output = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 820.0))),
                ..Default::default()
            },
            |ctx| app.frame(ctx),
        );
        for shape in output.shapes {
            if let egui::Shape::Text(text) = shape.shape
                && text.galley.text() == xpeed_i18n::t("Session disconnected")
            {
                status_position = Some(text.pos);
            }
        }
    }
    assert!(
        status_position.expect("disconnect status visible").y > 600.0,
        "status must be below console output"
    );
    assert!(app.sessions[&id].size.cols >= 12);
    app.close(id);
}

#[test]
fn terminal_keeps_focus_during_repeated_history_navigation_and_history_click_inserts_only() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    app.action(Action::Local(Profile::default_shell()), &ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let id = app.focused.unwrap();
    let widget = app.sessions[&id].input_id.unwrap();
    ctx.memory_mut(|m| m.request_focus(widget));
    for arrow in [
        Key::ArrowUp,
        Key::ArrowUp,
        Key::ArrowUp,
        Key::ArrowDown,
        Key::ArrowLeft,
        Key::ArrowRight,
    ] {
        frame(&mut app, &ctx, vec![key(arrow, Modifiers::NONE)]);
        frame(&mut app, &ctx, vec![]);
        assert_eq!(ctx.memory(|m| m.focused()), Some(widget));
    }
    let prefix = connection_key(&app.sources[&id]);
    app.history.push(
        format!("{prefix} :: previous-app-run"),
        "echo XPEED_CLICK_COMMAND".into(),
    );
    let mut pos = None;
    for _ in 0..3 {
        let output = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 820.0))),
                ..Default::default()
            },
            |ctx| app.frame(ctx),
        );
        for shape in output.shapes {
            if let egui::Shape::Text(text) = shape.shape
                && text.galley.text() == "echo XPEED_CLICK_COMMAND"
            {
                pos = Some(text.pos + Vec2::new(12.0, 6.0));
            }
        }
    }
    let pos = pos.expect("saved commands must appear without an opt-in setting");
    for pressed in [true, false] {
        frame(
            &mut app,
            &ctx,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    assert_eq!(ctx.memory(|m| m.focused()), Some(widget));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !output(&app.sessions[&id]).contains("XPEED_CLICK_COMMAND") && Instant::now() < deadline {
        frame(&mut app, &ctx, vec![]);
        std::thread::sleep(Duration::from_millis(30));
    }
    assert_eq!(
        output(&app.sessions[&id])
            .matches("XPEED_CLICK_COMMAND")
            .count(),
        1,
        "click must insert text without pressing Enter"
    );
    app.sessions
        .get_mut(&id)
        .unwrap()
        .commands
        .push("pending command at close".into());
    app.close(id);
    assert!(
        app.history
            .commands
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .flat_map(|(_, records)| records)
            .any(|record| record.text == "pending command at close")
    );
}

#[test]
fn nested_splits_route_the_keyboard_to_one_session_and_keep_local_panels_readable() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    app.action(Action::Local(Profile::default_shell()), &ctx);
    app.split(ContainerKind::Horizontal, &ctx);
    app.split(ContainerKind::Vertical, &ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    assert_eq!(app.sessions.len(), 3);
    let active = app.focused.unwrap();
    for session in app.sessions.values() {
        assert!(
            session.size.cols >= 16,
            "session {}: {} columns",
            session.id,
            session.size.cols
        );
        assert!(
            session.size.rows >= 6,
            "session {}: {} rows",
            session.id,
            session.size.rows
        );
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    while (if cfg!(windows) {
        !output(&app.sessions[&active]).contains('>')
    } else {
        output(&app.sessions[&active]).trim().is_empty()
    }) && Instant::now() < deadline
    {
        frame(&mut app, &ctx, vec![]);
        std::thread::sleep(Duration::from_millis(30));
    }
    frame(
        &mut app,
        &ctx,
        vec![
            Event::Text("echo XPEED_DOCK_OK".into()),
            key(Key::Enter, Modifiers::NONE),
        ],
    );
    while output(&app.sessions[&active])
        .matches("XPEED_DOCK_OK")
        .count()
        < 2
        && Instant::now() < deadline
    {
        frame(&mut app, &ctx, vec![]);
        std::thread::sleep(Duration::from_millis(30));
    }
    assert!(
        output(&app.sessions[&active])
            .matches("XPEED_DOCK_OK")
            .count()
            >= 2
    );
    let source = &app.sources[&active];
    let history_key = format!(
        "{} :: {}",
        connection_key(source),
        app.sessions[&active].history_key
    );
    assert!(
        app.history.commands[&history_key]
            .iter()
            .any(|c| c.text == "echo XPEED_DOCK_OK")
    );
    for (&id, session) in &app.sessions {
        if id != active {
            assert!(!app.history.commands.contains_key(&format!(
                "{} :: {}",
                connection_key(&app.sources[&id]),
                session.history_key
            )));
        }
    }
    for (&id, session) in &app.sessions {
        if id != active {
            assert!(!output(session).contains("XPEED_DOCK_OK"));
        }
    }
    app.show_palette = true;
    frame(
        &mut app,
        &ctx,
        vec![Event::Text("NEVER_SEND_THIS_TO_THE_SHELL".into())],
    );
    std::thread::sleep(Duration::from_millis(70));
    frame(&mut app, &ctx, vec![]);
    assert!(!output(&app.sessions[&active]).contains("NEVER_SEND_THIS_TO_THE_SHELL"));
    app.show_palette = false;
    app.action(Action::Preset(true), &ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let sidebar = workspace::find(&app.sidebar_tree, Pane::Connections).unwrap();
    let terminal = workspace::find(&app.tree, Pane::Terminal(active)).unwrap();
    assert!(
        app.sidebar_tree.tiles.rect(sidebar).unwrap().right()
            < app.tree.tiles.rect(terminal).unwrap().left()
    );
    assert_eq!(app.sessions.len(), 3);
    for id in app.sources.keys().copied().collect::<Vec<_>>() {
        app.close(id);
    }
    assert!(app.sessions.is_empty());
    assert!(workspace::find(&app.tree, Pane::Welcome).is_some());
}

#[test]
fn modes_keep_their_own_theme_and_new_tab_shortcuts_keep_sessions_independent() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    app.action(Action::Local(Profile::default_shell()), &ctx);
    let chord = Modifiers {
        ctrl: true,
        shift: true,
        ..Modifiers::NONE
    };
    frame(&mut app, &ctx, vec![key(Key::T, chord)]);
    assert_eq!(app.sessions.len(), 2);
    assert_eq!(app.focused, Some(2));
    frame(&mut app, &ctx, vec![key(Key::Tab, Modifiers::CTRL)]);
    assert_eq!(app.focused, Some(1));
    frame(&mut app, &ctx, vec![key(Key::W, chord)]);
    assert_eq!(app.sessions.len(), 1);
    app.settings.dark_theme = 2;
    app.settings.light_theme = 1;
    app.settings.mode = Mode::Light;
    app.save_settings(&ctx);
    frame(&mut app, &ctx, vec![]);
    assert_eq!(app.palette().name, "Paper");
    assert_eq!(
        app.sessions.values().next().unwrap().palette.terminal,
        app.palette().terminal
    );
    app.settings.mode = Mode::Dark;
    app.save_settings(&ctx);
    assert_eq!(app.palette().name, "Forest");
    let loaded: Settings =
        serde_json::from_str("{\"font_size\":16,\"accent\":1,\"sidebar\":true}").unwrap();
    assert_eq!(loaded.mode, Mode::Dark);
    assert_eq!(loaded.font_size, 16.0);
}

#[test]
fn options_views_share_one_host_without_moving_or_restarting_terminals() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    app.action(Action::Local(Profile::default_shell()), &ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let terminal = workspace::find(&app.tree, Pane::Terminal(1)).unwrap();
    let baseline = app.tree.tiles.rect(terminal).unwrap();
    assert!(workspace::find(&app.tree, Pane::Connections).is_none());
    assert!(workspace::find(&app.sidebar_tree, Pane::Extensions).is_none());
    app.show(Pane::Extensions);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    assert!(app.sidebar_view == SidebarView::Extensions);
    let extension_rect = app.tree.tiles.rect(terminal).unwrap();
    app.action(Action::Settings, &ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    assert!(app.sidebar_view == SidebarView::Settings);
    let settings_rect = app.tree.tiles.rect(terminal).unwrap();
    assert!((extension_rect.left() - baseline.left()).abs() < 2.0);
    assert!((settings_rect.left() - baseline.left()).abs() < 2.0);
    assert!((settings_rect.width() - baseline.width()).abs() < 2.0);
    app.action(Action::ToggleSidebar, &ctx);
    frame(&mut app, &ctx, vec![]);
    assert!(app.sidebar_view == SidebarView::Sessions);
    assert_eq!(app.sessions.len(), 1);
    assert_eq!(app.focused, Some(1));
    for pane in [
        Pane::Connections,
        Pane::Files,
        Pane::Commands,
        Pane::Activity,
    ] {
        assert!(workspace::find(&app.sidebar_tree, pane).is_some());
    }
}

#[test]
fn startup_has_home_without_processes_and_files_follow_the_remote_selection() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    frame(&mut app, &ctx, vec![]);
    assert!(app.sessions.is_empty());
    assert!(app.sources.is_empty());
    assert!(workspace::find(&app.tree, Pane::Welcome).is_some());
    let files = workspace::find(&app.sidebar_tree, Pane::Files).unwrap();
    assert!(!app.sidebar_tree.tiles.is_visible(files));
    app.action(Action::Local(Profile::default_shell()), &ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let terminal_tile = workspace::find(&app.tree, Pane::Terminal(1)).unwrap();
    let before = app.tree.tiles.rect(terminal_tile).unwrap().width();
    app.action(Action::ToggleAI, &ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    assert!(app.tree.tiles.rect(terminal_tile).unwrap().width() < before - 100.0);
    app.sync_files();
    assert!(!app.sidebar_tree.tiles.is_visible(files));
    // Simulate an active SSH session without contacting any external host.
    app.sessions.get_mut(&1).unwrap().remote = Some(SshConfig {
        host: "example.test".into(),
        ..Default::default()
    });
    app.sync_files();
    assert!(app.sidebar_tree.tiles.is_visible(files));
    app.action(Action::Local(Profile::default_shell()), &ctx);
    app.sync_files();
    assert!(!app.sidebar_tree.tiles.is_visible(files));
    app.close(1);
    app.close(2);
    assert!(workspace::find(&app.tree, Pane::Welcome).is_some());
}
#[test]
fn tab_hover_keeps_normal_cursor_and_free_bar_keeps_drag_cursor() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    app.action(Action::Local(Profile::default_shell()), &ctx);
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    let pane = workspace::find(&app.tree, Pane::Terminal(1)).unwrap();
    let parent = app.tree.tiles.parent_of(pane).unwrap();
    let rect = app.tree.tiles.rect(parent).unwrap();
    for (point, expected) in [
        (rect.min + Vec2::new(55.0, 17.0), egui::CursorIcon::Default),
        (
            rect.min + Vec2::new(rect.width() * 0.65, 17.0),
            egui::CursorIcon::Grab,
        ),
    ] {
        frame(&mut app, &ctx, vec![Event::PointerMoved(point)]);
        let output = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 820.0))),
                events: vec![Event::PointerMoved(point)],
                ..Default::default()
            },
            |ctx| app.frame(ctx),
        );
        assert_eq!(output.platform_output.cursor_icon, expected);
    }
}

#[test]
fn reconnect_keeps_tab_and_clears_status_after_authentication() {
    let ctx = egui::Context::default();
    let mut app = Xpeed::from_context(&ctx, true);
    app.action(Action::Local(Profile::default_shell()), &ctx);
    let id = app.focused.unwrap();
    let session = app.sessions.get_mut(&id).unwrap();
    session.unexpected_disconnect = true;
    session.exited = true;
    app.ephemeral = false;
    app.reconnect_sessions(&ctx);
    assert!(app.sources.contains_key(&id));
    assert_eq!(app.reconnects[&id].0, 0);
    let session = app.sessions.get_mut(&id).unwrap();
    session.unexpected_disconnect = false;
    session.exited = false;
    session.auth_complete = true;
    app.reconnect_sessions(&ctx);
    assert!(!app.reconnects.contains_key(&id));
    assert!(app.sessions[&id].error.is_none());
    app.ephemeral = true;
}
