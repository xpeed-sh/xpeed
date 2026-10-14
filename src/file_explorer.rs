use crate::{
    palette::Palette,
    sftp::{Entry, Mutation, RemoteExplorer},
    svg_icons::{self, Icon},
};
use eframe::egui::{self, Align2, FontId, RichText, Stroke, Vec2};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Browser {
    pub selected: BTreeMap<String, bool>,
    pub grid: bool,
    cut: Vec<(String, bool)>,
    dialog: Option<Dialog>,
    focus_name: bool,
    upload: Option<(crate::upload_picker::Picker, String)>,
}
impl Browser {
    pub fn has_dialog(&self) -> bool {
        self.dialog.is_some() || self.upload.is_some()
    }
}
enum Dialog {
    Name {
        parent: String,
        old: Option<String>,
        folder: bool,
        name: String,
    },
    Delete(Vec<(String, bool)>),
    Archive {
        sources: Vec<String>,
        destination: String,
        kind: String,
    },
}
enum Event {
    Back,
    Expand(String),
    Navigate(String),
    Refresh,
    Edit(String),
    Download(String),
    Mutate(Vec<Mutation>),
    Archive(Vec<String>, String, String),
}

fn join(parent: &str, name: &str) -> String {
    format!("{}/{}", parent.trim_end_matches('/'), name)
}
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0', '\r', '\n'])
}
fn size(bytes: u64) -> String {
    if bytes < 1000 {
        format!("{bytes} B")
    } else if bytes < 1_000_000 {
        format!("{:.1} KB", bytes as f64 / 1000.0)
    } else if bytes < 1_000_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    }
}
fn filename(ui: &egui::Ui, rect: egui::Rect, text: &str, p: Palette, centered: bool) {
    let mut job = egui::text::LayoutJob::simple(
        text.into(),
        FontId::proportional(if centered { 11. } else { 13. }),
        p.text,
        rect.width(),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    let x = if centered {
        rect.center().x - galley.size().x / 2.
    } else {
        rect.left()
    };
    ui.painter().with_clip_rect(rect).galley(
        egui::pos2(x, rect.center().y - galley.size().y / 2.),
        galley,
        p.text,
    );
}
fn icon(entry: &Entry) -> Icon {
    if entry.directory {
        return Icon::Folder;
    }
    match entry
        .name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "avif" | "tif"
        | "tiff" | "heic" => Icon::Image,
        "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" | "opus" | "aiff" | "wma" => Icon::Audio,
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" | "mpeg" | "mpg" | "wmv" => Icon::Video,
        _ => Icon::File,
    }
}
#[derive(Clone, Copy)]
enum Tool {
    More,
    Grid,
    List,
    Trash,
    Back,
    Copy,
    Cut,
    Paste,
}
fn tool(ui: &mut egui::Ui, kind: Tool, hint: &str, p: Palette) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(27.0), egui::Sense::hover());
    let response = ui.interact(rect, ui.id().with(hint), egui::Sense::click());
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 6, p.raised);
    }
    let c = rect.center();
    let stroke = Stroke::new(
        1.4_f32,
        if ui.is_enabled() {
            p.muted
        } else {
            p.dim.gamma_multiply(0.5)
        },
    );
    let line = |a: [f32; 2], b: [f32; 2]| {
        painter.line_segment(
            [c + Vec2::new(a[0], a[1]), c + Vec2::new(b[0], b[1])],
            stroke,
        );
    };
    match kind {
        Tool::More => {
            for x in [-5., 0., 5.] {
                painter.circle_filled(c + Vec2::new(x, 0.), 1.2, stroke.color);
            }
        }
        Tool::Grid => {
            for x in [-6., 2.] {
                for y in [-6., 2.] {
                    painter.rect_stroke(
                        egui::Rect::from_min_size(c + Vec2::new(x, y), Vec2::splat(5.)),
                        1,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
        Tool::List => {
            for y in [-5., 0., 5.] {
                line([-6., y], [6., y]);
            }
        }
        Tool::Back => {
            line([6., 0.], [-6., 0.]);
            line([-6., 0.], [-1., -5.]);
            line([-6., 0.], [-1., 5.]);
        }
        Tool::Trash => {
            line([-6., -4.], [6., -4.]);
            line([-3., -7.], [3., -7.]);
            line([-4., -3.], [-3., 7.]);
            line([-3., 7.], [3., 7.]);
            line([3., 7.], [4., -3.]);
            line([-1., -1.], [-1., 4.]);
            line([1., -1.], [1., 4.]);
        }
        Tool::Copy => {
            painter.rect_stroke(
                egui::Rect::from_min_size(c + Vec2::new(-3., -3.), Vec2::splat(9.)),
                2,
                stroke,
                egui::StrokeKind::Inside,
            );
            line([-6., 3.], [-6., -6.]);
            line([-6., -6.], [3., -6.]);
        }
        Tool::Cut => {
            for y in [-4., 4.] {
                painter.circle_stroke(c + Vec2::new(-4., y), 2., stroke);
            }
            line([-2., -3.], [6., 5.]);
            line([-2., 3.], [6., -5.]);
        }
        Tool::Paste => {
            painter.rect_stroke(
                egui::Rect::from_min_size(c + Vec2::new(-5., -5.), Vec2::new(10., 12.)),
                2,
                stroke,
                egui::StrokeKind::Inside,
            );
            line([-2., -7.], [2., -7.]);
            line([-2., -7.], [-2., -3.]);
            line([-2., -3.], [2., -3.]);
            line([2., -3.], [2., -7.]);
        }
    }
    response.on_hover_text(hint)
}
fn select_all(browser: &mut Browser, parent: &str, entries: &[Entry]) {
    for entry in entries {
        browser
            .selected
            .insert(join(parent, &entry.name), entry.directory);
    }
}
fn copy_paths(ui: &egui::Ui, browser: &Browser) {
    ui.ctx().copy_text(
        browser
            .selected
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
fn cut_selection(browser: &mut Browser) {
    browser.cut = browser
        .selected
        .iter()
        .filter(|(path, _)| {
            !browser
                .selected
                .keys()
                .any(|parent| *path != parent && path.starts_with(&format!("{parent}/")))
        })
        .map(|(path, directory)| (path.clone(), *directory))
        .collect();
}
fn paste(browser: &mut Browser, parent: &str, events: &mut Vec<Event>) {
    let operations: Vec<_> = browser
        .cut
        .iter()
        .filter_map(|(path, _)| {
            let dest = join(parent, path.rsplit('/').next().unwrap_or(""));
            (dest != *path).then(|| Mutation::Rename(path.clone(), dest))
        })
        .collect();
    if !operations.is_empty() {
        events.push(Event::Mutate(operations));
        browser.cut.clear();
    }
}
fn menu(
    ui: &mut egui::Ui,
    browser: &mut Browser,
    parent: &str,
    entries: &[Entry],
    events: &mut Vec<Event>,
) {
    for (label, folder) in [
        (xpeed_i18n::t("Nuevo archivo"), false),
        (xpeed_i18n::t("Nueva carpeta"), true),
    ] {
        if ui.button(label).clicked() {
            browser.dialog = Some(Dialog::Name {
                parent: parent.into(),
                old: None,
                folder,
                name: String::new(),
            });
            browser.focus_name = true;
            ui.close();
        }
    }
    if ui.button(xpeed_i18n::t("Upload files…")).clicked() {
        browser.upload = Some((Default::default(), parent.into()));
        ui.close();
    }
    ui.separator();
    let has_selection = !browser.selected.is_empty();
    if ui
        .button(xpeed_i18n::t(if has_selection {
            "Deseleccionar todo"
        } else {
            "Seleccionar todo"
        }))
        .clicked()
    {
        if has_selection {
            browser.selected.clear();
        } else {
            select_all(browser, parent, entries);
        }
        ui.close();
    }
    if ui
        .add_enabled(
            !browser.cut.is_empty(),
            egui::Button::new(xpeed_i18n::t("Pegar aquí (mover)")),
        )
        .clicked()
    {
        paste(browser, parent, events);
        ui.close();
    }
    if ui.button(xpeed_i18n::t("Refrescar")).clicked() {
        events.push(Event::Refresh);
        ui.close();
    }
}
fn context(
    ui: &mut egui::Ui,
    browser: &mut Browser,
    path: &str,
    entry: &Entry,
    events: &mut Vec<Event>,
) {
    if !browser.selected.contains_key(path) {
        browser.selected.clear();
        browser.selected.insert(path.into(), entry.directory);
    }
    if entry.directory && ui.button(xpeed_i18n::t("Abrir carpeta")).clicked() {
        events.push(Event::Navigate(path.into()));
        ui.close();
    }
    if !entry.directory {
        if ui.button(xpeed_i18n::t("Edit remote file")).clicked() {
            events.push(Event::Edit(path.into()));
            ui.close();
        }
        if ui.button(xpeed_i18n::t("Download file...")).clicked() {
            events.push(Event::Download(path.into()));
            ui.close();
        }
    }
    if ui.button(xpeed_i18n::t("Copiar rutas")).clicked() {
        copy_paths(ui, browser);
        ui.close();
    }
    if ui.button(xpeed_i18n::t("Cortar para mover")).clicked() {
        cut_selection(browser);
        ui.close();
    }
    ui.menu_button(xpeed_i18n::t("Compress…"), |ui| {
        for kind in ["zip", "tar.gz", "rar"] {
            if ui.button(kind.to_uppercase()).clicked() {
                browser.dialog = Some(Dialog::Archive {
                    sources: browser.selected.keys().cloned().collect(),
                    destination: format!("archive.{kind}"),
                    kind: kind.into(),
                });
                ui.close();
            }
        }
    });
    if !entry.directory
        && [
            ".zip", ".rar", ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tar.xz",
        ]
        .iter()
        .any(|suffix| entry.name.to_lowercase().ends_with(suffix))
        && ui.button(xpeed_i18n::t("Extract to folder…")).clicked()
    {
        browser.dialog = Some(Dialog::Archive {
            sources: vec![path.into()],
            destination: format!("{}-extracted", entry.name),
            kind: "extract".into(),
        });
        ui.close();
    }
    if ui
        .add_enabled(
            browser.selected.len() == 1,
            egui::Button::new(xpeed_i18n::t("Renombrar")),
        )
        .clicked()
    {
        browser.dialog = Some(Dialog::Name {
            parent: path.rsplit_once('/').map_or("/", |p| p.0).into(),
            old: Some(path.into()),
            folder: entry.directory,
            name: entry.name.clone(),
        });
        browser.focus_name = true;
        ui.close();
    }
    ui.separator();
    if ui.button(xpeed_i18n::t("Eliminar…")).clicked() {
        browser.dialog = Some(Dialog::Delete(
            browser
                .selected
                .iter()
                .map(|(p, d)| (p.clone(), *d))
                .collect(),
        ));
        ui.close();
    }
}
fn visible_range(
    viewport: egui::Rect,
    top: f32,
    height: f32,
    count: usize,
) -> std::ops::Range<usize> {
    let first = ((viewport.min.y - top).max(0.) / height).floor() as usize;
    let end = (((viewport.max.y - top).max(0.) / height).ceil() as usize + 1).min(count);
    first.min(end)..end
}
fn rows<'a>(
    remote: &'a RemoteExplorer,
    parent: &str,
    entries: &'a [Entry],
    depth: usize,
    out: &mut Vec<(String, &'a Entry, usize)>,
) {
    if depth > 32 {
        return;
    }
    for entry in entries {
        let path = join(parent, &entry.name);
        out.push((path.clone(), entry, depth));
        if remote.expanded.contains(&path)
            && let Some(children) = remote.children.get(&path)
        {
            rows(remote, &path, children, depth + 1, out);
        }
    }
}
fn checkbox(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: egui::Id,
    selected: &mut bool,
    p: Palette,
) -> bool {
    let response = ui.interact(rect, id, egui::Sense::click());
    if response.clicked() {
        *selected = !*selected;
    }
    ui.painter()
        .rect_filled(rect, 3, if *selected { p.accent } else { p.panel });
    ui.painter().rect_stroke(
        rect,
        3,
        Stroke::new(1_f32, if *selected { p.accent } else { p.dim }),
        egui::StrokeKind::Inside,
    );
    if *selected {
        ui.painter().line_segment(
            [
                rect.left_center() + Vec2::new(3., 0.),
                rect.center() + Vec2::new(-1., 3.),
            ],
            Stroke::new(2_f32, p.panel),
        );
        ui.painter().line_segment(
            [
                rect.center() + Vec2::new(-1., 3.),
                rect.right_top() + Vec2::new(-3., 4.),
            ],
            Stroke::new(2_f32, p.panel),
        );
    }
    response.clicked()
}
#[allow(clippy::too_many_arguments)]
fn item(
    ui: &mut egui::Ui,
    browser: &mut Browser,
    remote: &RemoteExplorer,
    path: &str,
    entry: &Entry,
    depth: usize,
    p: Palette,
    events: &mut Vec<Event>,
) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 28.), egui::Sense::click());
    let mut selected = browser.selected.contains_key(path);
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, 7, p.raised);
    }
    let x = rect.left() + 4. + depth as f32 * 14.;
    let expand_rect =
        egui::Rect::from_center_size(egui::pos2(x + 7., rect.center().y), Vec2::splat(16.));
    if entry.directory {
        let expanded = remote.expanded.contains(path);
        let c = expand_rect.center();
        let offsets = if expanded {
            [[-4., -2.], [0., 2.], [4., -2.]]
        } else {
            [[-2., -4.], [2., 0.], [-2., 4.]]
        };
        ui.painter().add(egui::Shape::line(
            offsets.map(|a| c + Vec2::new(a[0], a[1])).to_vec(),
            Stroke::new(1.5_f32, p.muted),
        ));
        if ui
            .interact(
                expand_rect,
                ui.id().with((path, "expand")),
                egui::Sense::click(),
            )
            .clicked()
        {
            events.push(Event::Expand(path.into()));
        }
    }
    let check =
        egui::Rect::from_center_size(egui::pos2(x + 27., rect.center().y), Vec2::splat(14.));
    let checked = checkbox(ui, check, ui.id().with((path, "check")), &mut selected, p);
    if checked {
        if selected {
            browser.selected.insert(path.into(), entry.directory);
        } else {
            browser.selected.remove(path);
        }
    }
    let image =
        egui::Rect::from_center_size(egui::pos2(x + 49., rect.center().y), Vec2::splat(19.));
    svg_icons::draw(
        ui.painter(),
        image,
        icon(entry),
        if entry.directory { p.accent } else { p.muted },
    );
    let end = if entry.directory {
        rect.right() - 5.
    } else {
        rect.right() - 58.
    };
    let name_rect = egui::Rect::from_min_max(
        egui::pos2(x + 63., rect.top()),
        egui::pos2(end.max(x + 64.), rect.bottom()),
    );
    filename(ui, name_rect, &entry.name, p, false);
    if !entry.directory {
        ui.painter().text(
            rect.right_center() - Vec2::new(5., 0.),
            Align2::RIGHT_CENTER,
            size(entry.bytes),
            FontId::proportional(10.),
            p.dim,
        );
    }
    if response.clicked() {
        browser.selected.clear();
        browser.selected.insert(path.into(), entry.directory);
    }
    if response.double_clicked() {
        events.push(if entry.directory {
            Event::Navigate(path.into())
        } else {
            Event::Edit(path.into())
        });
    }
    response
        .on_hover_text(path)
        .context_menu(|ui| context(ui, browser, path, entry, events));
}
fn tile(
    ui: &mut egui::Ui,
    browser: &mut Browser,
    path: &str,
    entry: &Entry,
    p: Palette,
    events: &mut Vec<Event>,
    width: f32,
) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 82.), egui::Sense::click());
    let mut selected = browser.selected.contains_key(path);
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, 7, p.raised);
    }
    let checked = checkbox(
        ui,
        egui::Rect::from_min_size(rect.min + Vec2::splat(4.), Vec2::splat(14.)),
        ui.id().with((path, "check")),
        &mut selected,
        p,
    );
    if checked {
        if selected {
            browser.selected.insert(path.into(), entry.directory);
        } else {
            browser.selected.remove(path);
        }
    }
    svg_icons::draw(
        ui.painter(),
        egui::Rect::from_center_size(rect.center() - Vec2::new(0., 6.), Vec2::splat(32.)),
        icon(entry),
        if entry.directory { p.accent } else { p.muted },
    );
    filename(
        ui,
        egui::Rect::from_min_max(
            rect.left_bottom() + Vec2::new(3., -25.),
            rect.right_bottom() - Vec2::new(3., 3.),
        ),
        &entry.name,
        p,
        true,
    );
    if response.clicked() {
        browser.selected.clear();
        browser.selected.insert(path.into(), entry.directory);
    }
    if response.double_clicked() {
        events.push(if entry.directory {
            Event::Navigate(path.into())
        } else {
            Event::Edit(path.into())
        });
    }
    response
        .on_hover_text(format!("{path}\n{}", size(entry.bytes)))
        .context_menu(|ui| context(ui, browser, path, entry, events));
}

