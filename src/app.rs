use crate::{
    assistant::Assistant,
    connections::{Connection, ConnectionStore, SshConfig, Target},
    history::History,
    palette::{Mode, Palette},
    settings::Settings,
    sftp::RemoteExplorer,
    terminal::{Profile, Session},
    terminal_view::terminal_view,
    workspace::{self, Pane, Snapshot},
};
use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, RichText, Stroke, Vec2};
use egui_tiles::{
    Behavior, ContainerKind, EditAction, SimplificationOptions, TabState, Tile, TileId, Tiles,
    Tree, UiResponse,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::{Duration, Instant},
};

const PANEL_GAP: i8 = 5;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarView {
    Sessions,
    Extensions,
    Settings,
}

enum Action {
    Local(Profile),
    Open(Connection),
    Reconnect(u64),
    Focus(u64),
    Close(u64),
    Split(ContainerKind),
    Show(Pane),
    Hide(Pane),
    ZoomPanel(Pane),
    Preset(bool),
    ToggleSidebar,
    Settings,
    NewConnection,
    EditConnection(usize),
    DeleteConnection(usize),
    ForgetAccess(usize),
    Draft(&'static str),
    Search,
    ToggleAI,
    OpenExtension(String),
    Home,
    ClearCommands,
    InsertCommand(u64, String),
    Backup,
}

struct ConnectionEditor {
    name: String,
    group: String,
    remote: bool,
    selected: bool,
    ssh: SshConfig,
    shell: Profile,
    directory: String,
    save: bool,
    editing: Option<usize>,
    error: Option<String>,
}
impl Default for ConnectionEditor {
    fn default() -> Self {
        Self {
            name: String::new(),
            group: String::new(),
            remote: false,
            selected: false,
            ssh: SshConfig::default(),
            shell: Profile::default_shell(),
            directory: String::new(),
            save: false,
            editing: None,
            error: None,
        }
    }
}
impl ConnectionEditor {
    fn connection(&self) -> Connection {
        let target = if self.remote {
            Target::Ssh(self.ssh.clone())
        } else {
            Target::Local {
                shell: self.shell,
                directory: self.directory.clone(),
            }
        };
        let name = if self.name.trim().is_empty() {
            if self.remote {
                self.ssh.label()
            } else {
                self.shell.name().into()
            }
        } else {
            self.name.trim().into()
        };
        Connection { name, target }
    }
    fn from_saved(index: usize, connection: &Connection) -> Self {
        let mut editor = Self {
            name: connection.name.clone(),
            editing: Some(index),
            selected: true,
            save: true,
            ..Default::default()
        };
        match &connection.target {
            Target::Local { shell, directory } => {
                editor.remote = false;
                editor.shell = *shell;
                editor.directory = directory.clone();
            }
            Target::Ssh(config) => {
                editor.remote = true;
                editor.ssh = config.clone();
            }
        }
        editor
    }
}

pub struct Xpeed {
    window_size: crate::window_size::WindowSize,
    tree: Tree<Pane>,
    zoom: Option<(Pane, Vec<(Pane, bool)>)>,
    last_right_panel: Pane,
    sessions: BTreeMap<u64, Session>,
    reconnects: BTreeMap<u64, (u32, Instant)>,
    productivity: crate::productivity::State,
    updates: crate::updates::Updates,
    sources: BTreeMap<u64, Connection>,
    focused: Option<u64>,
    next_id: u64,
    settings: Settings,
    connections: ConnectionStore,
    remote_files: BTreeMap<u64, RemoteExplorer>,
    access_candidates: BTreeMap<u64, crate::credentials::Secret>,
    access_handled: BTreeSet<u64>,
    access_prompt: Option<u64>,
    assistant: Assistant,
    extensions: crate::extensions::Extensions,
    monitor: Option<crate::server_monitor::Worker>,
    monitor_target: Option<(u64, SshConfig)>,
    monitor_interest: bool,
    monitor_generation: u64,
    monitor_retry: Instant,
    history: History,
    files_focus: Option<u64>,
    activity: Vec<String>,
    query: String,
    search: String,
    show_search: bool,
    search_result: (usize, usize),
    sidebar_view: SidebarView,
    sidebar_tree: Tree<Pane>,
    show_palette: bool,
    palette_query: String,
    palette_focus: bool,
    editor: Option<ConnectionEditor>,
    notice: Option<String>,
    cwd: PathBuf,
    capture: Option<PathBuf>,
    capture_requested: bool,
    created: Instant,
    frame_cost_ms: f32,
    layout_dirty: bool,
    edited_at: Instant,
    ephemeral: bool,
}

impl Xpeed {
    fn restore_zoom(&mut self) {
        if let Some((pane, visibility)) = self.zoom.take() {
            let tree = if workspace::find(&self.sidebar_tree, pane).is_some() {
                &mut self.sidebar_tree
            } else {
                &mut self.tree
            };
            for (pane, visible) in visibility {
                if let Some(id) = workspace::find(tree, pane) {
                    tree.set_visible(id, visible);
                }
            }
            workspace::refresh_visibility(tree);
        }
    }
    fn apply_zoom(&mut self) {
        if let Some((pane, _)) = &self.zoom {
            let tree = if workspace::find(&self.sidebar_tree, *pane).is_some() {
                &mut self.sidebar_tree
            } else {
                &mut self.tree
            };
            let members = workspace::panel_members(tree, *pane);
            let panes: Vec<_> = tree
                .tiles
                .iter()
                .filter_map(|(&id, tile)| {
                    if let Tile::Pane(pane) = tile {
                        Some((id, members.contains(pane)))
                    } else {
                        None
                    }
                })
                .collect();
            for (id, visible) in panes {
                tree.set_visible(id, visible);
            }
            workspace::refresh_visibility(tree);
        }
    }
    fn sync_monitor(&mut self, ctx: &egui::Context) {
        let interested = self.extensions.take_metrics_interest();
        let target = self.focused.and_then(|id| {
            let session = self.sessions.get(&id)?;
            (session.auth_complete && !session.exited)
                .then(|| session.remote.clone().map(|config| (id, config)))
                .flatten()
        });
        if target != self.monitor_target || interested != self.monitor_interest {
            self.monitor = None;
            self.monitor_generation += 1;
            self.monitor_target = target.clone();
            self.monitor_interest = interested;
            self.monitor_retry = Instant::now();
        }
        let context = if let Some((id, config)) = &target {
            serde_json::json!({"id":self.monitor_generation,"sessionId":id,"name":self.sources.get(id).map(|source| &source.name),"host":config.host,"port":config.port,"user":config.user,"observing":interested})
        } else {
            serde_json::json!({"id":null,"observing":false,"name":xpeed_i18n::t("Selecciona una sesión SSH autenticada")})
        };
        self.extensions.server_context(context);
        if interested && let Some((id, config)) = &target {
            if self.monitor.is_none() && Instant::now() >= self.monitor_retry {
                let password = self
                    .remote_files
                    .get(id)
                    .and_then(RemoteExplorer::monitoring_access)
                    .or_else(|| crate::credentials::load(config));
                self.monitor = Some(crate::server_monitor::Worker::start(
                    config.clone(),
                    password,
                    ctx.clone(),
                    None,
                ));
            }
            if let Some(result) = self
                .monitor
                .as_ref()
                .and_then(crate::server_monitor::Worker::poll)
            {
                if result.is_err() {
                    self.monitor = None;
                    self.monitor_retry = Instant::now() + Duration::from_secs(10);
                }
                self.extensions
                    .server_sample(self.monitor_generation, result);
            }
        }
    }
    fn sync_extension_views(&mut self) {
        use crate::extensions::ViewLocation;
        let views = self.extensions.views();
        let ids: BTreeSet<_> = views.iter().map(|(id, _)| *id).collect();
        for tree in [&mut self.tree, &mut self.sidebar_tree] {
            let stale: Vec<_> = tree
                .tiles
                .iter()
                .filter_map(|(&tile, pane)| {
                    matches!(pane, Tile::Pane(Pane::ExtensionView(id)) if !ids.contains(id))
                        .then_some(tile)
                })
                .collect();
            for tile in stale {
                tree.remove_recursively(tile);
            }
        }
        for (id, view) in views {
            if matches!(
                view.location,
                ViewLocation::Settings | ViewLocation::Extensions
            ) {
                continue;
            }
            let pane = Pane::ExtensionView(id);
            if view.location == ViewLocation::Server {
                workspace::join_panel(&mut self.tree, Pane::ServerInfo, pane);
                continue;
            }
            let peer = self
                .extensions
                .panel_peers(id)
                .into_iter()
                .filter(|peer| *peer != id)
                .map(Pane::ExtensionView)
                .find(|pane| {
                    workspace::find(&self.tree, *pane).is_some()
                        || workspace::find(&self.sidebar_tree, *pane).is_some()
                });
            if let Some(peer) = peer {
                let tree = if workspace::find(&self.sidebar_tree, peer).is_some() {
                    &mut self.sidebar_tree
                } else {
                    &mut self.tree
                };
                workspace::join_panel(tree, peer, pane);
                continue;
            }
            if workspace::find(&self.tree, pane).is_some()
                || workspace::find(&self.sidebar_tree, pane).is_some()
            {
                continue;
            }
            let (tree, anchor) = match view.location {
                ViewLocation::Left | ViewLocation::LeftBelow => {
                    (&mut self.sidebar_tree, Pane::Connections)
                }
                ViewLocation::Files | ViewLocation::FilesBelow => {
                    (&mut self.sidebar_tree, Pane::Files)
                }
                ViewLocation::Commands | ViewLocation::CommandsBelow => {
                    (&mut self.sidebar_tree, Pane::Commands)
                }
                ViewLocation::Activity => (&mut self.sidebar_tree, Pane::Activity),
                ViewLocation::Right | ViewLocation::RightTab => (&mut self.tree, Pane::AppInfo),
                ViewLocation::Settings | ViewLocation::Extensions | ViewLocation::Server => {
                    unreachable!()
                }
                ViewLocation::Center => {
                    let anchor = self.focused.map_or(Pane::Welcome, Pane::Terminal);
                    (&mut self.tree, anchor)
                }
            };
            let visible = workspace::find(tree, anchor).is_some_and(|id| tree.tiles.is_visible(id));
            if matches!(
                view.location,
                ViewLocation::Left
                    | ViewLocation::LeftBelow
                    | ViewLocation::CommandsBelow
                    | ViewLocation::FilesBelow
                    | ViewLocation::Right
            ) {
                workspace::add_panel(tree, anchor, pane);
            } else {
                workspace::add_view(tree, anchor, pane);
            }
            if let Some(id) = workspace::find(tree, pane) {
                tree.set_visible(
                    id,
                    visible
                        && !matches!(
                            view.location,
                            ViewLocation::Left
                                | ViewLocation::LeftBelow
                                | ViewLocation::CommandsBelow
                                | ViewLocation::FilesBelow
                                | ViewLocation::Right
                        ),
                );
            }
        }
    }
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::from_context(&cc.egui_ctx, false)
    }
    fn from_context(ctx: &egui::Context, testing: bool) -> Self {
        let args: Vec<String> = std::env::args().collect();
        let capture = args
            .windows(2)
            .find(|pair| pair[0] == "--capture")
            .map(|pair| PathBuf::from(&pair[1]));
        let ephemeral = testing || capture.is_some() || args.iter().any(|a| a == "--window-test");
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut settings = if ephemeral {
            Settings::default()
        } else {
            Settings::load()
        };
        if args.iter().any(|a| a == "--light") {
            settings.mode = Mode::Light;
        }
        if let Some(pair) = args.windows(2).find(|p| p[0] == "--theme")
            && let Ok(index) = pair[1].parse::<usize>()
        {
            if settings.mode == Mode::Dark {
                settings.dark_theme = index.min(2);
            } else {
                settings.light_theme = index.min(2);
            }
        }
        if ephemeral && let Some(pair) = args.windows(2).find(|p| p[0] == "--language") {
            settings.language = xpeed_i18n::Language::from_code(&pair[1]);
        }
        xpeed_i18n::set_language(settings.language);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            xpeed_i18n::t("Xpeed — Conexiones y terminales").into(),
        ));
        settings.palette().apply(ctx);
        let mut app = Self {
            window_size: crate::window_size::WindowSize::default(),
            tree: workspace::default_layout(&[], false),
            zoom: None,
            last_right_panel: Pane::AppInfo,
            sessions: BTreeMap::new(),
            reconnects: BTreeMap::new(),
            productivity: Default::default(),
            updates: Default::default(),
            sources: BTreeMap::new(),
            focused: None,
            next_id: 1,
            settings,
            connections: if ephemeral {
                ConnectionStore::default()
            } else {
                ConnectionStore::load()
            },
            remote_files: BTreeMap::new(),
            access_candidates: BTreeMap::new(),
            access_handled: BTreeSet::new(),
            access_prompt: None,
            assistant: Assistant::default(),
            extensions: crate::extensions::Extensions::new(&cwd, ephemeral),
            monitor: None,
            monitor_target: None,
            monitor_interest: false,
            monitor_generation: 0,
            monitor_retry: Instant::now(),
            history: if ephemeral {
                History::default()
            } else {
                History::load()
            },
            files_focus: None,
            activity: Vec::new(),
            query: String::new(),
            search: String::new(),
            show_search: false,
            search_result: (0, 0),
            sidebar_view: if args.iter().any(|a| a == "--show-settings") {
                SidebarView::Settings
            } else {
                SidebarView::Sessions
            },
            sidebar_tree: workspace::sidebar_layout(),
            show_palette: false,
            palette_query: String::new(),
            palette_focus: false,
            editor: args
                .iter()
                .any(|a| a == "--show-connection")
                .then(ConnectionEditor::default),
            notice: None,
            cwd,
            capture,
            capture_requested: false,
            created: Instant::now(),
            frame_cost_ms: 0.0,
            layout_dirty: false,
            edited_at: Instant::now(),
            ephemeral,
        };
        if !ephemeral
            && !args.iter().any(|a| a == "--fresh-layout")
            && let Some(snapshot) = Snapshot::load()
            && workspace::find(&snapshot.tree, Pane::AppInfo).is_some()
        {
            app.tree = snapshot.tree;
            if let Some(sidebar_tree) = snapshot.sidebar_tree {
                app.sidebar_tree = sidebar_tree;
            }
            let terminals: Vec<_> = app
                .tree
                .tiles
                .iter()
                .filter_map(|(&tile, pane)| {
                    matches!(pane, Tile::Pane(Pane::Terminal(_))).then_some(tile)
                })
                .collect();
            for (index, tile) in terminals.into_iter().enumerate() {
                if index == 0 {
                    app.tree.tiles.insert(tile, Tile::Pane(Pane::Welcome));
                } else {
                    app.tree.remove_recursively(tile);
                }
            }
            app.tree
                .make_active(|_, tile| *tile == Tile::Pane(Pane::Welcome));
        }
        workspace::detach_sidebar(&mut app.tree);
        workspace::ensure_right_views(&mut app.tree);
        app.sync_extension_views();
        app.hide_right_column();
        if args.iter().any(|a| a == "--show-extensions") {
            app.show(Pane::Extensions);
        }
        if args.iter().any(|a| a == "--terminal" || a == "--split") {
            app.open(
                local_connection(Profile::default_shell(), &app.cwd),
                None,
                ctx,
            );
        }
        if args.iter().any(|a| a == "--split") {
            app.split(ContainerKind::Horizontal, ctx);
        }
        if app.capture.is_some() && args.iter().any(|a| a == "--demo-files") {
            workspace::add_terminal(&mut app.tree, 9000, None, None);
            app.focused = Some(9000);
            let mut remote = RemoteExplorer::preview();
            remote.loading = args.iter().any(|a| a == "--files-loading");
            remote.browser.grid = args.iter().any(|a| a == "--grid");
            remote
                .browser
                .selected
                .insert("/doolpool.com/.well-known".into(), true);
            app.remote_files.insert(9000, remote);
            app.sources.insert(
                9000,
                Connection {
                    name: "doolpool.com".into(),
                    target: Target::Ssh(SshConfig::default()),
                },
            );
        }
        app.apply_preferences();
        if app.capture.is_some() && args.iter().any(|a| a == "--demo-workspace") {
            let config = SshConfig {
                host: "192.0.2.10".into(),
                user: "developer".into(),
                ..Default::default()
            };
            let connection = Connection {
                name: "demo-server".into(),
                target: Target::Ssh(config.clone()),
            };
            if let Ok(session) = Session::screenshot_preview(config) {
                app.sessions.insert(9000, session);
                app.sources.insert(9000, connection.clone());
                app.connections.saved.push(connection);
                app.focused = Some(9000);
                workspace::add_terminal(&mut app.tree, 9000, None, None);
                app.tree
                    .make_active(|_, tile| *tile == Tile::Pane(Pane::Terminal(9000)));
                let remote = RemoteExplorer::workspace_preview();
                app.remote_files.insert(9000, remote);
                app.sidebar_tree
                    .make_active(|_, tile| *tile == Tile::Pane(Pane::Files));
                for command in ["ls -la", "git status --short", "pwd"] {
                    app.history.push(
                        format!("{} :: preview", connection_key(&app.sources[&9000])),
                        command.into(),
                    );
                }
                app.apply_preferences();
            }
        }
        if app.capture.is_some() && args.iter().any(|a| a == "--demo-panels") {
            app.show(Pane::AppInfo);
            app.show(Pane::ServerInfo);
        }
        if app.capture.is_some()
            && let Some(pair) = args.windows(2).find(|pair| pair[0] == "--demo-monitor")
        {
            if let Err(error) = app
                .extensions
                .preview_monitor(std::path::Path::new(&pair[1]))
            {
                app.notice = Some(error);
            }
            app.sync_extension_views();
            if let Some((id, _)) = app
                .extensions
                .views()
                .into_iter()
                .find(|(_, view)| view.id == "monitor")
            {
                app.show(Pane::ExtensionView(id));
            }
        }
        if app.capture.is_some() && args.iter().any(|a| a == "--demo-about") {
            app.show(Pane::AppInfo);
            app.action(Action::Hide(Pane::Assistant), ctx);
        }
        if app.capture.is_some()
            && let Some(pair) = args.windows(2).find(|pair| pair[0] == "--demo-extension")
        {
            app.extensions.request_details(&pair[1]);
            app.show(Pane::Extensions);
            app.action(Action::Hide(Pane::Assistant), ctx);
            app.action(Action::Hide(Pane::ServerInfo), ctx);
        }
        app.log(xpeed_i18n::t("Espacio de trabajo listo").into());
        if app.capture.is_some() && args.iter().any(|a| a == "--preview-codex") {
            if let Err(error) = app.extensions.preview_codex(ctx, app.focused) {
                app.log(error);
            }
            app.sync_extension_views();
            if let Some(key) = app.extensions.selected_chat_view() {
                app.show(Pane::ExtensionView(key));
            }
        }

        if app.capture.is_some() && args.iter().any(|arg| arg == "--demo-disconnect") {
            app.open(
                local_connection(Profile::default_shell(), &app.cwd),
                None,
                ctx,
            );
            if let Some(session) = app.focused.and_then(|id| app.sessions.get_mut(&id)) {
                session.send(b"exit\r".to_vec());
                if let Ok(mut term) = session.term.lock() {
                    let mut parser: alacritty_terminal::vte::ansi::Processor =
                        alacritty_terminal::vte::ansi::Processor::new();
                    parser.advance(&mut *term, b"Last login: demonstration\r\nroot@demo:~# ls\r\nREADME.md  src\r\nConnection closed.\r\n");
                }
                session.exited = true;
                session.error =
                    Some("Demo: connection closed. No remote server was contacted.".into());
            }
        }
        if app.capture.is_some() && args.iter().any(|a| a == "--demo-extension-manager") {
            if let Err(error) = app.extensions.preview_codex(ctx, None) {
                app.log(error);
            }
            app.sidebar_view = SidebarView::Extensions;
            if let Some(pair) = args
                .windows(2)
                .find(|pair| pair[0] == "--demo-installed-details")
            {
                app.extensions.request_details(&pair[1]);
            }
        }
        app
    }
    fn sync_files(&mut self) {
        if self.ephemeral && self.focused == Some(9000) && self.remote_files.contains_key(&9000) {
            if let Some(tile) = workspace::find(&self.sidebar_tree, Pane::Files) {
                self.sidebar_tree.set_visible(tile, true);
            }
            self.sidebar_tree
                .make_active(|_, tile| *tile == Tile::Pane(Pane::Files));
            return;
        }
        let active = self.focused.filter(|id| {
            self.sessions
                .get(id)
                .is_some_and(|s| s.remote.is_some() && !s.exited)
        });
        let navigation_open = [Pane::Connections, Pane::Files].into_iter().any(|pane| {
            workspace::find(&self.sidebar_tree, pane)
                .is_some_and(|id| self.sidebar_tree.tiles.is_visible(id))
        });
        if let Some(tile) = workspace::find(&self.sidebar_tree, Pane::Files) {
            self.sidebar_tree.set_visible(
                tile,
                active.is_some() && self.settings.sidebar && navigation_open,
            );
            if active.is_some() && active != self.files_focus {
                self.sidebar_tree
                    .make_active(|_, tile| *tile == Tile::Pane(Pane::Files));
            }
        }
        self.files_focus = active;
        workspace::refresh_visibility(&mut self.sidebar_tree);
        workspace::refresh_visibility(&mut self.tree);
    }
    fn palette(&self) -> Palette {
        self.settings.palette()
    }
    fn window_chrome(&self, ctx: &egui::Context) {
        let rect = ctx.content_rect();
        let p = self.palette();
        ctx.layer_painter(egui::LayerId::background()).rect(
            rect.shrink(0.5),
            10,
            p.bg,
            Stroke::new(1.0_f32, p.border),
            egui::StrokeKind::Inside,
        );
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        if let Some(pos) = ctx.pointer_hover_pos() {
            let left = pos.x < rect.left() + 5.0;
            let right = pos.x > rect.right() - 5.0;
            let top = pos.y < rect.top() + 5.0;
            let bottom = pos.y > rect.bottom() - 5.0;
            use egui::{CursorIcon as C, ResizeDirection as D};
            let edge = match (left, right, top, bottom) {
                (true, _, true, _) => Some((D::NorthWest, C::ResizeNwSe)),
                (_, true, _, true) => Some((D::SouthEast, C::ResizeNwSe)),
                (_, true, true, _) => Some((D::NorthEast, C::ResizeNeSw)),
                (true, _, _, true) => Some((D::SouthWest, C::ResizeNeSw)),
                (true, _, _, _) => Some((D::West, C::ResizeHorizontal)),
                (_, true, _, _) => Some((D::East, C::ResizeHorizontal)),
                (_, _, true, _) => Some((D::North, C::ResizeVertical)),
                (_, _, _, true) => Some((D::South, C::ResizeVertical)),
                _ => None,
            };
            if let Some((direction, cursor)) = edge {
                ctx.set_cursor_icon(cursor);
                if ctx.input(|i| i.pointer.primary_pressed()) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                }
            }
        }
    }
    fn log(&mut self, text: String) {
        let seconds = self.created.elapsed().as_secs();
        if self
            .activity
            .last()
            .is_some_and(|line| line.ends_with(&text))
        {
            return;
        }
        self.activity
            .push(format!("{:02}:{:02}  {text}", seconds / 60, seconds % 60));
        if self.activity.len() > 100 {
            self.activity.remove(0);
        }
    }
    fn changed(&mut self) {
        self.layout_dirty = true;
        self.edited_at = Instant::now();
    }
    fn start_existing(
        &mut self,
        id: u64,
        connection: &Connection,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        connection.validate()?;
        let mut session = match &connection.target {
            Target::Local { shell, directory } => Session::spawn(
                id,
                *shell,
                if directory.is_empty() {
                    self.cwd.clone()
                } else {
                    PathBuf::from(directory)
                },
                ctx.clone(),
            )?,
            Target::Ssh(config) => {
                Session::spawn_ssh(id, config.clone(), self.cwd.clone(), ctx.clone())?
            }
        };
        session.palette = self.palette();
        session.record_commands = self.settings.record_commands;
        if !self.ephemeral
            && let Target::Ssh(config) = &connection.target
        {
            session.saved_password = crate::credentials::load(config);
        }
        self.sessions.insert(id, session);
        if let Target::Ssh(config) = &connection.target {
            self.remote_files
                .insert(id, RemoteExplorer::new(config.clone()));
        }
        self.apply_preferences();
        self.files_focus = None;
        self.focused = Some(id);
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        self.log(xpeed_i18n::format(
            "Abierta: {}",
            &[("0", connection.name.to_string())],
        ));
        Ok(())
    }
    fn open(&mut self, connection: Connection, split: Option<ContainerKind>, ctx: &egui::Context) {
        self.restore_zoom();
        let id = self.next_id;
        self.next_id += 1;
        let previous = self.focused;
        match self.start_existing(id, &connection, ctx) {
            Ok(()) => {
                self.connections.recent.retain(|c| {
                    c.name != connection.name || c.description() != connection.description()
                });
                self.connections.recent.insert(0, connection.clone());
                self.connections.recent.truncate(10);
                if !self.ephemeral
                    && let Err(error) = self.connections.save()
                {
                    self.notice = Some(error);
                }
                self.sources.insert(id, connection);
                workspace::add_terminal(&mut self.tree, id, previous, split);
                self.changed();
                self.show_palette = false;
            }
            Err(error) => self.notice = Some(error),
        }
    }
    fn split(&mut self, kind: ContainerKind, ctx: &egui::Context) {
        let connection = self
            .focused
            .and_then(|id| self.sources.get(&id).cloned())
            .unwrap_or_else(|| local_connection(Profile::default_shell(), &self.cwd));
        self.open(connection, Some(kind), ctx);
    }
    fn flush_commands(&mut self) {
        let mut history_changed = false;
        for (&id, session) in &mut self.sessions {
            if let Some(source) = self.sources.get(&id) {
                for command in session.take_recorded_commands() {
                    self.history.push(
                        format!("{} :: {}", connection_key(source), session.history_key),
                        command,
                    );
                    history_changed = true;
                }
            }
        }
        if history_changed
            && !self.ephemeral
            && let Err(error) = self.history.save()
        {
            self.notice = Some(error);
        }
    }
    fn close(&mut self, id: u64) {
        self.flush_commands();
        self.reconnects.remove(&id);
        if let Some(tile) = workspace::find(&self.tree, Pane::Terminal(id)) {
            if self.sources.len() == 1 && workspace::find(&self.tree, Pane::Welcome).is_none() {
                self.tree.tiles.insert(tile, Tile::Pane(Pane::Welcome));
            } else {
                self.tree.remove_recursively(tile);
            }
        }
        self.sessions.remove(&id);
        self.remote_files.remove(&id);
        self.access_candidates.remove(&id);
        self.access_handled.remove(&id);
        if self.access_prompt == Some(id) {
            self.access_prompt = None;
        }
        if let Some(source) = self.sources.remove(&id) {
            self.log(xpeed_i18n::format(
                "Cerrada: {}",
                &[("0", source.name.to_string())],
            ));
        }
        if self.focused == Some(id) {
            self.focused = self.sources.keys().next_back().copied();
        }
        if self.sources.is_empty() {
            if workspace::find(&self.tree, Pane::Welcome).is_none() {
                self.tree = workspace::default_layout(&[], false);
            }
            self.tree
                .make_active(|_, tile| *tile == Tile::Pane(Pane::Welcome));
        } else if let Some(id) = self.focused {
            self.tree
                .make_active(|_, tile| *tile == Tile::Pane(Pane::Terminal(id)));
        }
        self.changed();
    }
    fn open_extension_details(&mut self, key: u64) {
        self.restore_zoom();
        let pane = Pane::ExtensionDetails(key);
        let anchor = self
            .focused
            .map(Pane::Terminal)
            .filter(|pane| workspace::find(&self.tree, *pane).is_some())
            .unwrap_or(Pane::Welcome);
        workspace::add_view(&mut self.tree, anchor, pane);
        self.show(pane);
    }
    fn right_panel_panes(&self) -> Vec<Pane> {
        use crate::extensions::ViewLocation;
        let mut panes = vec![Pane::AppInfo, Pane::ServerInfo];
        panes.extend(
            self.extensions
                .views()
                .into_iter()
                .filter_map(|(id, view)| {
                    matches!(
                        view.location,
                        ViewLocation::Right | ViewLocation::RightTab | ViewLocation::Server
                    )
                    .then_some(Pane::ExtensionView(id))
                }),
        );
        panes
    }
    fn extension_active(&self, id: &str, views: &[u64]) -> bool {
        let active = |tree: &Tree<Pane>, pane| {
            workspace::find(tree, pane).is_some_and(|tile| tree.active_tiles().contains(&tile))
        };
        if self.extensions.chat_selected(id) && active(&self.tree, Pane::Assistant) {
            return true;
        }
        views.iter().any(|key| {
            let pane = Pane::ExtensionView(*key);
            active(&self.tree, pane)
                || (self.settings.sidebar
                    && self.sidebar_view == SidebarView::Sessions
                    && active(&self.sidebar_tree, pane))
                || (self.settings.sidebar
                    && self.extensions.views().iter().any(|(view_key, view)| {
                        *view_key == *key
                            && matches!(
                                (view.location, self.sidebar_view),
                                (
                                    crate::extensions::ViewLocation::Settings,
                                    SidebarView::Settings
                                ) | (
                                    crate::extensions::ViewLocation::Extensions,
                                    SidebarView::Extensions
                                )
                            )
                    }))
        })
    }
    fn hide_right_column(&mut self) {
        self.restore_zoom();
        for pane in self.right_panel_panes() {
            for member in workspace::panel_members(&self.tree, pane) {
                if let Some(tile) = workspace::find(&self.tree, member) {
                    self.tree.set_visible(tile, false);
                }
            }
        }
        workspace::refresh_visibility(&mut self.tree);
    }
    fn show(&mut self, pane: Pane) {
        self.restore_zoom();
        if self.right_panel_panes().contains(&pane) {
            self.last_right_panel = pane;
        }
        if let Pane::ExtensionView(id) = pane
            && let Some((_, view)) = self
                .extensions
                .views()
                .into_iter()
                .find(|(key, _)| *key == id)
            && matches!(
                view.location,
                crate::extensions::ViewLocation::Settings
                    | crate::extensions::ViewLocation::Extensions
            )
        {
            self.settings.sidebar = true;
            self.sidebar_view = if view.location == crate::extensions::ViewLocation::Settings {
                SidebarView::Settings
            } else {
                SidebarView::Extensions
            };
            self.changed();
            return;
        }
        let sidebar = matches!(
            pane,
            Pane::Connections | Pane::Files | Pane::Commands | Pane::Activity | Pane::Extensions
        ) || matches!(pane, Pane::ExtensionView(_))
            && workspace::find(&self.sidebar_tree, pane).is_some();
        if sidebar {
            self.settings.sidebar = true;
            self.sidebar_view = if pane == Pane::Extensions {
                SidebarView::Extensions
            } else {
                SidebarView::Sessions
            };
        }
        let tree = if sidebar {
            &mut self.sidebar_tree
        } else {
            &mut self.tree
        };
        if let Some(id) = workspace::find(tree, pane) {
            for member in workspace::panel_members(tree, pane) {
                if let Some(tile) = workspace::find(tree, member) {
                    tree.set_visible(tile, true);
                }
            }
            tree.set_visible(id, true);
            tree.make_active(|_, tile| *tile == Tile::Pane(pane));
        }
        workspace::refresh_visibility(tree);
        self.changed();
        self.log(xpeed_i18n::format(
            "Panel abierto: {}",
            &[(
                "0",
                (match pane {
                    Pane::Connections => xpeed_i18n::t("Conexiones"),
                    Pane::Files => xpeed_i18n::t("Archivos remotos"),
                    Pane::Commands => xpeed_i18n::t("Comandos"),
                    Pane::Activity => xpeed_i18n::t("Actividad"),
                    Pane::Assistant => xpeed_i18n::t("IA"),
                    Pane::AppInfo => "Xpeed",
                    Pane::ServerInfo => xpeed_i18n::t("Servidor"),
                    Pane::ExtensionView(_) => xpeed_i18n::t("Vista de extensión"),
                    Pane::Extensions => xpeed_i18n::t("Extensiones"),
                    _ => xpeed_i18n::t("Inicio"),
                })
                .to_string(),
            )],
        ));
    }
    fn apply_preferences(&mut self) {
        for session in self.sessions.values_mut() {
            session.record_commands = self.settings.record_commands;
            if let Ok(mut term) = session.term.lock() {
                term.set_options(alacritty_terminal::term::Config {
                    scrolling_history: self.settings.scrollback_lines,
                    ..Default::default()
                });
            }
        }
        for remote in self.remote_files.values_mut() {
            if remote.follow_terminal != self.settings.follow_terminal {
                remote.follow_terminal = self.settings.follow_terminal;
                remote.resume_follow();
            }
        }
        self.extensions
            .set_terminal_context_enabled(self.settings.attach_terminal_to_ai);
    }
    fn save_settings(&mut self, ctx: &egui::Context) {
        self.apply_preferences();
        self.palette().apply(ctx);
        self.log(xpeed_i18n::t("Ajustes actualizados").into());
        if !self.ephemeral
            && let Err(error) = self.settings.save()
        {
            self.notice = Some(error);
        }
    }
    fn save_layout(&mut self) {
        if !self.ephemeral && self.layout_dirty {
            let mut tree = self.tree.clone();
            let mut sidebar_tree = self.sidebar_tree.clone();
            if let Some((pane, visibility)) = &self.zoom {
                let target = if workspace::find(&sidebar_tree, *pane).is_some() {
                    &mut sidebar_tree
                } else {
                    &mut tree
                };
                for (pane, visible) in visibility {
                    if let Some(id) = workspace::find(target, *pane) {
                        target.set_visible(id, *visible);
                    }
                }
                workspace::refresh_visibility(target);
            }
            let snapshot = Snapshot {
                layout_version: 3,
                tree,
                sources: self.sources.clone(),
                sidebar_tree: Some(sidebar_tree),
            };
            if let Err(error) = snapshot.save() {
                self.notice = Some(xpeed_i18n::format(
                    "No se pudo guardar la distribución: {error}",
                    &[("error", error.to_string())],
                ));
            }
        }
        self.layout_dirty = false;
    }
    fn reconnect_sessions(&mut self, ctx: &egui::Context) {
        if !self.settings.auto_reconnect || self.ephemeral {
            for session in self.sessions.values_mut() {
                if session.reconnecting {
                    session.reconnecting = false;
                    session.error = None;
                }
            }
            self.reconnects.clear();
            return;
        }
        let now = Instant::now();
        let disconnected: Vec<_> = self
            .sessions
            .iter()
            .filter_map(|(&id, session)| {
                (session.unexpected_disconnect && !self.reconnects.contains_key(&id)).then_some(id)
            })
            .collect();
        for id in disconnected {
            self.reconnects
                .insert(id, (0, now + Duration::from_secs(2)));
        }
        let ready: Vec<_> = self
            .reconnects
            .iter()
            .filter_map(|(&id, &(attempt, due))| {
                let session = self.sessions.get(&id)?;
                (session.exited && attempt < 5 && now >= due).then_some((id, attempt))
            })
            .collect();
        for (id, attempt) in ready {
            let previous_focus = self.focused;
            let previous_widget = ctx.memory(|m| m.focused());
            if let Some(source) = self.sources.get(&id).cloned() {
                if let Err(error) = self.start_existing(id, &source, ctx) {
                    self.notice = Some(error);
                }
                self.focused = previous_focus;
                if let Some(widget) = previous_widget {
                    ctx.memory_mut(|m| m.request_focus(widget));
                }
            }
            self.reconnects.insert(
                id,
                (
                    attempt + 1,
                    now + Duration::from_secs(2u64.pow(attempt + 2).min(30)),
                ),
            );
        }
        for (&id, session) in &mut self.sessions {
            if self.reconnects.contains_key(&id)
                && ((session.auth_complete && !session.exited) || session.at_password_prompt())
            {
                session.error = None;
            }
        }
        self.reconnects.retain(|id, _| {
            self.sessions
                .get(id)
                .is_some_and(|s| (!s.auth_complete || s.exited) && !s.at_password_prompt())
        });
        for (&id, session) in &mut self.sessions {
            session.reconnecting = self
                .reconnects
                .get(&id)
                .is_some_and(|(attempt, _)| *attempt < 5 || !session.exited);
        }
        for (&id, &(attempt, due)) in &self.reconnects {
            if let Some(session) = self.sessions.get_mut(&id) {
                session.error = Some(if attempt >= 5 && session.exited {
                    xpeed_i18n::t("Reconnection stopped. Use Reconnect to try again.").into()
                } else if !session.exited {
                    xpeed_i18n::format(
                        "Reconnecting: attempt {attempt}/5",
                        &[("attempt", attempt.max(1).to_string())],
                    )
                } else {
                    xpeed_i18n::format(
                        "Reconnecting: attempt {attempt}/5, in {seconds} s",
                        &[
                            ("attempt", (attempt + 1).to_string()),
                            (
                                "seconds",
                                due.saturating_duration_since(now).as_secs().to_string(),
                            ),
                        ],
                    )
                });
            }
        }
    }
    fn action(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Hide(pane) => {
                self.restore_zoom();
                for tree in [&mut self.tree, &mut self.sidebar_tree] {
                    for member in workspace::panel_members(tree, pane) {
                        if let Some(id) = workspace::find(tree, member) {
                            tree.set_visible(id, false);
                        }
                    }
                    workspace::refresh_visibility(tree);
                }
                self.changed();
            }
            Action::ZoomPanel(pane) => {
                if self.zoom.as_ref().is_some_and(|(current, _)| {
                    let tree = if workspace::find(&self.sidebar_tree, pane).is_some() {
                        &self.sidebar_tree
                    } else {
                        &self.tree
                    };
                    workspace::panel_members(tree, pane).contains(current)
                }) {
                    self.restore_zoom();
                } else {
                    self.restore_zoom();
                    let tree = if workspace::find(&self.sidebar_tree, pane).is_some() {
                        &self.sidebar_tree
                    } else {
                        &self.tree
                    };
                    let visibility = tree
                        .tiles
                        .iter()
                        .filter_map(|(&id, tile)| {
                            if let Tile::Pane(pane) = tile {
                                Some((*pane, tree.tiles.is_visible(id)))
                            } else {
                                None
                            }
                        })
                        .collect();
                    self.zoom = Some((pane, visibility));
                    self.apply_zoom();
                }
            }
            Action::Local(profile) => self.open(local_connection(profile, &self.cwd), None, ctx),
            Action::Open(connection) => self.open(connection, None, ctx),
            Action::Backup => {
                self.productivity.backup_open = true;
            }
            Action::Reconnect(id) => {
                self.reconnects.remove(&id);
                if let Some(connection) = self.sources.get(&id).cloned()
                    && let Err(error) = self.start_existing(id, &connection, ctx)
                {
                    self.notice = Some(error);
                }
            }
            Action::Focus(id) => {
                self.restore_zoom();
                self.focused = Some(id);
                if let Some(source) = self.sources.get(&id) {
                    self.log(xpeed_i18n::format(
                        "Sesión seleccionada: {}",
                        &[("0", source.name.to_string())],
                    ));
                }
                ctx.memory_mut(|m| {
                    if let Some(widget) = self.sessions.get(&id).and_then(|s| s.input_id) {
                        m.request_focus(widget);
                    } else if let Some(widget) = m.focused() {
                        m.surrender_focus(widget);
                    }
                });
                self.tree
                    .make_active(|_, tile| *tile == Tile::Pane(Pane::Terminal(id)));
            }
            Action::Close(id) => self.close(id),
            Action::Split(kind) => self.split(kind, ctx),
            Action::Show(pane) => self.show(pane),
            Action::OpenExtension(id) => self.extensions.open_extension(&id),
            Action::Preset(right) => {
                self.restore_zoom();
                self.settings.sidebar = true;
                self.log(xpeed_i18n::t("Distribución restablecida").into());
                self.tree = workspace::default_layout(
                    &self.sources.keys().copied().collect::<Vec<_>>(),
                    right,
                );
                workspace::detach_sidebar(&mut self.tree);
                if let Some(id) = self.focused {
                    self.tree
                        .make_active(|_, tile| *tile == Tile::Pane(Pane::Terminal(id)));
                }
                self.changed();
            }
            Action::ToggleSidebar => {
                if self.sidebar_view != SidebarView::Sessions || !self.settings.sidebar {
                    self.sidebar_view = SidebarView::Sessions;
                    self.settings.sidebar = true;
                } else {
                    self.settings.sidebar = false;
                }
                self.changed();
            }
            Action::ToggleAI => {
                let visible = self.right_panel_panes().into_iter().any(|pane| {
                    workspace::find(&self.tree, pane)
                        .is_some_and(|id| self.tree.tiles.is_visible(id))
                });
                if visible {
                    self.hide_right_column();
                    self.changed();
                } else {
                    let pane = if workspace::find(&self.tree, self.last_right_panel).is_some() {
                        self.last_right_panel
                    } else {
                        Pane::AppInfo
                    };
                    self.show(pane);
                }
            }
            Action::Home => {
                if workspace::find(&self.tree, Pane::Welcome).is_none() {
                    let pane = self.tree.tiles.insert_pane(Pane::Welcome);
                    if let Some(id) = self
                        .focused
                        .and_then(|id| workspace::find(&self.tree, Pane::Terminal(id)))
                        .and_then(|tile| self.tree.tiles.parent_of(tile))
                        && let Some(Tile::Container(egui_tiles::Container::Tabs(tabs))) =
                            self.tree.tiles.get_mut(id)
                    {
                        tabs.children.insert(0, pane);
                    }
                }
                self.show(Pane::Welcome);
            }
            Action::InsertCommand(id, command) => {
                if let Some(session) = self.sessions.get_mut(&id) {
                    // History clicks insert text; only Enter may execute it.
                    let command: String = command
                        .chars()
                        .map(|c| if c.is_control() { ' ' } else { c })
                        .collect();
                    session.paste(&command);
                }
                self.focused = Some(id);
                self.show(Pane::Terminal(id));
                if let Some(widget) = self.sessions.get(&id).and_then(|s| s.input_id) {
                    ctx.memory_mut(|m| m.request_focus(widget));
                }
            }
            Action::ClearCommands => {
                if let Some(source) = self.focused.and_then(|id| self.sources.get(&id)) {
                    let prefix = format!("{} :: ", connection_key(source));
                    self.history
                        .commands
                        .retain(|key, _| !key.starts_with(&prefix));
                }
                if !self.ephemeral
                    && let Err(error) = self.history.save()
                {
                    self.notice = Some(error);
                }
                self.log(xpeed_i18n::t("Registro de comandos borrado").into());
            }
            Action::Settings => {
                self.sidebar_view = if self.sidebar_view == SidebarView::Settings {
                    SidebarView::Sessions
                } else {
                    SidebarView::Settings
                };
                self.settings.sidebar = true;
                self.log(xpeed_i18n::t("Ajustes alternados").into());
            }
            Action::NewConnection => {
                self.editor = Some(ConnectionEditor::default());
                self.log(xpeed_i18n::t("Formulario de conexión abierto").into());
            }
            Action::EditConnection(index) => {
                if let Some(connection) = self.connections.saved.get(index) {
                    let mut editor = ConnectionEditor::from_saved(index, connection);
                    editor.group = self
                        .connections
                        .groups
                        .get(&connection_key(connection))
                        .cloned()
                        .unwrap_or_default();
                    self.editor = Some(editor);
                }
            }
            Action::DeleteConnection(index) => {
                if index < self.connections.saved.len() {
                    let source = self.connections.saved.remove(index);
                    self.log(xpeed_i18n::format(
                        "Perfil eliminado: {}",
                        &[("0", source.name.to_string())],
                    ));
                    if !self.ephemeral
                        && let Err(error) = self.connections.save()
                    {
                        self.notice = Some(error);
                    }
                }
            }
            Action::Draft(command) => {
                if let Some(session) = self.focused.and_then(|id| self.sessions.get_mut(&id)) {
                    session.paste(command);
                    session.bottom();
                }
            }
            Action::ForgetAccess(index) => {
                if let Some(Connection {
                    target: Target::Ssh(config),
                    ..
                }) = self.connections.saved.get(index)
                    && let Err(error) = crate::credentials::forget(config)
                {
                    self.notice = Some(error);
                }
            }
            Action::Search => {
                self.show_search = !self.show_search;
                if self.show_search {
                    ctx.memory_mut(|m| m.request_focus(egui::Id::new("terminal_search")));
                }
            }
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let chord = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        };
        let mut actions = Vec::new();
        for (key, action) in [
            (Key::T, Action::Local(Profile::default_shell())),
            (Key::D, Action::Split(ContainerKind::Horizontal)),
            (Key::E, Action::Split(ContainerKind::Vertical)),
            (Key::B, Action::ToggleSidebar),
            (Key::F, Action::Search),
            (Key::N, Action::NewConnection),
        ] {
            if ctx.input_mut(|i| i.consume_key(chord, key)) {
                actions.push(action);
            }
        }
        if ctx.input_mut(|i| i.consume_key(chord, Key::W))
            && let Some(id) = self.focused
        {
            actions.push(Action::Close(id));
        }
        if ctx.input_mut(|i| i.consume_key(chord, Key::P)) {
            self.show_palette = !self.show_palette;
            self.palette_focus = true;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Comma)) {
            self.sidebar_view = if self.sidebar_view == SidebarView::Settings {
                SidebarView::Sessions
            } else {
                SidebarView::Settings
            };
            self.settings.sidebar = true;
        }
        if (self.show_search
            || self.show_palette
            || (self.sidebar_view == SidebarView::Settings)
            || self.editor.is_some())
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.show_search = false;
            self.show_palette = false;
            self.sidebar_view = SidebarView::Sessions;
            self.editor = None;
        }
        let previous = ctx.input_mut(|i| i.consume_key(chord, Key::Tab));
        let next = !previous && ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Tab));
        if previous || next {
            let ids: Vec<_> = self.sources.keys().copied().collect();
            if !ids.is_empty() {
                let index = ids
                    .iter()
                    .position(|id| Some(*id) == self.focused)
                    .unwrap_or(0);
                let offset = if previous { ids.len() - 1 } else { 1 };
                actions.push(Action::Focus(ids[(index + offset) % ids.len()]));
            }
        }
        for action in actions {
            self.action(action, ctx);
        }
    }
    fn toolbar(&mut self, ctx: &egui::Context) {
        let p = self.palette();
        let mut actions = Vec::new();
        egui::TopBottomPanel::top("toolbar")
            .exact_height(40.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .corner_radius(10)
                    .inner_margin(egui::Margin {
                        left: 12,
                        right: 8,
                        top: 6,
                        bottom: 6,
                    }),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let logo_size = crate::svg_icons::logo_size(14.0);
                    let (logo_slot, _) =
                        ui.allocate_exact_size(Vec2::new(logo_size.x, 28.0), egui::Sense::hover());
                    crate::svg_icons::draw(
                        ui.painter(),
                        egui::Rect::from_center_size(logo_slot.center(), logo_size),
                        crate::svg_icons::Icon::Logo,
                        p.accent,
                    );
                    ui.add_space(12.0);
                    ui.menu_button(xpeed_i18n::t("Sesión"), |ui| {
                        if ui.button(xpeed_i18n::t("Inicio")).clicked() {
                            actions.push(Action::Home);
                            ui.close();
                        }
                        if ui.button(xpeed_i18n::t("Nueva conexión…")).clicked() {
                            actions.push(Action::NewConnection);
                            ui.close();
                        }
                        for profile in Profile::available() {
                            if ui.button(profile.name()).clicked() {
                                actions.push(Action::Local(profile));
                                ui.close();
                            }
                        }
                    });
                    ui.menu_button(xpeed_i18n::t("Vista"), |ui| {
                        for (label, action) in [
                            (xpeed_i18n::t("Conexiones"), Action::Show(Pane::Connections)),
                            (
                                xpeed_i18n::t("Registro de comandos"),
                                Action::Show(Pane::Commands),
                            ),
                            (xpeed_i18n::t("Actividad"), Action::Show(Pane::Activity)),
                            (xpeed_i18n::t("Extensiones"), Action::Show(Pane::Extensions)),
                            (
                                xpeed_i18n::t("Información de Xpeed"),
                                Action::Show(Pane::AppInfo),
                            ),
                            (
                                xpeed_i18n::t("Información del servidor"),
                                Action::Show(Pane::ServerInfo),
                            ),
                            (
                                xpeed_i18n::t("Mostrar/ocultar panel derecho"),
                                Action::ToggleAI,
                            ),
                            (
                                xpeed_i18n::t("Restablecer distribución"),
                                Action::Preset(false),
                            ),
                        ] {
                            if ui.button(label).clicked() {
                                actions.push(action);
                                ui.close();
                            }
                        }
                    });
                    ui.menu_button(xpeed_i18n::t("Extensiones"), |ui| {
                        for (id, name, enabled, views) in self.extensions.shortcuts() {
                            let active = enabled && self.extension_active(&id, &views);
                            let label = if active {
                                xpeed_i18n::format("{name} · Activa", &[("name", name.to_string())])
                            } else if !enabled {
                                xpeed_i18n::format(
                                    "{name} · Desactivada",
                                    &[("name", name.to_string())],
                                )
                            } else {
                                name
                            };
                            if ui.selectable_label(active, label).clicked() {
                                actions.push(Action::OpenExtension(id));
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui
                            .button(xpeed_i18n::t("Administrar extensiones…"))
                            .clicked()
                        {
                            actions.push(Action::Show(Pane::Extensions));
                            ui.close();
                        }
                    });
                    let controls_width = 5.0 * 32.0 + 4.0 * ui.spacing().item_spacing.x;
                    let (_, drag) = ui.allocate_exact_size(
                        Vec2::new(
                            (ui.available_width() - controls_width - ui.spacing().item_spacing.x)
                                .max(0.0),
                            27.0,
                        ),
                        egui::Sense::click_and_drag(),
                    );
                    if drag.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if drag.double_clicked() {
                        self.window_size.toggle_maximized(ctx);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if title_icon_button(ui, Icon::Close, p)
                            .on_hover_text(xpeed_i18n::t("Cerrar Xpeed"))
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if title_icon_button(ui, Icon::Maximize, p)
                            .on_hover_text(xpeed_i18n::t("Maximizar / restaurar"))
                            .clicked()
                        {
                            self.window_size.toggle_maximized(ctx);
                        }
                        if title_icon_button(ui, Icon::Minimize, p)
                            .on_hover_text(xpeed_i18n::t("Minimizar"))
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        if title_icon_button(ui, Icon::SidebarRight, p)
                            .on_hover_text(xpeed_i18n::t("Mostrar / ocultar panel derecho"))
                            .clicked()
                        {
                            actions.push(Action::ToggleAI);
                        }
                        if title_icon_button(ui, Icon::SidebarLeft, p)
                            .on_hover_text(xpeed_i18n::t(
                                "Mostrar / ocultar conexiones y registros",
                            ))
                            .clicked()
                        {
                            actions.push(Action::ToggleSidebar);
                        }
                    });
                });
            });
        for action in actions {
            self.action(action, ctx);
        }
    }
    fn activity_rail(&mut self, ctx: &egui::Context) {
        let p = self.palette();
        let mut action = None;
        egui::SidePanel::left("activity_rail")
            .show_separator_line(false)
            .exact_width(47.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::symmetric(5, 12))
                    .stroke(Stroke::new(1.0_f32, p.border)),
            )
            .show(ctx, |ui| {
                let selected = self.settings.sidebar && self.sidebar_view == SidebarView::Sessions;
                let response = icon_button(ui, Icon::Workspace, p);
                if selected {
                    ui.painter().rect_filled(response.rect, 6, p.raised);
                    draw_icon(
                        ui.painter(),
                        response.rect.shrink(8.0),
                        Icon::Workspace,
                        p.accent,
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(response.rect.left_top(), Vec2::new(2.0, 36.0)),
                        1,
                        p.accent,
                    );
                }
                if response
                    .on_hover_text(xpeed_i18n::t("Mostrar/ocultar conexiones y registros"))
                    .clicked()
                {
                    action = Some(Action::ToggleSidebar);
                }
                let extensions_button = icon_button(ui, Icon::Extensions, p);
                if self.settings.sidebar && self.sidebar_view == SidebarView::Extensions {
                    ui.painter()
                        .rect_filled(extensions_button.rect, 6, p.raised);
                    draw_icon(
                        ui.painter(),
                        extensions_button.rect.shrink(8.0),
                        Icon::Extensions,
                        p.accent,
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            extensions_button.rect.left_top(),
                            Vec2::new(2.0, 36.0),
                        ),
                        1,
                        p.accent,
                    );
                }
                if extensions_button
                    .on_hover_text(xpeed_i18n::t("Extensiones"))
                    .clicked()
                {
                    action = Some(Action::Show(Pane::Extensions));
                }
                let space = ui.available_height();
                if space > 36.0 {
                    ui.add_space(space - 36.0);
                }
                let settings_button = icon_button(ui, Icon::Settings, p);
                if self.settings.sidebar && self.sidebar_view == SidebarView::Settings {
                    ui.painter().rect_filled(settings_button.rect, 6, p.raised);
                    draw_icon(
                        ui.painter(),
                        settings_button.rect.shrink(8.0),
                        Icon::Settings,
                        p.accent,
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            settings_button.rect.left_top(),
                            Vec2::new(2.0, 36.0),
                        ),
                        1,
                        p.accent,
                    );
                }
                if settings_button
                    .on_hover_text(xpeed_i18n::t("Apariencia y ajustes"))
                    .clicked()
                {
                    action = Some(Action::Settings);
                }
            });
        if let Some(action) = action {
            self.action(action, ctx);
        }
    }
    fn status(&mut self, ctx: &egui::Context) {
        let p = self.palette();
        egui::TopBottomPanel::bottom("status")
            .show_separator_line(false)
            .exact_height(26.0)
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 10,
                        se: 10,
                    })
                    .inner_margin(egui::Margin::symmetric(12, 4))
                    .stroke(Stroke::new(1.0_f32, p.border)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::click());
                    crate::svg_icons::draw(
                        ui.painter(),
                        rect,
                        crate::svg_icons::Icon::Info,
                        p.accent,
                    );
                    if response
                        .on_hover_text(xpeed_i18n::t("Información de Xpeed"))
                        .clicked()
                        && let Some(id) = workspace::find(&self.tree, Pane::AppInfo)
                    {
                        if self.tree.tiles.is_visible(id) {
                            self.action(Action::Hide(Pane::AppInfo), ctx);
                        } else {
                            self.show(Pane::AppInfo);
                        }
                    }
                    let views = self.extensions.views();
                    for (id, view) in &views {
                        if let Some(icon) = view.icon {
                            let (rect, response) =
                                ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::click());
                            crate::svg_icons::draw(ui.painter(), rect, icon.svg(), p.muted);
                            if response.on_hover_text(&view.title).clicked() {
                                let pane = Pane::ExtensionView(*id);
                                let sidebar = workspace::find(&self.sidebar_tree, pane).is_some();
                                let host_visible = !sidebar
                                    || (self.settings.sidebar
                                        && self.sidebar_view == SidebarView::Sessions);
                                let tree = if sidebar {
                                    &mut self.sidebar_tree
                                } else {
                                    &mut self.tree
                                };
                                if let Some(tile) = workspace::find(tree, pane) {
                                    if host_visible && tree.tiles.is_visible(tile) {
                                        self.action(Action::Hide(pane), ctx);
                                    } else {
                                        self.show(pane);
                                    }
                                } else {
                                    self.show(pane);
                                }
                            }
                        }
                    }
                    status_dot(ui, p.accent);
                    if let Some(connection) = self.focused.and_then(|id| self.sources.get(&id)) {
                        let active = self.focused.and_then(|id| self.sessions.get(&id));
                        let label = if matches!(connection.target, Target::Ssh(_)) {
                            "SSH"
                        } else {
                            xpeed_i18n::t("LOCAL")
                        };
                        ui.label(
                            RichText::new(format!("{label} · {}", connection.name))
                                .size(10.0)
                                .color(p.muted),
                        );
                        if let Some(session) = active {
                            ui.label(
                                RichText::new(format!(
                                    "#{} · {} × {}  ·  {} · {} min",
                                    session.id,
                                    session.size.cols,
                                    session.size.rows,
                                    if session.exited {
                                        xpeed_i18n::t("Finalizada")
                                    } else {
                                        xpeed_i18n::t("Proceso activo")
                                    },
                                    session.started.elapsed().as_secs() / 60
                                ))
                                .monospace()
                                .size(10.0)
                                .color(p.dim),
                            )
                            .on_hover_text(xpeed_i18n::format(
                                "Directorio inicial: {}",
                                &[("0", format!("{}", session.cwd.display()))],
                            ));
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} / {}",
                                p.name,
                                if p.mode == Mode::Dark {
                                    xpeed_i18n::t("Oscuro")
                                } else {
                                    xpeed_i18n::t("Claro")
                                }
                            ))
                            .size(10.0)
                            .color(p.muted),
                        );
                        ui.label(
                            RichText::new(xpeed_i18n::format(
                                "{} sesiones  ·  UTF-8",
                                &[("0", format!("{}", self.sessions.len()))],
                            ))
                            .size(10.0)
                            .color(p.dim),
                        );
                    });
                });
            });
    }
    fn central(&mut self, ctx: &egui::Context) {
        let p = self.palette();
        let mut actions = Vec::new();
        let mut edited = false;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin {
                        left: if self.settings.sidebar { PANEL_GAP - PANEL_GAP / 2 } else { PANEL_GAP },
                        right: PANEL_GAP,
                        top: PANEL_GAP,
                        bottom: PANEL_GAP,
                    })
                    .corner_radius(10),
            )
            .show(ctx, |ui| {
                if self.show_search {
                    egui::Frame::new()
                        .fill(p.panel)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(xpeed_i18n::t("Buscar en terminal"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.search)
                                        .id(egui::Id::new("terminal_search"))
                                        .hint_text(xpeed_i18n::t("Texto a resaltar"))
                                        .desired_width(250.0),
                                );
                                let previous = ui.small_button("↑").on_hover_text(xpeed_i18n::t("Previous")).clicked();
                                let next = ui.small_button("↓").on_hover_text(xpeed_i18n::t("Next")).clicked();
                                if (previous || next || ui.input(|i| i.key_pressed(egui::Key::Enter))) && let Some(session) = self.focused.and_then(|id| self.sessions.get_mut(&id)) {
                                    self.search_result = session.find_text(&self.search, previous);
                                }
                                ui.weak(format!("{} / {}", self.search_result.0, self.search_result.1));
                                if ui.button(xpeed_i18n::t("Cerrar")).clicked() {
                                    self.show_search = false;
                                }
                            });
                        });
                }
                let terminal_focus = ctx.memory(|memory| memory.focused()).is_none_or(|focus| {
                    self.focused.and_then(|id| self.sessions.get(&id)).is_some_and(|session| session.input_id == Some(focus))
                });
                let keyboard = terminal_focus
                    && !self.show_palette
                    && self.editor.is_none()
                    && self.access_prompt.is_none()
                    && !self.remote_files.values().any(|remote| remote.browser.has_dialog())
                    && self.notice.is_none()
                    && !self.show_search;
                let terminal_context = self.focused.and_then(|id| self.sessions.get(&id)).map(|session| {
                    if !self.extensions.wants_terminal_context() { return String::new(); }
                    let Ok(term) = session.term.lock() else { return String::new(); };
                    let mut text = String::new();
                    for cell in term.renderable_content().display_iter {
                        if cell.point.column.0 == 0 { text.push('\n'); }
                        if !cell.cell.flags.intersects(alacritty_terminal::term::cell::Flags::HIDDEN | alacritty_terminal::term::cell::Flags::WIDE_CHAR_SPACER | alacritty_terminal::term::cell::Flags::LEADING_WIDE_CHAR_SPACER) {
                            text.push(cell.cell.c);
                        }
                    }
                    text.chars().take(24000).collect::<String>()
                });
                self.extensions.selected_fragment(self.focused.and_then(|id|self.sessions.get(&id).and_then(|session|session.term.lock().ok()?.selection_to_string().map(|text|(id,text)))));
                let mut behavior = WorkspaceUi {
                    sessions: &mut self.sessions,
                    sources: &self.sources,
                    connections: &self.connections,
                    remote_files: &mut self.remote_files,
                    assistant: &mut self.assistant,
                    extensions: &mut self.extensions,
                    terminal_context: terminal_context.as_deref(),
                    history: &self.history,
                    activity: &self.activity,
                    focused: &mut self.focused,
                    palette: p,
                    font_size: self.settings.font_size,
                    query: &self.query,
                    search: if self.show_search { &self.search } else { "" },
                    keyboard,
                    actions: &mut actions,
                    edited: &mut edited,
                };
                for (&id, tile) in self.tree.tiles.iter() {
                    if matches!(tile, Tile::Container(egui_tiles::Container::Tabs(_)))
                        && self.tree.tiles.is_visible(id)
                        && let Some(rect) = self.tree.tiles.rect(id)
                    {
                        ui.painter().rect(
                            rect,
                            10,
                            p.panel,
                            Stroke::new(1.0_f32, p.border),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                self.tree.ui(&mut behavior, ui);
                paint_dock_dividers(ui, &self.tree, p);
            });
        if edited {
            self.log(xpeed_i18n::t("Panel reorganizado, redimensionado o seleccionado").into());
            self.changed();
        }
        for action in actions {
            self.action(action, ctx);
        }
    }
    fn overlays(&mut self, ctx: &egui::Context) {
        self.access_window(ctx);

        self.connection_window(ctx);
        self.palette_window(ctx);
        if let Some(notice) = self.notice.clone() {
            let mut open = true;
            egui::Window::new("Xpeed")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(420.0)
                .show(ctx, |ui| {
                    ui.label(notice);
                    ui.add_space(12.0);
                    if ui.button(xpeed_i18n::t("Entendido")).clicked() {
                        self.notice = None;
                    }
                });
            if !open {
                self.notice = None;
            }
        }
    }
    fn access_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.access_prompt else {
            return;
        };
        let Some(source) = self.sources.get(&id).cloned() else {
            self.access_prompt = None;
            return;
        };
        let mut accept = false;
        let mut decline = false;
        let modal = egui::Modal::new(egui::Id::new("save_ssh_access")).show(ctx, |ui| {
            ui.set_width(390.0);
            ui.heading(xpeed_i18n::t("¿Guardar este acceso?"));
            ui.label(xpeed_i18n::format("La conexión con {} está autenticada.", &[("0", source.description().to_string())]));
            ui.add_space(10.0);
            ui.label(xpeed_i18n::t("Guarda la conexión y su contraseña en el almacén seguro del sistema para entrar directamente la próxima vez."));
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                accept = ui.button(xpeed_i18n::t("Guardar acceso")).clicked();
                decline = ui.button(xpeed_i18n::t("Ahora no")).clicked();
            });
        });
        decline |= modal.should_close();
        if accept
            && let Target::Ssh(config) = &source.target
            && let Some(password) = self.access_candidates.get(&id)
        {
            if let Err(error) = crate::credentials::save(config, password) {
                self.notice = Some(error);
                return;
            }
            if !self
                .connections
                .saved
                .iter()
                .any(|c| matches!(&c.target, Target::Ssh(saved) if saved == config))
            {
                self.connections.saved.push(source);
            }
            if let Err(error) = self.connections.save() {
                self.notice = Some(error);
                return;
            }
            self.log(xpeed_i18n::t("Acceso SSH guardado en el almacén del sistema").into());
        }
        if accept || decline {
            self.access_handled.insert(id);
            self.access_candidates.remove(&id);
            self.access_prompt = None;
        }
    }
    fn session_sidebar_ui(&mut self, ui: &mut egui::Ui) {
        let p = self.palette();
        let mut actions = Vec::new();
        let mut edited = false;
        let mut behavior = WorkspaceUi {
            sessions: &mut self.sessions,
            sources: &self.sources,
            connections: &self.connections,
            remote_files: &mut self.remote_files,
            assistant: &mut self.assistant,
            extensions: &mut self.extensions,
            terminal_context: None,
            history: &self.history,
            activity: &self.activity,
            focused: &mut self.focused,
            palette: p,
            font_size: self.settings.font_size,
            query: &self.query,
            search: "",
            keyboard: false,
            actions: &mut actions,
            edited: &mut edited,
        };
        for (&id, tile) in self.sidebar_tree.tiles.iter() {
            if matches!(tile, Tile::Container(egui_tiles::Container::Tabs(_)))
                && self.sidebar_tree.tiles.is_visible(id)
                && let Some(rect) = self.sidebar_tree.tiles.rect(id)
            {
                ui.painter().rect(
                    rect,
                    10,
                    p.panel,
                    Stroke::new(1.0_f32, p.border),
                    egui::StrokeKind::Inside,
                );
            }
        }
        self.sidebar_tree.ui(&mut behavior, ui);
        paint_dock_dividers(ui, &self.sidebar_tree, p);
        if edited {
            self.changed();
        }
        for action in actions {
            self.action(action, ui.ctx());
        }
    }
    fn extension_sections(&mut self, ui: &mut egui::Ui, location: crate::extensions::ViewLocation) {
        let views = self.extensions.views();
        for (id, view) in views
            .into_iter()
            .filter(|(_, view)| view.location == location)
        {
            ui.separator();
            egui::CollapsingHeader::new(&view.title)
                .id_salt(id)
                .show(ui, |ui| self.extensions.view_ui(ui, id));
        }
    }
    fn sidebar_panel(&mut self, ctx: &egui::Context) {
        if !self.settings.sidebar {
            return;
        }
        let p = self.palette();
        let mut changed = false;
        let mut preset = None;
        let panel = egui::SidePanel::left("options_panel")
            .default_width(300.0).min_width(250.0).max_width(420.0)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin {
                left: PANEL_GAP,
                right: PANEL_GAP / 2,
                top: PANEL_GAP,
                bottom: PANEL_GAP,
            }))
            .show(ctx, |ui| {
                if self.sidebar_view == SidebarView::Sessions { self.session_sidebar_ui(ui); return; }
                if self.sidebar_view == SidebarView::Extensions {
                    egui::Frame::new().fill(p.panel).corner_radius(10).stroke(Stroke::new(1.0_f32,p.border)).inner_margin(14).show(ui, |ui| {
                        self.extensions.manager_ui(ui);
                        self.extension_sections(ui, crate::extensions::ViewLocation::Extensions);
                    }); return;
                }
                egui::Frame::new().fill(p.panel).corner_radius(10)
                    .stroke(Stroke::new(1.0_f32, p.border)).inner_margin(14).show(ui, |ui| {
                    ui.set_min_height(ui.available_height());
                    ui.horizontal(|ui| {
                        ui.heading(xpeed_i18n::t("Ajustes"));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("×").on_hover_text(xpeed_i18n::t("Cerrar ajustes")).clicked() { self.sidebar_view = SidebarView::Sessions; }
                        });
                    });
                    ui.separator();
                    egui::ScrollArea::vertical().id_salt("settings_scroll").show(ui, |ui| {
                        ui.add_space(10.0);
                        egui::CollapsingHeader::new(xpeed_i18n::t("Idioma")).id_salt("settings_language").default_open(true).show(ui, |ui| {
                            egui::ComboBox::from_id_salt("language_selector").selected_text(self.settings.language.name()).show_ui(ui, |ui| {
                                for language in xpeed_i18n::Language::ALL {
                                    changed |= ui.selectable_value(&mut self.settings.language, language, language.name()).changed();
                                }
                            });
                            ui.add(egui::Label::new(xpeed_i18n::t("El idioma se aplica a Xpeed y a las extensiones compatibles.")).wrap());
                        });
                        ui.separator();
                        egui::CollapsingHeader::new(RichText::new(xpeed_i18n::t("Temas y apariencia")).strong()).id_salt("settings_themes").default_open(true).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for (mode, icon, label) in [(Mode::Dark, crate::svg_icons::Icon::Moon, xpeed_i18n::t("Oscuro")), (Mode::Light, crate::svg_icons::Icon::Sun, xpeed_i18n::t("Claro"))] {
                                    let selected = self.settings.mode == mode;
                                    let response = ui.add_sized([44.0, 36.0], egui::Button::new("").selected(selected)).on_hover_text(label);
                                    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label));
                                    crate::svg_icons::draw(ui.painter(), egui::Rect::from_center_size(response.rect.center(), Vec2::splat(24.0)), icon, if selected || response.hovered() { p.accent } else { p.muted });
                                    if response.clicked() && !selected { self.settings.mode = mode; changed = true; }
                                }
                            });
                            ui.add_space(8.0);
                            for (index, name) in Palette::names(self.settings.mode).iter().enumerate() {
                                let theme = Palette::new(self.settings.mode, index);
                                let selected = if self.settings.mode == Mode::Dark { self.settings.dark_theme } else { self.settings.light_theme } == index;
                                let label = if selected { xpeed_i18n::format("{name} · Activo", &[("name", name.to_string())]) } else { name.to_string() };
                                if ui.add_sized([ui.available_width(), 40.0], egui::Button::new(RichText::new(label).color(theme.text)).fill(theme.terminal).stroke(Stroke::new(1.0_f32, if selected {theme.accent} else {theme.border}))).clicked() {
                                    if self.settings.mode == Mode::Dark { self.settings.dark_theme = index; } else { self.settings.light_theme = index; }
                                    changed = true;
                                }
                                ui.add_space(6.0);
                            }
                        });
                        ui.separator();
                        egui::CollapsingHeader::new(RichText::new(xpeed_i18n::t("Editor y terminal")).strong()).id_salt("settings_editor").show(ui, |ui| {
                            ui.label(xpeed_i18n::t("Tamaño del texto de la terminal"));
                            changed |= ui.add(egui::Slider::new(&mut self.settings.font_size, 11.0..=24.0).suffix(" px")).changed();
                            ui.weak(format!("{}: {:.2} ms", xpeed_i18n::t("Interface CPU time"), self.frame_cost_ms));
                            ui.label(xpeed_i18n::t("Terminal scrollback lines"));
                            changed |= ui.add(egui::Slider::new(&mut self.settings.scrollback_lines, 100..=50000).logarithmic(true)).changed();
                        });
                        ui.separator();
                        egui::CollapsingHeader::new(RichText::new(xpeed_i18n::t("Archivos")).strong()).id_salt("settings_files").show(ui, |ui| {
                            changed |= ui.checkbox(&mut self.settings.follow_terminal, xpeed_i18n::t("Seguir carpeta de la terminal")).changed();
                            ui.add(egui::Label::new(RichText::new(xpeed_i18n::t("El explorador remoto sigue el directorio de la consola al usar cd.")).small()).wrap());
                        });
                        ui.separator();
                        egui::CollapsingHeader::new(RichText::new(xpeed_i18n::t("Comandos e historial")).strong()).id_salt("settings_commands").show(ui, |ui| {
                            changed |= ui.checkbox(&mut self.settings.record_commands, xpeed_i18n::t("Registrar comandos")).changed();
                            ui.add(egui::Label::new(RichText::new(xpeed_i18n::t("Se aplica a las conexiones abiertas y nuevas. Cada conexión conserva su propio historial.")).small()).wrap());
                        });
                        ui.separator();
                        egui::CollapsingHeader::new(RichText::new(xpeed_i18n::t("IA y herramientas")).strong()).id_salt("settings_ai").show(ui, |ui| {
                            changed |= ui.checkbox(&mut self.settings.attach_terminal_to_ai, xpeed_i18n::t("Adjuntar texto visible de la terminal")).changed();
                            ui.add(egui::Label::new(RichText::new(xpeed_i18n::t("El texto se adjunta al enviar una consulta desde una extensión. Instala y activa cada proveedor en Extensiones.")).small()).wrap());
                        });
                        ui.separator();
                        egui::CollapsingHeader::new(RichText::new(xpeed_i18n::t("Distribución")).strong()).id_salt("settings_layout").show(ui, |ui| {
                            if ui.button(xpeed_i18n::t("Restablecer distribución")).clicked() { preset = Some(false); }
                            ui.add(egui::Label::new(RichText::new(xpeed_i18n::t("Arrastra las pestañas de terminales y herramientas para moverlas; usa los separadores para cambiar el tamaño.")).small()).wrap());
                        });
                        egui::CollapsingHeader::new(xpeed_i18n::t("Connections and backups")).show(ui, |ui| {
                            changed |= ui.checkbox(&mut self.settings.auto_reconnect, xpeed_i18n::t("Reconnect SSH automatically")).changed();
                            ui.weak(xpeed_i18n::t("Reopens the session after a network failure; commands are never replayed."));
                            if ui.button(xpeed_i18n::t("Export / import configuration")).clicked() { self.productivity.backup_open = true; }
                        });
                        self.extension_sections(ui, crate::extensions::ViewLocation::Settings);
                        egui::CollapsingHeader::new(xpeed_i18n::t("Updates")).id_salt("settings_updates").show(ui, |ui| {
                            self.updates.ui(ui);
                        });
                        ui.add_space(12.0);
                        ui.add(egui::Label::new(RichText::new(xpeed_i18n::t("Los ajustes se guardan automáticamente en este equipo.")).size(11.0).color(p.dim)).wrap());
                    });
                });
            });
        // Keep the divider on its panel layer, below popup menus and modals.
        let painter = ctx.layer_painter(panel.response.layer_id);
        let rect = panel.response.rect;
        let x = rect.right() + 0.5;
        // Cover the native hover/drag stroke and paint the same rounded divider
        // used by the dock tree. The transparent gap still receives resizing.
        painter.rect_filled(
            egui::Rect::from_center_size(
                egui::pos2(x, rect.center().y),
                Vec2::new(f32::from(PANEL_GAP), rect.height()),
            ),
            0,
            p.bg,
        );
        paint_divider(
            ctx,
            &painter,
            egui::Id::new("options_divider_drag"),
            egui::Rect::from_min_max(
                egui::pos2(x - 1.0, rect.top() + 10.0),
                egui::pos2(x + 1.0, rect.bottom() - 10.0),
            ),
            p,
        );
        if changed {
            xpeed_i18n::set_language(self.settings.language);
            self.extensions.language_changed(self.settings.language);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                xpeed_i18n::t("Xpeed — Conexiones y terminales").into(),
            ));
            self.save_settings(ctx);
        }
        if let Some(right) = preset {
            self.action(Action::Preset(right), ctx);
        }
    }
    fn connection_window(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let mut submit = false;
        let mut save_only = false;
        let mut cancelled = false;
        let p = self.settings.palette();
        let modal = egui::Modal::new(egui::Id::new("new_connection")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.horizontal(|ui| { ui.heading(xpeed_i18n::t("Nueva conexión")); ui.with_layout(Layout::right_to_left(Align::Center), |ui| { if ui.small_button("×").clicked() { cancelled = true; } }); });
            ui.add_space(14.0);
            ui.horizontal_wrapped(|ui| {
                if ui.selectable_label(editor.selected && editor.remote, "SSH").clicked() { editor.selected = true; editor.remote = true; }
                for profile in Profile::available() {
                    if ui.selectable_label(editor.selected && !editor.remote && editor.shell == profile, format!(">_  {}", profile.name())).clicked() {
                        editor.selected = true; editor.remote = false; editor.shell = profile;
                    }
                }
            });
            ui.separator();
            if !editor.selected {
                ui.add_space(28.0);
                ui.label(RichText::new(xpeed_i18n::t("Elige un tipo de conexión")).size(22.0));
                ui.label(xpeed_i18n::t("Si pulsas Conectar sin elegir uno, se abrirá una terminal local."));
                ui.add_space(28.0);
            } else {
            ui.add_space(16.0);
            field(ui, xpeed_i18n::t("Nombre"), &mut editor.name, xpeed_i18n::t("Ej. Desarrollo / Producción"));
            field(ui, xpeed_i18n::t("Project / group"), &mut editor.group, xpeed_i18n::t("Optional"));
            if editor.remote {
                ui.add_space(8.0);
                if field(
                    ui,
                    xpeed_i18n::t("Host o alias SSH"),
                    &mut editor.ssh.host,
                    "servidor.example.com",
                )
                .changed()
                {
                    editor.error = None;
                }
                ui.horizontal(|ui| {
                    ui.vertical(|ui| { ui.label(xpeed_i18n::t("Usuario (opcional)")); ui.add(egui::TextEdit::singleline(&mut editor.ssh.user).desired_width(320.0).hint_text(xpeed_i18n::t("Usar configuración SSH"))); });
                    ui.vertical(|ui| { ui.label(xpeed_i18n::t("Puerto")); ui.add(egui::DragValue::new(&mut editor.ssh.port).range(1..=65535)); });
                });
                field(ui, xpeed_i18n::t("Archivo de clave privada (opcional)"), &mut editor.ssh.identity, xpeed_i18n::t("Ruta completa al archivo"));
                ui.add_space(12.0); ui.label(RichText::new(xpeed_i18n::t("OpenSSH usa tus claves, el agente y ~/.ssh/config. La contraseña y la confirmación del servidor se introducen en la terminal.")).size(12.0).color(p.muted));
            } else {
                ui.add_space(8.0); egui::ComboBox::from_label(xpeed_i18n::t("Consola")).selected_text(editor.shell.name()).show_ui(ui, |ui| { for profile in Profile::available() { ui.selectable_value(&mut editor.shell, profile, profile.name()); } });
                field(ui, xpeed_i18n::t("Directorio de inicio (opcional)"), &mut editor.directory, xpeed_i18n::t("Usar la carpeta del proyecto"));
            }
            ui.add_space(16.0); ui.checkbox(&mut editor.save, xpeed_i18n::t("Guardar en mis conexiones"));
            }
            if let Some(error) = &editor.error { ui.colored_label(Color32::from_rgb(210, 82, 98), error); }
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new(xpeed_i18n::t("Conectar")).color(if p.mode == Mode::Dark { p.bg } else { Color32::WHITE })).fill(p.accent)).clicked() { submit = true; }
                if editor.selected && ui.button(xpeed_i18n::t("Solo guardar")).clicked() { save_only = true; }
                if ui.button(xpeed_i18n::t("Cancelar")).clicked() { cancelled = true; }
            });
        });
        cancelled |= modal.should_close();
        if submit || save_only {
            let connection = editor.connection();
            match connection.validate() {
                Err(error) => editor.error = Some(error),
                Ok(()) => {
                    if editor.save || save_only {
                        if let Some(old) =
                            editor.editing.and_then(|i| self.connections.saved.get(i))
                        {
                            self.connections.groups.remove(&connection_key(old));
                        }
                        self.connections
                            .groups
                            .insert(connection_key(&connection), editor.group.trim().to_owned());
                        if let Some(index) = editor.editing {
                            if let Some(saved) = self.connections.saved.get_mut(index) {
                                *saved = connection.clone();
                            }
                        } else {
                            self.connections.saved.push(connection.clone());
                        }
                        if !self.ephemeral
                            && let Err(error) = self.connections.save()
                        {
                            editor.error = Some(error);
                            return;
                        }
                    }
                    self.editor = None;
                    self.log(xpeed_i18n::format(
                        "Perfil configurado: {}",
                        &[("0", connection.name.to_string())],
                    ));
                    if submit {
                        self.open(connection, None, ctx);
                    }
                }
            }
        } else if cancelled {
            self.editor = None;
        }
    }
    fn palette_window(&mut self, ctx: &egui::Context) {
        if !self.show_palette {
            return;
        }
        let mut open = true;
        let mut action = None;
        egui::Window::new(xpeed_i18n::t("Acciones rápidas"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_TOP, [0.0, 90.0])
            .default_width(460.0)
            .show(ctx, |ui| {
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.palette_query)
                        .hint_text(xpeed_i18n::t("Buscar una acción…"))
                        .desired_width(f32::INFINITY),
                );
                if self.palette_focus {
                    response.request_focus();
                    self.palette_focus = false;
                }
                ui.add_space(12.0);
                for (label, next) in [
                    (
                        xpeed_i18n::t("Nueva terminal local"),
                        Action::Local(Profile::default_shell()),
                    ),
                    (xpeed_i18n::t("Nueva conexión"), Action::NewConnection),
                    (
                        xpeed_i18n::t("Export / import configuration"),
                        Action::Backup,
                    ),
                    (xpeed_i18n::t("Terminal search"), Action::Search),
                    (xpeed_i18n::t("Extensiones"), Action::Show(Pane::Extensions)),
                    (
                        xpeed_i18n::t("Dividir en columnas"),
                        Action::Split(ContainerKind::Horizontal),
                    ),
                    (
                        xpeed_i18n::t("Dividir en filas"),
                        Action::Split(ContainerKind::Vertical),
                    ),
                    (
                        xpeed_i18n::t("Mostrar archivos locales"),
                        Action::Show(Pane::Files),
                    ),
                    (
                        xpeed_i18n::t("Restablecer distribución"),
                        Action::Preset(false),
                    ),
                    (
                        xpeed_i18n::t("Git status (insertar)"),
                        Action::Draft("git status"),
                    ),
                    (xpeed_i18n::t("Apariencia y temas"), Action::Settings),
                ] {
                    if label
                        .to_lowercase()
                        .contains(&self.palette_query.to_lowercase())
                        && (ui
                            .add_sized([ui.available_width(), 32.0], egui::Button::new(label))
                            .clicked()
                            || enter && action.is_none())
                    {
                        action = Some(next);
                    }
                }
                for connection in &self.connections.saved {
                    let label = format!("{}: {}", xpeed_i18n::t("Conexiones"), connection.name);
                    if label
                        .to_lowercase()
                        .contains(&self.palette_query.to_lowercase())
                        && (ui
                            .add_sized([ui.available_width(), 32.0], egui::Button::new(label))
                            .clicked()
                            || enter && action.is_none())
                    {
                        action = Some(Action::Open(connection.clone()));
                    }
                }
                for (id, name, enabled, _) in self.extensions.shortcuts() {
                    let label = format!("{}: {name}", xpeed_i18n::t("Extensiones"));
                    if enabled
                        && label
                            .to_lowercase()
                            .contains(&self.palette_query.to_lowercase())
                        && (ui
                            .add_sized([ui.available_width(), 32.0], egui::Button::new(label))
                            .clicked()
                            || enter && action.is_none())
                    {
                        action = Some(Action::OpenExtension(id));
                    }
                }
            });
        if !open {
            self.show_palette = false;
        }
        if let Some(action) = action {
            self.show_palette = false;
            self.palette_query.clear();
            self.action(action, ctx);
        }
    }
    fn frame(&mut self, ctx: &egui::Context) {
        xpeed_i18n::set_language(self.settings.language);
        self.extensions.language_changed(self.settings.language);
        for session in self.sessions.values_mut() {
            session.poll();
        }
        self.extensions.sessions(
            self.sessions
                .iter()
                .map(|(&id, session)| crate::extensions::session_tools::Target {
                    id,
                    incarnation: session.started,
                    name: self
                        .sources
                        .get(&id)
                        .map_or_else(|| session.title.clone(), |source| source.name.clone()),
                    cwd: session
                        .remote_directory()
                        .or_else(|| session.console.directory())
                        .unwrap_or_else(|| session.cwd.to_string_lossy().into_owned()),
                    connected: !session.exited
                        && (session.remote.is_none() || session.auth_complete),
                    shell: session.shell,
                    ssh: session.remote.clone(),
                    password: self
                        .remote_files
                        .get(&id)
                        .and_then(RemoteExplorer::monitoring_access)
                        .or_else(|| session.saved_password.clone()),
                    hosts: None,
                    console: Some(session.console.clone()),
                })
                .collect(),
        );
        self.extensions.poll();
        self.extensions.poll_git(ctx);
        if let Some(key) = self.extensions.details_requested.take() {
            self.open_extension_details(key);
        }
        if let Some(tile) = workspace::find(&self.tree, Pane::Assistant) {
            self.tree.remove_recursively(tile);
        }
        let fragments: Vec<_> = self
            .sessions
            .values_mut()
            .filter_map(|session| session.chat_selection.take().map(|text| (session.id, text)))
            .collect();
        for (id, text) in fragments {
            self.extensions.insert_selection(id, text);
        }
        self.sync_extension_views();
        if let Some(id) = self.extensions.show_view_requested.take() {
            self.show(Pane::ExtensionView(id));
        }
        if std::mem::take(&mut self.extensions.show_chat_requested)
            && let Some(key) = self.extensions.selected_chat_view()
        {
            self.show(Pane::ExtensionView(key));
        }
        for activity in self.extensions.take_activity() {
            self.log(activity);
        }
        self.window_chrome(ctx);
        let palette = self.palette();
        for session in self.sessions.values_mut() {
            session.palette = palette;
        }
        let mut file_activity = Vec::new();
        for (&id, remote) in &mut self.remote_files {
            remote.poll();
            let name = self.sources.get(&id).map_or("SFTP", |s| s.name.as_str());
            file_activity.extend(
                remote
                    .activity
                    .drain(..)
                    .map(|message| format!("{name}: {message}")),
            );
        }
        for message in file_activity {
            self.log(message);
        }
        for (&id, session) in &self.sessions {
            if session.auth_complete
                && !session.exited
                && let Some(remote) = self.remote_files.get_mut(&id)
                && let Some(path) = session.remote_directory()
            {
                remote.follow(&path);
            }
        }
        for (&id, session) in &mut self.sessions {
            if let Some(password) = session.submitted_password.take() {
                if let Some(remote) = self.remote_files.get_mut(&id) {
                    remote.set_password(password.clone());
                }
                let remembered = !self.ephemeral
                    && session
                        .remote
                        .as_ref()
                        .and_then(crate::credentials::load)
                        .is_some_and(|saved| saved == password);
                if remembered {
                    self.access_handled.insert(id);
                } else {
                    self.access_handled.remove(&id);
                    self.access_candidates.insert(id, password);
                }
            }
            if self.remote_files.get(&id).is_some_and(|r| r.authenticated)
                || session.has_shell_prompt()
            {
                if !session.auth_complete
                    && let Some(remote) = self.remote_files.get_mut(&id)
                    && !remote.loading
                    && remote.error.is_some()
                {
                    remote.refresh();
                }
                session.auth_complete = true;
                if self.access_candidates.contains_key(&id)
                    && !self.access_handled.contains(&id)
                    && self.access_prompt.is_none()
                    && !self.ephemeral
                {
                    self.access_prompt = Some(id);
                }
            }
            if session.exited {
                self.access_candidates.remove(&id);
            }
        }
        self.flush_commands();
        self.reconnect_sessions(ctx);
        self.sync_files();
        self.apply_zoom();
        self.sync_monitor(ctx);
        self.shortcuts(ctx);
        self.toolbar(ctx);
        self.status(ctx);
        self.activity_rail(ctx);
        self.sidebar_panel(ctx);
        self.central(ctx);
        self.overlays(ctx);
        self.extensions.review_ui(ctx);
        self.productivity.show(
            ctx,
            &mut self.settings,
            &mut self.connections,
            &mut self.remote_files,
            &self.sources,
            self.ephemeral,
        );
        if std::mem::take(&mut self.productivity.preferences_changed) {
            self.apply_preferences();
            self.changed();
        }
        let p = self.palette();
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("desktop_outer_border"),
        ))
        .rect_stroke(
            ctx.content_rect().shrink(0.8),
            10,
            Stroke::new(1.5_f32, p.muted.gamma_multiply(0.65)),
            egui::StrokeKind::Inside,
        );
        self.flush_commands();
        // Closing a dock tab removes its session even when initiated by the library.
        let removed: Vec<_> = self
            .sources
            .keys()
            .filter(|&&id| workspace::find(&self.tree, Pane::Terminal(id)).is_none())
            .copied()
            .collect();
        for id in removed {
            self.sessions.remove(&id);
            self.sources.remove(&id);
            if self.focused == Some(id) {
                self.focused = self.sources.keys().next_back().copied();
            }
            self.changed();
        }
        if self.layout_dirty && self.edited_at.elapsed() > Duration::from_millis(700) {
            self.save_layout();
        }
        if let Some(path) = &self.capture {
            let screenshots = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::Screenshot { image, .. } = e {
                            Some(image.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            });
            for image in screenshots {
                if let Err(error) = save_capture(path, &image) {
                    eprintln!("Capture failed: {error}");
                    std::process::exit(1);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if !self.capture_requested && self.created.elapsed() > Duration::from_secs(3) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.capture_requested = true;
            }
        }
        ctx.request_repaint_after(Duration::from_millis(
            if self.remote_files.values().any(RemoteExplorer::busy)
                || self.extensions.polling_tools()
            {
                50
            } else {
                1000
            },
        ));
    }
}
impl eframe::App for Xpeed {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.capture.is_some() {
            input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. }
                )
            });
            input.events.push(egui::Event::PointerGone);
        }
    }
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.window_size.update(ctx, frame);
        let start = Instant::now();
        self.frame(ctx);
        self.frame_cost_ms =
            self.frame_cost_ms * 0.9 + start.elapsed().as_secs_f32() * 1000.0 * 0.1;
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush_commands();
        self.extensions.flush_chat_states();
        self.save_layout();
    }
}

