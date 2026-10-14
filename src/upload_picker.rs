use crate::svg_icons::{self, Icon};
use eframe::egui;
use std::{collections::BTreeSet, path::PathBuf, sync::mpsc};

type Selection = Result<Option<Vec<PathBuf>>, String>;
type Preparation = Result<UploadPlan, String>;

#[derive(Default)]
pub struct UploadPlan {
    pub directories: Vec<PathBuf>,
    pub files: Vec<(PathBuf, PathBuf)>,
}

#[derive(Default)]
pub struct Picker {
    selected: BTreeSet<PathBuf>,
    choosing: Option<mpsc::Receiver<Selection>>,
    preparing: Option<mpsc::Receiver<Preparation>>,
    error: Option<String>,
}

impl Picker {
    fn choose(&mut self, folders: bool, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        self.choosing = Some(rx);
        self.error = None;
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new();
            if let Some(path) = dirs::download_dir().or_else(dirs::home_dir) {
                dialog = dialog.set_directory(path);
            }
            let paths = if folders {
                dialog.pick_folders()
            } else {
                dialog.pick_files()
            };
            let _ = tx.send(Ok(paths));
            wake.request_repaint();
        });
    }

    fn prepare(&mut self, ctx: &egui::Context) {
        let roots = self.selected.clone();
        let (tx, rx) = mpsc::channel();
        self.preparing = Some(rx);
        self.error = None;
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(prepare_upload(roots));
            wake.request_repaint();
        });
    }

    pub fn show(&mut self, ctx: &egui::Context, id: u64, destination: &str) -> Option<UploadPlan> {
        let t = xpeed_i18n::t;
        if let Some(rx) = &self.choosing {
            match rx.try_recv() {
                Ok(Ok(Some(paths))) => {
                    self.choosing = None;
                    self.selected.extend(paths);
                }
                Ok(Ok(None)) => self.choosing = None,
                Ok(Err(error)) => {
                    self.choosing = None;
                    self.error = Some(error);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.choosing = None;
                    self.error = Some(t("Unable to open the system file picker.").into());
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }
        if let Some(rx) = &self.preparing {
            match rx.try_recv() {
                Ok(result) => {
                    self.preparing = None;
                    match result {
                        Ok(plan) => return Some(plan),
                        Err(error) => self.error = Some(error),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.preparing = None;
                    self.error = Some(t("Unable to prepare selected items.").into());
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }

        let mut result = None;
        let mut open = true;
        let screen = ctx.content_rect();
        egui::Window::new(t("Upload files…"))
            .id(egui::Id::new(("native_upload_picker_v2", id)))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_pos(screen.center() - egui::vec2(270., 210.))
            .default_size([540., 420.])
            .min_size([420., 300.])
            .max_size([760., 620.])
            .show(ctx, |ui| {
                ui.label(format!("{} {destination}", t("Destination:")));
                ui.add_space(6.);
                let busy = self.choosing.is_some() || self.preparing.is_some();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!busy, egui::Button::new(t("Choose files…")))
                        .clicked()
                    {
                        self.choose(false, ctx);
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new(t("Choose folders…")))
                        .clicked()
                    {
                        self.choose(true, ctx);
                    }
                    if ui
                        .add_enabled(
                            !busy && !self.selected.is_empty(),
                            egui::Button::new(t("Clear selection")),
                        )
                        .clicked()
                    {
                        self.selected.clear();
                    }
                });
                if busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t(if self.preparing.is_some() {
                            "Preparing selected items…"
                        } else {
                            "Waiting for the system file picker…"
                        }));
                    });
                }
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.separator();
                ui.label(egui::RichText::new(t("Selected items")).strong());
                let list_height = (ui.available_height() - 92.).max(110.);
                egui::ScrollArea::vertical()
                    .max_height(list_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.selected.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(28.);
                                ui.weak(t("No files or folders selected."));
                            });
                        }
                        let mut remove = None;
                        for path in &self.selected {
                            let directory = path.is_dir();
                            egui::Frame::new()
                                .fill(ui.visuals().faint_bg_color)
                                .corner_radius(7.)
                                .inner_margin(egui::Margin::symmetric(8, 5))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::Vec2::splat(20.),
                                            egui::Sense::hover(),
                                        );
                                        svg_icons::draw(
                                            ui.painter(),
                                            rect,
                                            if directory { Icon::Folder } else { Icon::File },
                                            ui.visuals().widgets.inactive.fg_stroke.color,
                                        );
                                        ui.vertical(|ui| {
                                            ui.label(
                                                path.file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy(),
                                            );
                                            ui.weak(path.to_string_lossy());
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .small_button("×")
                                                    .on_hover_text(t("Remove"))
                                                    .clicked()
                                                {
                                                    remove = Some(path.clone());
                                                }
                                            },
                                        );
                                    });
                                });
                            ui.add_space(3.);
                        }
                        if let Some(path) = remove {
                            self.selected.remove(&path);
                        }
                    });
                ui.separator();
                ui.label(format!("{}: {}", t("Selected items"), self.selected.len()));
                ui.weak(t(
                    "Folders are recreated and files upload one at a time. Existing destinations are not overwritten.",
                ));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.selected.is_empty() && !busy,
                            egui::Button::new(t("Upload")),
                        )
                        .clicked()
                    {
                        self.prepare(ctx);
                    }
                    if ui.button(t("Cancelar")).clicked() {
                        result = Some(UploadPlan::default());
                    }
                });
            });
        if !open {
            result = Some(UploadPlan::default());
        }
        result
    }
}