pub fn show(ui: &mut egui::Ui, remote: &mut RemoteExplorer, name: &str, id: u64, p: Palette) {
    let mut browser = std::mem::take(&mut remote.browser);
    let mut events = Vec::new();
    let busy = remote.busy();
    ui.push_id(("explorer", id), |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4., 3.);
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                if remote.at_home() {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::splat(27.), egui::Sense::hover());
                    svg_icons::draw(ui.painter(), rect.shrink(4.), Icon::Home, p.muted);
                    response.on_hover_text(xpeed_i18n::t("Carpeta inicial"));
                } else if remote.can_go_back()
                    && tool(
                        ui,
                        Tool::Back,
                        xpeed_i18n::t("Volver a la carpeta anterior"),
                        p,
                    )
                    .clicked()
                {
                    events.push(Event::Back);
                }
                let path_width = (ui.available_width() - 93.).max(1.);
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(path_width, 27.), egui::Sense::hover());
                filename(ui, rect, &remote.path, p, false);
                response.on_hover_text(&remote.path);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled_ui(!browser.selected.is_empty(), |ui| {
                            tool(ui, Tool::Trash, xpeed_i18n::t("Eliminar selección"), p)
                        })
                        .inner
                        .clicked()
                    {
                        browser.dialog = Some(Dialog::Delete(
                            browser
                                .selected
                                .iter()
                                .map(|(p, d)| (p.clone(), *d))
                                .collect(),
                        ));
                    }
                    if tool(
                        ui,
                        if browser.grid { Tool::Grid } else { Tool::List },
                        if browser.grid {
                            xpeed_i18n::t("Vista de árbol")
                        } else {
                            xpeed_i18n::t("Vista de cuadrícula")
                        },
                        p,
                    )
                    .clicked()
                    {
                        browser.grid = !browser.grid;
                    }
                    let more = tool(ui, Tool::More, xpeed_i18n::t("Opciones del explorador"), p);
                    egui::Popup::menu(&more).show(|ui| {
                        menu(ui, &mut browser, &remote.path, &remote.entries, &mut events)
                    });
                });
            });
        });
        if let Some(error) = &remote.error {
            ui.add(egui::Label::new(RichText::new(error).small()).wrap());
        }
        ui.separator();
        let footer = if browser.selected.is_empty() { 0. } else { 39. }
            + if remote.operation_running() { 64. } else { 0. };
        egui::ScrollArea::vertical()
            .id_salt("items")
            .auto_shrink([false, false])
            .max_height((ui.available_height() - footer).max(50.))
            .show_viewport(ui, |ui, viewport| {
                let background = ui
                    .add_enabled_ui(!busy, |ui| {
                        ui.horizontal(|ui| {
                            let mut all = !remote.entries.is_empty()
                                && remote.entries.iter().all(|e| {
                                    browser.selected.contains_key(&join(&remote.path, &e.name))
                                });
                            if ui.checkbox(&mut all, "").clicked() {
                                if all {
                                    select_all(&mut browser, &remote.path, &remote.entries);
                                } else {
                                    browser.selected.clear();
                                }
                            }
                            let (r, _) =
                                ui.allocate_exact_size(Vec2::splat(19.), egui::Sense::hover());
                            svg_icons::draw(ui.painter(), r, Icon::Folder, p.accent);
                            let label = format!("{name}: {}", remote.host());
                            let width =
                                (ui.available_width() - if busy { 86. } else { 0. }).max(1.);
                            let (rect, response) =
                                ui.allocate_exact_size(Vec2::new(width, 24.), egui::Sense::click());
                            filename(ui, rect, &label, p, false);
                            response.on_hover_text(label).context_menu(|ui| {
                                menu(ui, &mut browser, &remote.path, &remote.entries, &mut events)
                            });
                            if busy {
                                ui.label(
                                    RichText::new(xpeed_i18n::t("- cargando…"))
                                        .size(10.)
                                        .color(p.muted),
                                );
                            }
                        });
                        if browser.grid {
                            let columns = (ui.available_width() / 78.).floor().max(1.) as usize;
                            let width =
                                (ui.available_width() - (columns - 1) as f32 * 4.) / columns as f32;
                            let count = remote.entries.len().div_ceil(columns);
                            let top = ui.cursor().min.y - ui.max_rect().top();
                            let range = visible_range(viewport, top, 85., count);
                            ui.add_space(range.start as f32 * 85.);
                            for chunk in remote
                                .entries
                                .chunks(columns)
                                .skip(range.start)
                                .take(range.len())
                            {
                                ui.horizontal(|ui| {
                                    for entry in chunk {
                                        tile(
                                            ui,
                                            &mut browser,
                                            &join(&remote.path, &entry.name),
                                            entry,
                                            p,
                                            &mut events,
                                            width,
                                        );
                                    }
                                });
                            }
                            ui.add_space((count - range.end) as f32 * 85.);
                        } else {
                            let mut listing = Vec::new();
                            rows(remote, &remote.path, &remote.entries, 0, &mut listing);
                            let top = ui.cursor().min.y - ui.max_rect().top();
                            let range = visible_range(viewport, top, 31., listing.len());
                            ui.add_space(range.start as f32 * 31.);
                            for (path, entry, depth) in &listing[range.clone()] {
                                item(
                                    ui,
                                    &mut browser,
                                    remote,
                                    path,
                                    entry,
                                    *depth,
                                    p,
                                    &mut events,
                                );
                            }
                            ui.add_space((listing.len() - range.end) as f32 * 31.);
                        }
                        if remote.entries.is_empty() && !busy && remote.error.is_none() {
                            ui.label(
                                RichText::new(xpeed_i18n::t("Carpeta vacía"))
                                    .small()
                                    .color(p.dim),
                            );
                        }
                        let content_height = ui.cursor().min.y - ui.max_rect().top();
                        let blank_height = (viewport.height() - content_height).max(0.);
                        ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), blank_height),
                            egui::Sense::click(),
                        )
                        .1
                    })
                    .inner;
                if !busy {
                    background.context_menu(|ui| {
                        menu(ui, &mut browser, &remote.path, &remote.entries, &mut events)
                    });
                }
            });
        if !browser.selected.is_empty() {
            ui.separator();
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(xpeed_i18n::format(
                            "{} seleccionados",
                            &[("0", format!("{}", browser.selected.len()))],
                        ))
                        .size(10.),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if tool(ui, Tool::Trash, xpeed_i18n::t("Eliminar selección"), p).clicked()
                        {
                            browser.dialog = Some(Dialog::Delete(
                                browser
                                    .selected
                                    .iter()
                                    .map(|(p, d)| (p.clone(), *d))
                                    .collect(),
                            ));
                        }
                        if tool(ui, Tool::Paste, xpeed_i18n::t("Pegar aquí (mover)"), p).clicked()
                        {
                            paste(&mut browser, &remote.path, &mut events);
                        }
                        if tool(ui, Tool::Cut, xpeed_i18n::t("Cortar para mover"), p).clicked() {
                            cut_selection(&mut browser);
                        }
                        if tool(ui, Tool::Copy, xpeed_i18n::t("Copiar rutas"), p).clicked() {
                            copy_paths(ui, &browser);
                        }
                    });
                })
            });
        }
    });
    if let Some(dialog) = &mut browser.dialog {
        let focus_name = std::mem::take(&mut browser.focus_name);
        let mut done = false;
        let modal = egui::Modal::new(egui::Id::new(("file_dialog", id))).show(ui.ctx(), |ui| {
            ui.set_width(320.);
            match dialog {
                Dialog::Archive { sources, destination, kind } => {
                    ui.heading(xpeed_i18n::t(if kind == "extract" { "Extract to folder…" } else { "Compress…" }));
                    ui.label(xpeed_i18n::t("Runs on the server. Requires Python 3. RAR requires additional server tools. Existing destinations are not overwritten."));
                    ui.label(xpeed_i18n::t("Destination name:"));
                    ui.text_edit_singleline(destination);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(valid_name(destination) && !destination.contains('\\'), egui::Button::new(xpeed_i18n::t("Start"))).clicked() {
                            events.push(Event::Archive(sources.clone(), destination.clone(), kind.clone()));
                            done = true;
                        }
                        if ui.button(xpeed_i18n::t("Cancelar")).clicked() { done = true; }
                    });
                }
                Dialog::Name {
                    parent,
                    old,
                    folder,
                    name,
                } => {
                    ui.heading(if old.is_some() {
                        xpeed_i18n::t("Renombrar")
                    } else if *folder {
                        xpeed_i18n::t("Nueva carpeta")
                    } else {
                        xpeed_i18n::t("Nuevo archivo")
                    });
                    ui.add(egui::Label::new(parent.as_str()).wrap());
                    let response = ui.text_edit_singleline(name);
                    if focus_name {
                        response.request_focus();
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                valid_name(name),
                                egui::Button::new(xpeed_i18n::t("Guardar")),
                            )
                            .clicked()
                        {
                            let path = join(parent, name);
                            let op = if let Some(old) = old {
                                Mutation::Rename(old.clone(), path)
                            } else if *folder {
                                Mutation::Folder(path)
                            } else {
                                Mutation::File(path)
                            };
                            events.push(Event::Mutate(vec![op]));
                            done = true;
                        }
                        if ui.button(xpeed_i18n::t("Cancelar")).clicked() {
                            done = true;
                        }
                    });
                }
                Dialog::Delete(paths) => {
                    ui.heading(xpeed_i18n::t("Eliminar selección"));
                    ui.label(xpeed_i18n::format(
                        "Se eliminarán {} elementos. Las carpetas deben estar vacías.",
                        &[("0", format!("{}", paths.len()))],
                    ));
                    egui::ScrollArea::vertical()
                        .max_height(150.)
                        .show(ui, |ui| {
                            for (path, _) in paths.iter() {
                                ui.add(egui::Label::new(path).wrap());
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui.button(xpeed_i18n::t("Eliminar")).clicked() {
                            let mut paths = paths.clone();
                            paths.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
                            events.push(Event::Mutate(
                                paths
                                    .into_iter()
                                    .map(|(p, d)| Mutation::Remove(p, d))
                                    .collect(),
                            ));
                            done = true;
                        }
                        if ui.button(xpeed_i18n::t("Cancelar")).clicked() {
                            done = true;
                        }
                    });
                }
            }
        });
        if done || modal.should_close() {
            browser.dialog = None;
        }
    }
    if let Some((picker, destination)) = &mut browser.upload
        && let Some(plan) = picker.show(ui.ctx(), id, destination)
    {
        let relative = |path: &std::path::Path| {
            let parts: Option<Vec<_>> = path
                .components()
                .map(|component| match component {
                    std::path::Component::Normal(name) => name.to_str().map(str::to_owned),
                    _ => None,
                })
                .collect();
            parts.map(|parts| parts.join("/"))
        };
        let directories = plan
            .directories
            .into_iter()
            .filter_map(|path| {
                let path = relative(&path)?;
                Some(join(destination, &path))
            })
            .collect();
        let files = plan
            .files
            .into_iter()
            .filter_map(|(local, path)| {
                let path = relative(&path)?;
                Some((local, join(destination, &path)))
            })
            .collect();
        remote.upload_tree(files, directories);
        browser.upload = None;
    }
    let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
    if ui.rect_contains_pointer(ui.max_rect()) && !busy && !dropped.is_empty() {
        let mut files = Vec::new();
        for file in dropped {
            if let Some(path) = file.path
                && path.is_file()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                && valid_name(name)
            {
                let destination = join(&remote.path, name);
                files.push((path, destination));
            }
        }
        remote.transfer(files, false);
    }
    let bytes = remote
        .transferred
        .load(std::sync::atomic::Ordering::Acquire);
    if remote.operation_running() {
        if let Ok(status) = remote.transfer_file.lock()
            && !status.is_empty()
        {
            ui.add(egui::Label::new(status.as_str()).truncate());
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
        ui.horizontal(|ui| {
            if remote.transfer_total > 0 {
                ui.add(
                    egui::ProgressBar::new(bytes as f32 / remote.transfer_total as f32)
                        .desired_width((ui.available_width() - 80.).max(20.))
                        .text(format!("{} / {}", size(bytes), size(remote.transfer_total))),
                );
            } else {
                ui.spinner();
                if bytes > 0 {
                    ui.label(size(bytes));
                }
            }
            if ui.small_button(xpeed_i18n::t("Cancelar")).clicked() {
                remote.cancel_operation();
            }
        });
    }
    remote.browser = browser;
    // At most one navigation/operation per frame.
    if let Some(event) = events.into_iter().next() {
        match event {
            Event::Back => remote.go_back(),
            Event::Expand(path) => remote.expand(path),
            Event::Navigate(path) => remote.navigate(path),
            Event::Refresh => remote.refresh(),
            Event::Edit(path) => remote.edit_requested = Some(path),
            Event::Download(path) => remote.download_requested = Some(path),
            Event::Mutate(ops) => remote.mutate(ops),
            Event::Archive(sources, destination, kind) => {
                remote.archive(sources, destination, &kind)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loading_is_inline_with_connection_and_header_has_only_current_path() {
        let ctx = egui::Context::default();
        let mut remote = RemoteExplorer::preview();
        remote.loading = true;
        let mut output = render(&ctx, &mut remote, vec![]);
        for _ in 0..3 {
            output = render(&ctx, &mut remote, vec![]);
        }
        text_pos(&output, "/doolpool.com");
        let root = text_pos(&output, "doolpool.com: 192.0.2.10");
        assert!(
            root.x < 90.,
            "Connection label must sit next to its folder icon"
        );
        let loading = text_pos(&output, "- cargando…");
        assert!((root.y - loading.y).abs() < 5.);
        assert!(output.shapes.iter().all(|shape| !matches!(&shape.shape, egui::Shape::Text(text) if ["EXPLORADOR", "Procesando…"].contains(&text.galley.job.text.as_str()))));
    }
    #[test]
    fn large_directories_render_only_visible_rows() {
        let ctx = egui::Context::default();
        let mut remote = RemoteExplorer::preview();
        remote.entries = (0..10000)
            .map(|n| Entry {
                name: format!("file-{n}.txt"),
                directory: false,
                bytes: 12,
            })
            .collect();
        for grid in [false, true] {
            remote.browser.grid = grid;
            let start = std::time::Instant::now();
            let output = render(&ctx, &mut remote, Vec::new());
            let names = output.shapes.iter().filter(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text.starts_with("file-"))).count();
            assert!(
                names > 0 && names < 100,
                "Only visible entries should allocate widgets: {names}"
            );
            eprintln!(
                "10,000 files, grid={grid}: {:?}, {names} visible names",
                start.elapsed()
            );
        }
        let viewport = egui::Rect::from_min_max(egui::pos2(0., 3100.), egui::pos2(300., 3720.));
        let range = visible_range(viewport, 0., 31., 10000);
        assert_eq!(range, 100..121);
    }
    fn render(
        ctx: &egui::Context,
        remote: &mut RemoteExplorer,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(300., 620.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(ui, remote, "doolpool.com", 7, Palette::default())
                });
            },
        )
    }
    fn text_pos(output: &egui::FullOutput, name: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    (text.galley.job.text == xpeed_i18n::t(name)).then_some(text.pos)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("Missing text {name}"))
    }
    fn click(
        ctx: &egui::Context,
        remote: &mut RemoteExplorer,
        pos: egui::Pos2,
        button: egui::PointerButton,
    ) {
        render(ctx, remote, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            render(
                ctx,
                remote,
                vec![egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: Default::default(),
                }],
            );
        }
    }
    #[test]
    fn checkboxes_select_multiple_and_both_views_preserve_selection() {
        let ctx = egui::Context::default();
        let mut remote = RemoteExplorer::preview();
        let mut output = render(&ctx, &mut remote, vec![]);
        for _ in 0..3 {
            output = render(&ctx, &mut remote, vec![]);
        }
        let cache = text_pos(&output, ".cache") + Vec2::new(-36., 6.);
        let known = text_pos(&output, ".well-known") + Vec2::new(-36., 6.);
        click(&ctx, &mut remote, cache, egui::PointerButton::Primary);
        click(&ctx, &mut remote, known, egui::PointerButton::Primary);
        assert_eq!(remote.browser.selected.len(), 2);
        remote.browser.grid = true;
        let output = render(&ctx, &mut remote, vec![]);
        assert_eq!(remote.browser.selected.len(), 2);
        assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.job.text=="foto.JPG")));
        remote.browser.grid = false;
        render(&ctx, &mut remote, vec![]);
        assert_eq!(remote.browser.selected.len(), 2);
        assert!(!remote.busy());
    }
    #[test]
    fn nested_cut_selection_moves_only_ancestor() {
        let mut browser = Browser::default();
        browser.selected.insert("/root/folder".into(), true);
        browser.selected.insert("/root/folder/file".into(), false);
        browser.selected.insert("/root/folder-other".into(), false);
        cut_selection(&mut browser);
        assert_eq!(browser.cut.len(), 2);
        assert!(!browser.cut.iter().any(|(p, _)| p.ends_with("/file")));
    }
    #[test]
    fn blank_space_menu_creates_uploads_and_toggles_selection() {
        for grid in [false, true] {
            let ctx = egui::Context::default();
            let mut remote = RemoteExplorer::preview();
            remote.browser.grid = grid;
            for _ in 0..4 {
                render(&ctx, &mut remote, vec![]);
            }
            let blank = egui::pos2(140., 470.);
            click(&ctx, &mut remote, blank, egui::PointerButton::Secondary);
            let mut output = render(&ctx, &mut remote, vec![]);
            for _ in 0..3 {
                output = render(&ctx, &mut remote, vec![]);
            }
            for label in ["Nuevo archivo", "Nueva carpeta", "Upload files…"] {
                text_pos(&output, label);
            }
            let select = text_pos(&output, "Seleccionar todo") + Vec2::splat(5.);
            click(&ctx, &mut remote, select, egui::PointerButton::Primary);
            assert_eq!(remote.browser.selected.len(), remote.entries.len());
            click(&ctx, &mut remote, blank, egui::PointerButton::Secondary);
            for _ in 0..4 {
                output = render(&ctx, &mut remote, vec![]);
            }
            let clear = text_pos(&output, "Deseleccionar todo") + Vec2::splat(5.);
            click(&ctx, &mut remote, clear, egui::PointerButton::Primary);
            assert!(remote.browser.selected.is_empty());
            assert!(!remote.busy());
        }
    }
    #[test]
    fn right_click_opens_actions_and_rename_dialog_without_starting_transport() {
        let ctx = egui::Context::default();
        let mut remote = RemoteExplorer::preview();
        let mut output = render(&ctx, &mut remote, vec![]);
        for _ in 0..3 {
            output = render(&ctx, &mut remote, vec![]);
        }
        let pos = text_pos(&output, ".bash_history") + Vec2::splat(5.);
        click(&ctx, &mut remote, pos, egui::PointerButton::Secondary);
        for _ in 0..3 {
            output = render(&ctx, &mut remote, vec![]);
        }
        let rename = text_pos(&output, "Renombrar") + Vec2::splat(5.);
        click(&ctx, &mut remote, rename, egui::PointerButton::Primary);
        assert!(
            matches!(&remote.browser.dialog,Some(Dialog::Name{old:Some(path),..}) if path=="/doolpool.com/.bash_history")
        );
        assert!(!remote.busy());
    }
    #[test]
    fn names_cannot_escape_parent() {
        for name in ["", ".", "..", "a/b", "a\0b", "a\nb"] {
            assert!(!valid_name(name));
        }
        assert!(valid_name("mi carpeta ñ.txt"));
    }
    #[test]
    fn classification_is_case_insensitive() {
        let entry = |name: &str| Entry {
            name: name.into(),
            directory: false,
            bytes: 0,
        };
        assert!(matches!(icon(&entry("foto.JPG")), Icon::Image));
        assert!(matches!(icon(&entry("audio.FLAC")), Icon::Audio));
        assert!(matches!(icon(&entry("video.MP4")), Icon::Video));
        assert!(matches!(icon(&entry(".bashrc")), Icon::File));
    }
    #[test]
    fn moving_never_renames_onto_itself() {
        let mut browser = Browser {
            cut: vec![("/root/a.txt".into(), false)],
            ..Default::default()
        };
        let mut events = Vec::new();
        paste(&mut browser, "/root", &mut events);
        assert!(events.is_empty());
        paste(&mut browser, "/dest", &mut events);
        assert!(
            matches!(&events[0],Event::Mutate(ops) if matches!(&ops[0],Mutation::Rename(a,b) if a=="/root/a.txt" && b=="/dest/a.txt"))
        );
    }
}