struct WorkspaceUi<'a> {
    sessions: &'a mut BTreeMap<u64, Session>,
    sources: &'a BTreeMap<u64, Connection>,
    connections: &'a ConnectionStore,
    remote_files: &'a mut BTreeMap<u64, RemoteExplorer>,
    assistant: &'a mut Assistant,
    extensions: &'a mut crate::extensions::Extensions,
    terminal_context: Option<&'a str>,
    history: &'a History,
    activity: &'a [String],
    focused: &'a mut Option<u64>,
    palette: Palette,
    font_size: f32,
    query: &'a str,
    search: &'a str,
    keyboard: bool,
    actions: &'a mut Vec<Action>,
    edited: &'a mut bool,
}
impl WorkspaceUi<'_> {
    fn server_info_ui(&mut self, ui: &mut egui::Ui) {
        let Some(id) = *self.focused else {
            ui.label(xpeed_i18n::t(
                "Selecciona una conexión para ver su información.",
            ));
            return;
        };
        let Some(source) = self.sources.get(&id) else {
            ui.label(xpeed_i18n::t("No hay una conexión activa."));
            return;
        };
        ui.heading(&source.name);
        match &source.target {
            Target::Ssh(config) => {
                ui.label(xpeed_i18n::format(
                    "Servidor: {}",
                    &[("0", config.host.to_string())],
                ));
                ui.label(xpeed_i18n::format(
                    "Puerto: {}",
                    &[("0", format!("{}", config.port))],
                ));
                ui.label(xpeed_i18n::format(
                    "Usuario: {}",
                    &[("0", config.user.to_string())],
                ));
            }
            Target::Local { shell, .. } => {
                ui.label(xpeed_i18n::format(
                    "Terminal local: {}",
                    &[("0", shell.name().to_string())],
                ));
            }
        }
        if let Some(session) = self.sessions.get(&id) {
            ui.label(if session.exited {
                xpeed_i18n::t("Sesión finalizada")
            } else if session.remote.is_some() && !session.auth_complete {
                xpeed_i18n::t("Conectando…")
            } else {
                xpeed_i18n::t("Sesión activa")
            });
            ui.label(xpeed_i18n::format(
                "Tiempo abierto: {} min",
                &[("0", format!("{}", session.started.elapsed().as_secs() / 60))],
            ));
            if let Some(error) = &session.error {
                ui.add(egui::Label::new(error).wrap());
            }
        }
        if let Some(remote) = self.remote_files.get(&id) {
            ui.separator();
            ui.add(
                egui::Label::new(xpeed_i18n::format(
                    "Directorio: {}",
                    &[("0", remote.path.to_string())],
                ))
                .wrap(),
            );
            ui.label(xpeed_i18n::format(
                "Elementos: {}",
                &[("0", format!("{}", remote.entries.len()))],
            ));
            if remote.busy() {
                ui.label(xpeed_i18n::t("Archivos: cargando…"));
            }
            if let Some(error) = &remote.error {
                ui.add(egui::Label::new(error).wrap());
            }
        }
    }
    fn connections_ui(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        egui::Frame::new().inner_margin(14).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("connection_list")
                .show(ui, |ui| {
                    let mut any = false;
                    let mut grouped: Vec<_> = self.connections.saved.iter().enumerate().collect();
                    grouped.sort_by_key(|(_, c)| {
                        self.connections
                            .groups
                            .get(&connection_key(c))
                            .cloned()
                            .unwrap_or_default()
                    });
                    let mut last_group = None;
                    for (index, connection) in grouped {
                        if !format!("{} {}", connection.name, connection.description())
                            .to_lowercase()
                            .contains(&self.query.to_lowercase())
                        {
                            continue;
                        }
                        any = true;
                        let group = self
                            .connections
                            .groups
                            .get(&connection_key(connection))
                            .cloned()
                            .unwrap_or_default();
                        if last_group.as_ref() != Some(&group) {
                            ui.add_space(6.0);
                            if !group.is_empty() {
                                ui.strong(&group);
                            }
                            last_group = Some(group);
                        }
                        let row =
                            connection_row(ui, &connection.name, &connection.description(), p);
                        if row.clicked() {
                            self.actions.push(Action::Open(connection.clone()));
                        }
                        row.context_menu(|ui| {
                            if ui.button(xpeed_i18n::t("Editar conexión")).clicked() {
                                self.actions.push(Action::EditConnection(index));
                                ui.close();
                            }
                            if ui
                                .button(xpeed_i18n::t("Eliminar perfil guardado"))
                                .clicked()
                            {
                                self.actions.push(Action::DeleteConnection(index));
                                ui.close();
                            }
                            if matches!(connection.target, Target::Ssh(_))
                                && ui
                                    .button(xpeed_i18n::t("Olvidar contraseña guardada"))
                                    .clicked()
                            {
                                self.actions.push(Action::ForgetAccess(index));
                                ui.close();
                            }
                        });
                    }
                    if !any {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(if self.query.is_empty() {
                                xpeed_i18n::t("Tus conexiones SSH y locales aparecerán aquí.")
                            } else {
                                xpeed_i18n::t("Sin coincidencias.")
                            })
                            .size(11.0)
                            .color(p.dim),
                        );
                    }
                    ui.add_space(12.0);
                    if ui
                        .add_sized(
                            [ui.available_width(), 32.0],
                            egui::Button::new(xpeed_i18n::t("+ Nueva conexión")),
                        )
                        .clicked()
                    {
                        self.actions.push(Action::NewConnection);
                    }
                });
        });
    }
    fn files_ui(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let Some(id) = *self.focused else {
            ui.label(xpeed_i18n::t("Selecciona una conexión SSH."));
            return;
        };
        let Some(remote) = self.remote_files.get_mut(&id) else {
            ui.label(xpeed_i18n::t(
                "El explorador pertenece a una conexión remota.",
            ));
            return;
        };
        egui::Frame::new().inner_margin(8).show(ui, |ui| {
            crate::file_explorer::show(
                ui,
                remote,
                self.sources.get(&id).map_or("SFTP", |s| s.name.as_str()),
                id,
                p,
            );
        });
    }
    fn commands_ui(&mut self, ui: &mut egui::Ui) {
        let Some(id) = *self.focused else {
            ui.label(xpeed_i18n::t(
                "Los comandos de la conexión activa aparecerán aquí.",
            ));
            return;
        };
        let Some(source) = self.sources.get(&id) else {
            return;
        };
        let prefix = format!("{} :: ", connection_key(source));
        let bounds = ui.available_rect_before_wrap();
        let mut records: Vec<_> = self
            .history
            .commands
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .flat_map(|(_, records)| records.iter())
            .collect();
        records.sort_by_key(|record| record.time);
        egui::ScrollArea::vertical().id_salt(("commands", id)).stick_to_bottom(true).show(ui, |ui| {
            ui.set_min_width((bounds.width() - 16.0).max(0.0));
            for record in &records {
                let row = ui.add(egui::Label::new(RichText::new(&record.text).monospace().size(11.0)).wrap().sense(egui::Sense::click()));
                if row.clicked() { self.actions.push(Action::InsertCommand(id, record.text.clone())); }
                row.on_hover_text(xpeed_i18n::t("Click to insert into the terminal. Right-click to copy.")).context_menu(|ui| {
                    if ui.button(xpeed_i18n::t("Copiar comando")).clicked() { ui.ctx().copy_text(record.text.clone()); ui.close(); }
                });
            }
            if records.is_empty() { ui.label(RichText::new(xpeed_i18n::t("Sin comandos registrados. Se detectan los prompts estándar de PowerShell, CMD y shells Unix.")).small()); }
            ui.add_space(40.0);

        });
        let rect = egui::Rect::from_min_size(
            bounds.right_bottom() - egui::vec2(40.0, 40.0),
            egui::vec2(32.0, 32.0),
        );
        let response = ui.interact(rect, ui.id().with("clear_commands"), egui::Sense::click());
        let visuals = ui.style().interact(&response);
        ui.painter().rect_filled(rect, 10.0, visuals.bg_fill);
        crate::svg_icons::draw(
            ui.painter(),
            rect.shrink(6.0),
            crate::svg_icons::Icon::Trash,
            visuals.text_color(),
        );
        if response.on_hover_text(xpeed_i18n::t("Borrar")).clicked() {
            self.actions.push(Action::ClearCommands);
        }
    }
}
impl Behavior<Pane> for WorkspaceUi<'_> {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        match pane {
            Pane::Connections => xpeed_i18n::t("Conexiones").into(),
            Pane::Files => xpeed_i18n::t("Archivos").into(),
            Pane::Activity => xpeed_i18n::t("Actividad").into(),
            Pane::Commands => xpeed_i18n::t("Comandos").into(),
            Pane::Assistant => xpeed_i18n::t("IA").into(),
            Pane::AppInfo => xpeed_i18n::t("Acerca de").into(),
            Pane::ServerInfo => xpeed_i18n::t("Servidor").into(),
            Pane::ExtensionView(id) => self.extensions.view_title(*id).into(),
            Pane::ExtensionDetails(key) => xpeed_i18n::format(
                "Extensión: {}",
                &[("0", self.extensions.detail_title(*key).to_string())],
            )
            .into(),
            Pane::Extensions => xpeed_i18n::t("Extensiones").into(),
            Pane::Welcome => xpeed_i18n::t("Inicio").into(),
            Pane::Terminal(id) => self
                .sources
                .get(id)
                .map(|c| {
                    format!(
                        "{}  {} · {id:02}",
                        if matches!(c.target, Target::Ssh(_)) {
                            "SSH"
                        } else {
                            ">_"
                        },
                        c.name
                    )
                })
                .unwrap_or_else(|| xpeed_i18n::t("Terminal").into())
                .into(),
        }
    }
    fn pane_ui(&mut self, ui: &mut egui::Ui, _tile: TileId, pane: &mut Pane) -> UiResponse {
        let p = self.palette;
        ui.painter().rect_filled(
            ui.max_rect(),
            egui::CornerRadius {
                nw: 0,
                ne: 0,
                sw: 10,
                se: 10,
            },
            if matches!(pane, Pane::Terminal(_)) {
                p.terminal
            } else {
                p.panel
            },
        );
        match *pane {
            Pane::Connections => self.connections_ui(ui),
            Pane::Files => self.files_ui(ui),
            Pane::Commands => {
                egui::Frame::new()
                    .inner_margin(10)
                    .show(ui, |ui| self.commands_ui(ui));
            }
            Pane::Assistant => {
                egui::Frame::new().inner_margin(12).show(ui, |ui| {
                    if self.extensions.chat_ui(ui, self.terminal_context) {
                        self.assistant.ui(ui);
                    }
                });
            }
            Pane::AppInfo => {
                let active = self
                    .sessions
                    .values()
                    .filter(|session| !session.exited)
                    .count();
                crate::about::show(ui, p, active);
            }
            Pane::ServerInfo => {
                egui::Frame::new()
                    .inner_margin(12)
                    .show(ui, |ui| self.server_info_ui(ui));
            }
            Pane::ExtensionDetails(key) => self.extensions.details_ui(ui, key),
            Pane::ExtensionView(id) => {
                egui::Frame::new()
                    .inner_margin(12)
                    .show(ui, |ui| self.extensions.view_ui(ui, id));
            }
            Pane::Extensions => {
                egui::Frame::new()
                    .inner_margin(12)
                    .show(ui, |ui| self.extensions.manager_ui(ui));
            }
            Pane::Activity => {
                egui::Frame::new().inner_margin(12).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("activity_scroll")
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in self.activity {
                                ui.add(
                                    egui::Label::new(RichText::new(line).size(10.0).color(p.muted))
                                        .wrap(),
                                );
                            }
                        });
                });
            }
            Pane::Welcome => {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() * 0.20).max(20.0));
                    brand_logo(ui, 48.0, p.accent);
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(xpeed_i18n::t("Todas tus conexiones. Un solo espacio."))
                            .color(p.muted),
                    );
                    ui.add_space(28.0);
                    if ui
                        .add_sized(
                            [230.0, 40.0],
                            egui::Button::new(xpeed_i18n::t("+  Terminal local")),
                        )
                        .clicked()
                    {
                        self.actions.push(Action::Local(Profile::default_shell()));
                    }
                    if ui
                        .add_sized(
                            [230.0, 40.0],
                            egui::Button::new(xpeed_i18n::t("↗  Nueva conexión")),
                        )
                        .clicked()
                    {
                        self.actions.push(Action::NewConnection);
                    }
                    ui.add_space(25.0);
                    if !self.connections.recent.is_empty() {
                        ui.label(
                            RichText::new(xpeed_i18n::t("CONEXIONES RECIENTES"))
                                .small()
                                .color(p.muted),
                        );
                    }
                    for connection in self.connections.recent.iter().take(6) {
                        if ui
                            .button(&connection.name)
                            .on_hover_text(connection.description())
                            .clicked()
                        {
                            self.actions.push(Action::Open(connection.clone()));
                        }
                    }
                    ui.add_space(20.0);
                    ui.label(
                        RichText::new(xpeed_i18n::t(
                            "Ctrl+Shift+T  ·  terminal local     Ctrl+Shift+N  ·  conexión",
                        ))
                        .size(11.0)
                        .color(p.dim),
                    );
                });
            }
            Pane::Terminal(id) => {
                if let Some(session) = self.sessions.get_mut(&id) {
                    let has_status = session.exited || session.error.is_some();
                    let area = ui.available_rect_before_wrap();
                    let height_id = ui.id().with(("terminal_status_height", id));
                    let status_height = if has_status {
                        ui.ctx()
                            .data(|data| data.get_temp::<f32>(height_id))
                            .unwrap_or(112.0)
                            .min(area.height() * 0.5)
                    } else {
                        0.0
                    };
                    let terminal_rect = egui::Rect::from_min_max(
                        area.min,
                        egui::pos2(area.right(), area.bottom() - status_height),
                    );
                    let focused_widget = ui.ctx().memory(|m| m.focused());
                    let keyboard = self.keyboard
                        && (!ui.ctx().wants_keyboard_input() || focused_widget == session.input_id);
                    ui.scope_builder(egui::UiBuilder::new().max_rect(terminal_rect), |ui| {
                        if terminal_view(
                            ui,
                            session,
                            self.font_size,
                            p,
                            *self.focused == Some(id),
                            keyboard,
                            self.search,
                        ) {
                            *self.focused = Some(id);
                        }
                    });
                    if has_status {
                        let status_rect = egui::Rect::from_min_max(
                            egui::pos2(area.left(), terminal_rect.bottom()),
                            area.max,
                        );
                        let footer =
                            ui.scope_builder(egui::UiBuilder::new().max_rect(status_rect), |ui| {
                                ui.set_clip_rect(ui.clip_rect().intersect(status_rect));
                                if terminal_status(ui, session, p) {
                                    self.actions.push(Action::Reconnect(id));
                                }
                            });
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(height_id, footer.response.rect.height())
                        });
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(45.0);
                        ui.label(
                            RichText::new(xpeed_i18n::t("Conexión guardada · Sin conectar"))
                                .size(18.0),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(xpeed_i18n::t(
                                "La distribución se restauró. Conecta cuando estés listo.",
                            ))
                            .color(p.muted),
                        );
                        ui.add_space(15.0);
                        if ui.button(xpeed_i18n::t("Conectar")).clicked() {
                            self.actions.push(Action::Reconnect(id));
                        }
                    });
                }
            }
        }
        UiResponse::None
    }
    fn is_tab_closable(&self, tiles: &Tiles<Pane>, tile: TileId) -> bool {
        matches!(
            tiles.get(tile),
            Some(Tile::Pane(Pane::Terminal(_) | Pane::ExtensionDetails(_)))
        )
    }
    fn on_tab_close(&mut self, tiles: &mut Tiles<Pane>, tile: TileId) -> bool {
        if matches!(tiles.get(tile), Some(Tile::Pane(Pane::ExtensionDetails(_)))) {
            return true;
        }
        if let Some(Tile::Pane(Pane::Terminal(id))) = tiles.get(tile) {
            self.actions.push(Action::Close(*id));
        } else if let Some(Tile::Pane(pane)) = tiles.get(tile) {
            self.actions.push(Action::Hide(*pane));
        }
        false
    }
    fn on_tab_button(
        &mut self,
        tiles: &Tiles<Pane>,
        tile: TileId,
        response: egui::Response,
    ) -> egui::Response {
        if response.clicked()
            && let Some(Tile::Pane(Pane::Terminal(id))) = tiles.get(tile)
        {
            *self.focused = Some(*id);
            response.ctx.memory_mut(|m| {
                if let Some(widget) = self.sessions.get(id).and_then(|s| s.input_id) {
                    m.request_focus(widget);
                } else if let Some(widget) = m.focused() {
                    m.surrender_focus(widget);
                }
            });
        }
        if matches!(
            tiles.get(tile),
            Some(Tile::Pane(
                Pane::Connections | Pane::Files | Pane::Commands | Pane::Activity
            ))
        ) {
            response.on_hover_text(xpeed_i18n::t("Seleccionar panel"))
        } else {
            response.on_hover_text(xpeed_i18n::t(
                "Arrastra esta pestaña para mover o agrupar el panel",
            ))
        }
    }
    fn tab_ui(
        &mut self,
        tiles: &mut Tiles<Pane>,
        ui: &mut egui::Ui,
        widget_id: egui::Id,
        tile: TileId,
        state: &TabState,
    ) -> egui::Response {
        let text = self.tab_title_for_tile(tiles, tile);
        let compact = matches!(
            tiles.get(tile),
            Some(Tile::Pane(
                Pane::Assistant | Pane::AppInfo | Pane::ServerInfo | Pane::ExtensionView(_)
            ))
        );
        let font = if compact {
            FontId::proportional(12.)
        } else {
            egui::TextStyle::Button.resolve(ui.style())
        };
        let galley = text.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, font);
        let width = galley.size().x
            + if compact { 12. } else { 24. }
            + if state.closable { 20.0 } else { 0.0 };
        let (_, rect) = ui.allocate_space(Vec2::new(width, ui.available_height()));
        let navigation = matches!(
            tiles.get(tile),
            Some(Tile::Pane(
                Pane::Connections | Pane::Files | Pane::Commands | Pane::Activity
            ))
        );
        let response = ui
            .interact(
                rect,
                widget_id,
                if navigation {
                    egui::Sense::click()
                } else {
                    egui::Sense::click_and_drag()
                },
            )
            .on_hover_cursor(egui::CursorIcon::Default);
        if !state.is_being_dragged {
            let fill = if state.active {
                self.palette.raised
            } else {
                Color32::TRANSPARENT
            };
            let button = rect.shrink2(Vec2::new(3.0, 4.0));
            ui.painter().rect_filled(button, 6, fill);
            ui.painter().galley(
                egui::pos2(
                    button.left() + if compact { 3. } else { 9. },
                    button.center().y - galley.size().y / 2.0,
                ),
                galley,
                if state.active {
                    self.palette.text
                } else {
                    self.palette.muted
                },
            );
            if state.closable {
                let close_rect = egui::Rect::from_center_size(
                    egui::pos2(button.right() - 11.0, button.center().y),
                    Vec2::splat(14.0),
                );
                let close = ui
                    .interact(close_rect, widget_id.with("close"), egui::Sense::click())
                    .on_hover_text(xpeed_i18n::t("Cerrar terminal"));
                let stroke = Stroke::new(
                    1.3_f32,
                    if close.hovered() {
                        self.palette.accent
                    } else {
                        self.palette.muted
                    },
                );
                let cross = close_rect.shrink(3.0);
                ui.painter()
                    .line_segment([cross.left_top(), cross.right_bottom()], stroke);
                ui.painter()
                    .line_segment([cross.right_top(), cross.left_bottom()], stroke);
                if close.clicked() && self.on_tab_close(tiles, tile) {
                    tiles.remove(tile);
                }
            }
        }
        self.on_tab_button(tiles, tile, response)
    }
    fn top_bar_right_ui(
        &mut self,
        tiles: &Tiles<Pane>,
        ui: &mut egui::Ui,
        _tile: TileId,
        tabs: &egui_tiles::Tabs,
        _offset: &mut f32,
    ) {
        let active = tabs.active.and_then(|tile| tiles.get(tile));
        if let Some(Tile::Pane(pane)) = active
            && !matches!(
                pane,
                Pane::Terminal(_) | Pane::ExtensionDetails(_) | Pane::Welcome
            )
        {
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.add_space(8.0);
            if small_icon_button(ui, Icon::PanelClose, self.palette)
                .on_hover_text(xpeed_i18n::t("Cerrar panel"))
                .clicked()
            {
                self.actions.push(Action::Hide(*pane));
            }
            if small_icon_button(ui, Icon::Expand, self.palette)
                .on_hover_text(xpeed_i18n::t("Expandir / restaurar panel"))
                .clicked()
            {
                self.actions.push(Action::ZoomPanel(*pane));
            }
        }
        if let Some(Tile::Pane(Pane::Terminal(id))) = active {
            let id = *id;
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.add_space(8.0);
            ui.scope(|ui| {
                ui.spacing_mut().interact_size = Vec2::splat(23.0);
                ui.spacing_mut().button_padding = Vec2::new(4.0, 2.0);
                ui.menu_button(RichText::new("···").size(12.0), |ui| {
                    if ui.button(xpeed_i18n::t("Dividir en filas")).clicked() {
                        *self.focused = Some(id);
                        self.actions.push(Action::Split(ContainerKind::Vertical));
                        ui.close();
                    }
                    if ui.button(xpeed_i18n::t("Reconectar")).clicked() {
                        self.actions.push(Action::Reconnect(id));
                        ui.close();
                    }
                    if ui.button(xpeed_i18n::t("Buscar en terminal")).clicked() {
                        self.actions.push(Action::Search);
                        ui.close();
                    }
                });
            });
            if small_icon_button(ui, Icon::Split, self.palette)
                .on_hover_text(xpeed_i18n::t("Dividir en columnas"))
                .clicked()
            {
                *self.focused = Some(id);
                self.actions.push(Action::Split(ContainerKind::Horizontal));
            }
        }
    }
    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        34.0
    }
    fn gap_width(&self, _style: &egui::Style) -> f32 {
        f32::from(PANEL_GAP)
    }
    fn resize_stroke(&self, _style: &egui::Style, _state: egui_tiles::ResizeState) -> Stroke {
        Stroke::NONE
    }
    fn min_size(&self) -> f32 {
        110.0
    }
    fn simplification_options(&self) -> SimplificationOptions {
        SimplificationOptions {
            all_panes_must_have_tabs: true,
            ..Default::default()
        }
    }
    fn tab_bar_color(&self, _visuals: &egui::Visuals) -> Color32 {
        Color32::TRANSPARENT
    }
    fn tab_bg_color(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &Tiles<Pane>,
        _id: TileId,
        state: &TabState,
    ) -> Color32 {
        if state.active {
            self.palette.panel
        } else {
            self.palette.bg
        }
    }
    fn tab_text_color(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &Tiles<Pane>,
        _id: TileId,
        state: &TabState,
    ) -> Color32 {
        if state.active {
            self.palette.accent
        } else {
            self.palette.muted
        }
    }
    fn tab_outline_stroke(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &Tiles<Pane>,
        _id: TileId,
        _state: &TabState,
    ) -> Stroke {
        Stroke::NONE
    }
    fn on_edit(&mut self, _action: EditAction) {
        *self.edited = true;
    }
}

