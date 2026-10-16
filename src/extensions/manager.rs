use super::*;

fn document_ui(
    ui: &mut egui::Ui,
    cache: &mut egui_commonmark::CommonMarkCache,
    folder: &Path,
    manifest: &Manifest,
    document: &str,
) {
    ui.heading(xpeed_i18n::t("Detalles"));
    ui.add_space(8.0);
    if document.is_empty() {
        ui.add(egui::Label::new(manifest.description()).wrap());
    } else {
        super::markdown::show_document(ui, cache, folder, document);
    }
}

fn metadata_ui(ui: &mut egui::Ui, m: &Manifest, requirements: Option<&str>) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(14)
        .corner_radius(8)
        .show(ui, |ui| {
            ui.strong(xpeed_i18n::t("INFORMACIÓN"));
            ui.add_space(12.0);
            for (label, value) in [
                (xpeed_i18n::t("Identificador"), m.id.as_str()),
                (xpeed_i18n::t("Versión"), m.version.as_str()),
                (xpeed_i18n::t("Publicador"), m.publisher.as_str()),
                (xpeed_i18n::t("Licencia"), m.license.as_str()),
            ] {
                ui.weak(label);
                ui.add(egui::Label::new(value).wrap());
                ui.add_space(8.0);
            }
            if let Some(requirements) = requirements {
                ui.separator();
                ui.strong(xpeed_i18n::t("Requisitos"));
                ui.add(egui::Label::new(xpeed_i18n::t(requirements)).wrap());
            }
            ui.add_space(12.0);
            ui.separator();
            ui.strong(xpeed_i18n::t("Funciones"));
            for capability in &m.capabilities {
                ui.label(xpeed_i18n::t(capability));
            }
            ui.add_space(12.0);
            ui.separator();
            ui.strong(xpeed_i18n::t("Permisos"));
            for permission in m.permissions() {
                ui.label(xpeed_i18n::t(permission));
            }
        });
}