fn prepare_upload(mut roots: BTreeSet<PathBuf>) -> Preparation {
    let all = roots.clone();
    roots.retain(|path| !path.ancestors().skip(1).any(|parent| all.contains(parent)));
    let mut plan = UploadPlan::default();
    for root in roots {
        let name = root
            .file_name()
            .ok_or_else(|| "Selected item has no file name.".to_owned())?;
        let relative = PathBuf::from(name);
        let metadata = std::fs::symlink_metadata(&root).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            plan.files.push((root, relative));
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        let mut pending = vec![(root, relative)];
        while let Some((directory, relative)) = pending.pop() {
            plan.directories.push(relative.clone());
            let mut children: Vec<_> = std::fs::read_dir(&directory)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .collect();
            children.sort_by_key(|entry| entry.file_name().to_ascii_lowercase());
            for entry in children.into_iter().rev() {
                let kind = entry.file_type().map_err(|e| e.to_string())?;
                if kind.is_symlink() {
                    continue;
                }
                let child_relative = relative.join(entry.file_name());
                if kind.is_dir() {
                    pending.push((entry.path(), child_relative));
                } else if kind.is_file() {
                    plan.files.push((entry.path(), child_relative));
                }
            }
        }
    }
    plan.directories
        .sort_by_key(|path| path.components().count());
    plan.files.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_picker(
        ctx: &egui::Context,
        picker: &mut Picker,
        events: Vec<egui::Event>,
    ) -> Option<UploadPlan> {
        let mut outcome = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900., 700.),
                )),
                events,
                ..Default::default()
            },
            |ctx| outcome = picker.show(ctx, 99, "/remote"),
        );
        outcome
    }

    #[test]
    fn floating_picker_stays_open_when_background_is_clicked() {
        let ctx = egui::Context::default();
        let mut picker = Picker::default();
        for _ in 0..3 {
            assert!(render_picker(&ctx, &mut picker, Vec::new()).is_none());
        }
        let pos = egui::pos2(10., 680.);
        for pressed in [true, false] {
            assert!(
                render_picker(
                    &ctx,
                    &mut picker,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                )
                .is_none()
            );
        }
    }

    #[test]
    fn selected_folder_is_prepared_recursively_without_duplicate_children() {
        let root =
            std::env::temp_dir().join(format!("xpeed-upload-tree-{}", rand::random::<u64>()));
        let folder = root.join("site");
        let nested = folder.join("assets");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(folder.join("index.html"), "home").unwrap();
        let child = nested.join("app.js");
        std::fs::write(&child, "let ready = true;").unwrap();
        let plan = prepare_upload(BTreeSet::from([folder, child])).unwrap();
        assert_eq!(
            plan.directories,
            vec![PathBuf::from("site"), PathBuf::from("site/assets")]
        );
        let relative: Vec<_> = plan.files.into_iter().map(|(_, path)| path).collect();
        assert_eq!(
            relative,
            vec![
                PathBuf::from("site/assets/app.js"),
                PathBuf::from("site/index.html")
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