fn paint_divider(
    ctx: &egui::Context,
    painter: &egui::Painter,
    id: egui::Id,
    rect: egui::Rect,
    palette: Palette,
) {
    if !rect.is_positive() {
        return;
    }
    let vertical = rect.height() > rect.width();
    let hit = rect.expand2(if vertical {
        Vec2::new(4.0, 10.0)
    } else {
        Vec2::new(10.0, 4.0)
    });
    let (pressed, down, dragging, pointer) = ctx.input(|i| {
        (
            i.pointer.button_pressed(egui::PointerButton::Primary),
            i.pointer.button_down(egui::PointerButton::Primary),
            i.pointer.is_decidedly_dragging(),
            i.pointer.interact_pos(),
        )
    });
    let active = ctx.data_mut(|data| {
        let mut active = data.get_temp::<bool>(id).unwrap_or(false);
        if pressed {
            active = pointer.is_some_and(|pos| hit.contains(pos));
        }
        if !down {
            active = false;
        }
        data.insert_temp(id, active);
        active
    });
    if active && dragging {
        painter.rect_filled(rect, 1, palette.accent);
        return;
    }
    let hovered = ctx.pointer_hover_pos().is_some_and(|pos| hit.contains(pos));
    let color = if hovered {
        palette.muted
    } else {
        palette.muted.gamma_multiply(0.65)
    };
    for offset in [-4.0, 0.0, 4.0] {
        let point = rect.center()
            + if vertical {
                Vec2::new(0.0, offset)
            } else {
                Vec2::new(offset, 0.0)
            };
        painter.circle_filled(point, 1.0, color);
    }
}