fn key(id: &str) -> u64 {
    id.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn icon(ui: &mut egui::Ui, id: &str, size: f32, image: Option<&egui::TextureHandle>) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    if let Some(image) = image {
        let aspect = image.size_vec2();
        let fitted = aspect * (size / aspect.x.max(aspect.y));
        ui.painter().image(
            image.id(),
            egui::Rect::from_center_size(rect.center(), fitted),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        return;
    }
    let color = ui.visuals().hyperlink_color;
    ui.painter()
        .rect_filled(rect, 9, color.gamma_multiply(0.12));
    crate::svg_icons::draw(
        ui.painter(),
        rect.shrink(size * 0.18),
        if id == "xpeed.server-monitor" {
            crate::svg_icons::Icon::Connection
        } else {
            crate::svg_icons::Icon::Extensions
        },
        color,
    );
}

fn row(
    ui: &mut egui::Ui,
    id: &str,
    name: &str,
    description: &str,
    publisher: &str,
    image: Option<&egui::TextureHandle>,
) -> bool {
    let width = ui.available_width();
    let response = egui::Frame::new()
        .inner_margin(8)
        .corner_radius(8)
        .show(ui, |ui| {
            ui.set_min_width((width - 16.0).max(0.0));
            ui.horizontal_top(|ui| {
                icon(ui, id, 42.0, image);
                ui.vertical(|ui| {
                    ui.set_max_width((width - 72.0).max(32.0));
                    ui.add(egui::Label::new(RichText::new(name).strong()).truncate());
                    ui.add(egui::Label::new(RichText::new(description).weak()).truncate());
                    ui.add(egui::Label::new(RichText::new(publisher).small()).truncate());
                });
            });
        })
        .response;
    let click = ui.interact(response.rect, response.id.with(id), egui::Sense::click());
    if click.hovered() {
        ui.painter().rect_stroke(
            response.rect,
            8,
            egui::Stroke::new(1.0_f32, ui.visuals().widgets.hovered.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
    }
    click.on_hover_text(description).clicked()
}

impl Extensions {
    pub fn request_details(&mut self, id: &str) {
        if self.runtimes.contains_key(id) || self.catalog.iter().any(|(c, _)| c.id == id) {
            self.details_requested = Some(key(id));
        }
    }
    pub fn detail_title(&self, detail: u64) -> String {
        self.runtimes
            .values()
            .find(|r| key(&r.manifest.id) == detail)
            .map(|r| r.manifest.name().to_owned())
            .or_else(|| {
                self.catalog
                    .iter()
                    .find(|(c, _)| key(&c.id) == detail)
                    .map(|(c, folder)| {
                        Manifest::read(folder)
                            .map(|m| m.name().to_owned())
                            .unwrap_or_else(|_| c.name.clone())
                    })
            })
            .unwrap_or_else(|| xpeed_i18n::t("Extensión").into())
    }

    pub fn manager_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(xpeed_i18n::t("Extensiones"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::containers::menu::MenuButton::new("Git")
                    .config(
                        egui::containers::menu::MenuConfig::new()
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                    )
                    .ui(ui, |ui| {
                        ui.label(xpeed_i18n::t("Conectar repositorio de extensión"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.git.repository_input)
                                .hint_text("https://github.com/autor/extensión"),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.git.reference_input)
                                .hint_text(xpeed_i18n::t("Rama o etiqueta (opcional)")),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.git.path_input)
                                .hint_text(xpeed_i18n::t("Subcarpeta de extensión (opcional)")),
                        );
                        if ui
                            .add_enabled(
                                !self.git.busy(),
                                egui::Button::new(xpeed_i18n::t("Conectar repositorio")),
                            )
                            .clicked()
                        {
                            self.connect_repository(ui.ctx());
                            ui.close();
                        }
                        ui.separator();
                        ui.label(xpeed_i18n::t("Conectar catálogo de Xpeed"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.git.catalog_input)
                                .hint_text("https://github.com/xpeed-sh/xpeed"),
                        );
                        ui.weak(xpeed_i18n::t("Lee extensions.json del repositorio."));
                        if ui
                            .add_enabled(
                                !self.git.busy(),
                                egui::Button::new(xpeed_i18n::t("Conectar catálogo")),
                            )
                            .clicked()
                        {
                            self.connect_catalog(ui.ctx());
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                !self.git.busy(),
                                egui::Button::new(xpeed_i18n::t("Actualizar catálogo Git")),
                            )
                            .clicked()
                        {
                            self.refresh_git(ui.ctx());
                            ui.close();
                        }
                        ui.separator();
                        ui.label(xpeed_i18n::t("Instalar desde una carpeta"));
                        ui.text_edit_singleline(&mut self.install_path);
                        if ui.button(xpeed_i18n::t("Instalar paquete")).clicked() {
                            let path = PathBuf::from(self.install_path.trim());
                            self.review_install(&path);
                            ui.close();
                        }
                    });
            });
        });
        ui.add_space(8.0);
        if !self.git.status.is_empty() {
            ui.horizontal(|ui| {
                if self.git.busy() {
                    ui.spinner();
                }
                ui.weak(&self.git.status);
            });
        }
        ui.add(
            egui::TextEdit::singleline(&mut self.manager_query)
                .hint_text(xpeed_i18n::t("Buscar extensiones"))
                .desired_width(f32::INFINITY),
        );
        ui.add_space(8.0);
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        let query = self.manager_query.to_lowercase();
        let catalog: Vec<_> = self
            .catalog
            .iter()
            .map(|(c, folder)| {
                let installed = self.runtimes.get(&c.id);
                let manifest = installed
                    .map(|runtime| runtime.manifest.clone())
                    .or_else(|| Manifest::read(folder).ok());
                let assets = installed
                    .map(|runtime| runtime.folder.clone())
                    .unwrap_or_else(|| folder.clone());
                (c.clone(), manifest, assets)
            })
            .filter(|(c, m, _)| {
                format!(
                    "{} {}",
                    m.as_ref().map(|m| m.name()).unwrap_or(&c.name),
                    m.as_ref()
                        .map(|m| m.description())
                        .unwrap_or(&c.description)
                )
                .to_lowercase()
                .contains(&query)
            })
            .collect();
        ui.horizontal(|ui| {
            ui.strong(xpeed_i18n::t("EXTENSIONES"));
            ui.weak(catalog.len().to_string());
        });
        egui::ScrollArea::vertical()
            .id_salt("catalog_extensions")
            .show(ui, |ui| {
                if catalog.is_empty() {
                    ui.weak(xpeed_i18n::t("No hay extensiones que coincidan."));
                }
                for (c, m, folder) in &catalog {
                    let image = m.as_ref().and_then(|m| self.image(m, folder, ui.ctx()));
                    let publisher = m.as_ref().map(|m| m.publisher.as_str()).unwrap_or("XPEED");
                    if row(
                        ui,
                        &c.id,
                        m.as_ref().map(|m| m.name()).unwrap_or(&c.name),
                        m.as_ref()
                            .map(|m| m.description())
                            .unwrap_or(&c.description),
                        publisher,
                        image.as_ref(),
                    ) {
                        self.details_requested = Some(key(&c.id));
                    }
                }
            });
    }

    pub fn details_ui(&mut self, ui: &mut egui::Ui, detail: u64) {
        let catalog = self
            .catalog
            .iter()
            .find(|(c, _)| key(&c.id) == detail)
            .cloned();
        let metadata = self
            .runtimes
            .values()
            .find(|r| key(&r.manifest.id) == detail)
            .map(|r| (r.manifest.clone(), r.folder.clone()))
            .or_else(|| {
                catalog.as_ref().and_then(|(_, folder)| {
                    Manifest::read(folder).ok().map(|m| (m, folder.clone()))
                })
            });
        let Some((m, folder)) = metadata else {
            ui.label(xpeed_i18n::t(
                "La ficha del repositorio todavía no está disponible.",
            ));
            if self.git.busy() {
                ui.spinner();
            }
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            if ui
                .add_enabled(
                    !self.git.busy(),
                    egui::Button::new(xpeed_i18n::t("Actualizar catálogo Git")),
                )
                .clicked()
            {
                self.refresh_git(ui.ctx());
            }
            return;
        };
        let mut save = false;
        let mut uninstall = false;
        let mut update = false;
        let mut rollback = None;
        let previous = self.previous_versions(&m.id);
        egui::ScrollArea::vertical()
            .id_salt(("extension_details", detail))
            .show(ui, |ui| {
                egui::Frame::new().inner_margin(24).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        icon(
                            ui,
                            &m.id,
                            if ui.available_width() > 650.0 {
                                96.0
                            } else {
                                56.0
                            },
                            self.image(&m, &folder, ui.ctx()).as_ref(),
                        );
                        ui.vertical(|ui| {
                            ui.add(
                                egui::Label::new(RichText::new(m.name()).size(26.0).strong())
                                    .wrap(),
                            );
                            ui.label(
                                RichText::new(format!("{} · v{}", m.publisher, m.version))
                                    .color(ui.visuals().hyperlink_color),
                            );
                            ui.add(egui::Label::new(m.description()).wrap());
                        });
                    });
                    ui.add_space(16.0);
                    ui.horizontal_wrapped(|ui| {
                        if let Some(runtime) = self.runtimes.get_mut(&m.id) {
                            if ui
                                .checkbox(&mut runtime.enabled, xpeed_i18n::t("Activada"))
                                .changed()
                            {
                                if !runtime.enabled {
                                    runtime.disconnect();
                                }
                                save = true;
                            }
                            if runtime.enabled {
                                if m.capabilities.iter().any(|c| c == "chat")
                                    && ui.button(xpeed_i18n::t("Abrir extensión")).clicked()
                                {
                                    self.selected = Some(m.id.clone());
                                    self.legacy = false;
                                    self.show_chat_requested = true;
                                }
                                for view in &m.views {
                                    if ui
                                        .button(xpeed_i18n::format(
                                            "Abrir {}",
                                            &[("0", m.view_title(view).to_string())],
                                        ))
                                        .clicked()
                                    {
                                        self.show_view_requested = Some(view.key(&m.id));
                                    }
                                }
                            }
                            if runtime.process.is_some()
                                && !runtime.busy
                                && ui.button(xpeed_i18n::t("Desconectar")).clicked()
                            {
                                runtime.disconnect();
                            }
                            if catalog
                                .as_ref()
                                .is_some_and(|(item, _)| item.repository.is_some())
                                && ui
                                    .add_enabled(
                                        !self.git.busy()
                                            && runtime.process.is_none()
                                            && !runtime.busy,
                                        egui::Button::new(xpeed_i18n::t("Update from Git")),
                                    )
                                    .clicked()
                            {
                                update = true;
                            }
                            if !previous.is_empty() {
                                ui.add_enabled_ui(
                                    runtime.process.is_none() && !runtime.busy,
                                    |ui| {
                                        ui.menu_button(
                                            xpeed_i18n::t("Restore another version"),
                                            |ui| {
                                                for (version, folder) in &previous {
                                                    if ui.button(version).clicked() {
                                                        rollback = Some(folder.clone());
                                                        ui.close();
                                                    }
                                                }
                                            },
                                        );
                                    },
                                );
                            }
                            if ui.button(xpeed_i18n::t("Desinstalar")).clicked() {
                                uninstall = true;
                            }
                        } else if ui
                            .add_enabled(
                                !self.git.busy(),
                                egui::Button::new(xpeed_i18n::t("Instalar extensión")),
                            )
                            .clicked()
                        {
                            if let Some((item, _)) = &catalog
                                && item.repository.is_some()
                            {
                                self.install_git(item, ui.ctx());
                            } else {
                                self.review_install(&folder);
                            }
                        }
                    });
                    if self.pending_removals.iter().any(|(id, _)| id == &m.id) {
                        ui.weak(xpeed_i18n::t("Desinstalando…"));
                    }
                    if let Some(error) = &self.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if !self.git.status.is_empty() {
                        ui.weak(&self.git.status);
                    }
                    if let Some((item, _)) = &catalog
                        && let Some(repository) = &item.repository
                    {
                        ui.hyperlink_to(xpeed_i18n::t("Ver repositorio"), repository);
                        if !item.reference.is_empty() {
                            ui.weak(xpeed_i18n::format(
                                "Referencia Git: {}",
                                &[("0", item.reference.to_string())],
                            ));
                        }
                    }
                    ui.add_space(18.0);
                    ui.separator();
                    ui.add_space(12.0);
                    let document = self.detail_documents.entry(detail).or_insert_with(|| {
                        [m.readme(), "README.md", "readme.md"]
                            .iter()
                            .find_map(|name| {
                                let path = folder.join(name);
                                let meta = std::fs::metadata(&path).ok()?;
                                (meta.len() <= 65536)
                                    .then(|| std::fs::read_to_string(path).ok())
                                    .flatten()
                            })
                            .unwrap_or_default()
                    });
                    let width = ui.available_width();
                    if width > 700.0 {
                        ui.horizontal_top(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(width - 260.0, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    document_ui(
                                        ui,
                                        &mut self.detail_markdown_cache,
                                        &folder,
                                        &m,
                                        document,
                                    );
                                },
                            );
                            ui.separator();
                            ui.allocate_ui_with_layout(
                                egui::vec2(230.0, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    metadata_ui(
                                        ui,
                                        &m,
                                        catalog.as_ref().map(|(c, _)| c.requirements.as_str()),
                                    );
                                },
                            );
                        });
                    } else {
                        document_ui(ui, &mut self.detail_markdown_cache, &folder, &m, document);
                        ui.add_space(16.0);
                        ui.separator();
                        metadata_ui(
                            ui,
                            &m,
                            catalog.as_ref().map(|(c, _)| c.requirements.as_str()),
                        );
                    }
                    if let Some(runtime) = self.runtimes.get_mut(&m.id) {
                        ui.add_space(12.0);
                        ui.collapsing(xpeed_i18n::t("Configuración del proveedor"), |ui| {
                            ui.label(xpeed_i18n::t(
                                "Ruta del ejecutable del proveedor (opcional)",
                            ));
                            if ui
                                .text_edit_singleline(&mut runtime.provider_path)
                                .lost_focus()
                            {
                                save = true;
                            }
                            ui.weak(&runtime.status);
                            if let Some(error) = &runtime.error {
                                ui.colored_label(ui.visuals().error_fg_color, error);
                            }
                        });
                    }
                });
            });
        if save {
            self.save();
        }
        if update && let Some((item, _)) = &catalog {
            self.install_git(item, ui.ctx());
        }
        if let Some(folder) = rollback {
            self.review_install(&folder);
        }
        if uninstall {
            self.request_uninstall(&m.id);
        }
    }
}