fn paint_dock_dividers(ui: &egui::Ui, tree: &Tree<Pane>, palette: Palette) {
    for (&id, tile) in tree.tiles.iter() {
        let Tile::Container(egui_tiles::Container::Linear(linear)) = tile else {
            continue;
        };
        if !tree.tiles.is_visible(id) {
            continue;
        }
        let Some(parent) = tree.tiles.rect(id) else {
            continue;
        };
        let children: Vec<_> = linear
            .children
            .iter()
            .filter(|&&id| tree.tiles.is_visible(id))
            .filter_map(|&id| tree.tiles.rect(id))
            .collect();
        for (index, pair) in children.windows(2).enumerate() {
            let rect = match linear.dir {
                egui_tiles::LinearDir::Horizontal => {
                    let x = (pair[0].right() + pair[1].left()) / 2.0;
                    egui::Rect::from_min_max(
                        egui::pos2(x - 1.0, parent.top() + 10.0),
                        egui::pos2(x + 1.0, parent.bottom() - 10.0),
                    )
                }
                egui_tiles::LinearDir::Vertical => {
                    let y = (pair[0].bottom() + pair[1].top()) / 2.0;
                    egui::Rect::from_min_max(
                        egui::pos2(parent.left() + 10.0, y - 1.0),
                        egui::pos2(parent.right() - 10.0, y + 1.0),
                    )
                }
            };
            paint_divider(
                ui.ctx(),
                ui.painter(),
                ui.id().with((id, "divider_drag", index)),
                rect,
                palette,
            );
        }
    }
}

fn local_connection(shell: Profile, cwd: &std::path::Path) -> Connection {
    Connection {
        name: shell.name().into(),
        target: Target::Local {
            shell,
            directory: cwd.display().to_string(),
        },
    }
}
fn field(ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str) -> egui::Response {
    ui.label(label);
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    )
}
fn status_dot(ui: &mut egui::Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(9.0), egui::Sense::hover());
    ui.painter().circle_filled(r.center(), 2.8, color);
}
fn connection_row(ui: &mut egui::Ui, name: &str, detail: &str, p: Palette) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 47.0), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 4, p.raised);
    }
    draw_icon(
        ui.painter(),
        egui::Rect::from_min_size(rect.min + Vec2::new(6.0, 12.0), Vec2::splat(20.0)),
        Icon::Connection,
        p.accent,
    );
    let pos = rect.min + Vec2::new(34.0, 7.0);
    let painter = ui.painter().with_clip_rect(rect.shrink(3.0));
    painter.text(
        pos,
        Align2::LEFT_TOP,
        name,
        FontId::proportional(12.0),
        p.text,
    );
    painter.text(
        pos + Vec2::new(0.0, 19.0),
        Align2::LEFT_TOP,
        detail,
        FontId::proportional(10.0),
        p.dim,
    );
    response
}
#[derive(Clone, Copy)]
enum Icon {
    SidebarLeft,
    SidebarRight,
    Minimize,
    Maximize,
    Close,
    Connection,
    Workspace,
    Split,
    Expand,
    PanelClose,
    Settings,
    Extensions,
}
fn brand_logo(ui: &mut egui::Ui, height: f32, color: Color32) {
    let (rect, _) =
        ui.allocate_exact_size(crate::svg_icons::logo_size(height), egui::Sense::hover());
    crate::svg_icons::draw(ui.painter(), rect, crate::svg_icons::Icon::Logo, color);
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, p: Palette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(36.0), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 6, p.raised);
    }
    draw_icon(
        ui.painter(),
        rect.shrink(8.0),
        icon,
        if response.hovered() {
            p.accent
        } else {
            p.muted
        },
    );
    response
}
fn small_icon_button(ui: &mut egui::Ui, icon: Icon, p: Palette) -> egui::Response {
    let (allocated, response) = ui.allocate_exact_size(Vec2::splat(23.0), egui::Sense::click());
    let rect = egui::Rect::from_center_size(allocated.center(), Vec2::splat(23.0));
    ui.painter().rect_filled(
        rect.shrink(1.0),
        4,
        if response.hovered() {
            p.raised
        } else {
            Color32::TRANSPARENT
        },
    );
    draw_icon(
        ui.painter(),
        rect.shrink(1.0),
        icon,
        if response.hovered() {
            p.accent
        } else {
            p.muted
        },
    );
    response
}
fn title_icon_button(ui: &mut egui::Ui, icon: Icon, p: Palette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(32.0, 27.0), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 5, p.raised);
    }
    let icon_size = 22.0;
    draw_icon(
        ui.painter(),
        egui::Rect::from_center_size(rect.center(), Vec2::splat(icon_size)),
        icon,
        if response.hovered() {
            p.accent
        } else {
            p.muted
        },
    );
    response
}
fn draw_icon(painter: &egui::Painter, r: egui::Rect, icon: Icon, color: Color32) {
    let s = Stroke::new(1.3_f32, color);
    let c = r.center();
    match icon {
        Icon::SidebarLeft => {
            crate::svg_icons::draw(painter, r, crate::svg_icons::Icon::SidebarLeft, color)
        }
        Icon::SidebarRight => {
            crate::svg_icons::draw(painter, r, crate::svg_icons::Icon::SidebarRight, color)
        }
        Icon::Minimize => {
            crate::svg_icons::draw(painter, r, crate::svg_icons::Icon::Minimize, color)
        }
        Icon::Maximize => {
            crate::svg_icons::draw(painter, r, crate::svg_icons::Icon::Maximize, color)
        }
        Icon::Close => crate::svg_icons::draw(painter, r, crate::svg_icons::Icon::Close, color),
        Icon::Connection => {
            crate::svg_icons::draw(painter, r, crate::svg_icons::Icon::Connection, color)
        }
        Icon::PanelClose => {
            crate::svg_icons::draw(painter, r, crate::svg_icons::Icon::Close, color)
        }
        Icon::Expand => crate::svg_icons::draw(painter, r, crate::svg_icons::Icon::Maximize, color),
        Icon::Split => {
            painter.rect_stroke(r, 3, s, egui::StrokeKind::Inside);
            painter.line_segment([egui::pos2(c.x, r.top()), egui::pos2(c.x, r.bottom())], s);
        }
        Icon::Extensions => {
            crate::svg_icons::draw(painter, r, crate::svg_icons::Icon::Extensions, color)
        }
        Icon::Workspace => {
            crate::svg_icons::draw(painter, r, crate::svg_icons::Icon::Workspace, color)
        }
        Icon::Settings => {
            crate::svg_icons::draw(painter, r, crate::svg_icons::Icon::Settings, color)
        }
    }
}
fn save_capture(
    path: &std::path::Path,
    image: &egui::ColorImage,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        image.size[0] as u32,
        image.size[1] as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(
        &image
            .pixels
            .iter()
            .flat_map(|p| p.to_array())
            .collect::<Vec<_>>(),
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

fn terminal_status(ui: &mut egui::Ui, session: &Session, p: Palette) -> bool {
    if !session.exited && session.error.is_none() {
        return false;
    }
    let mut reconnect = false;
    egui::Frame::new()
        .fill(p.raised)
        .corner_radius(8.0)
        .inner_margin(10.0)
        .outer_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 4.0).max(0.0));
            ui.horizontal(|ui| {
                let status = if session.reconnecting {
                    "Reconnecting"
                } else if session.exited {
                    "Session disconnected"
                } else {
                    "Session notice"
                };
                ui.label(RichText::new(xpeed_i18n::t(status)).strong().color(p.muted));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if session.exited && !session.reconnecting {
                        reconnect = ui.button(xpeed_i18n::t("Reconnect")).clicked();
                    } else if session.reconnecting {
                        ui.spinner();
                    }
                });
            });
            if session.exited {
                ui.add(
                    egui::Label::new(
                        RichText::new(xpeed_i18n::t("Terminal output is preserved."))
                            .small()
                            .color(p.muted),
                    )
                    .wrap(),
                );
            }
            if let Some(error) = &session.error {
                egui::CollapsingHeader::new(xpeed_i18n::t("Technical details"))
                    .id_salt(("terminal_status_details", session.id))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(error).small().color(p.muted)).wrap(),
                        );
                        if ui.small_button(xpeed_i18n::t("Copy details")).clicked() {
                            ui.ctx().copy_text(error.clone());
                        }
                    });
            }
        });
    ui.add_space(6.0);
    reconnect
}

fn connection_key(connection: &Connection) -> String {
    format!("{} | {}", connection.name, connection.description())
}
