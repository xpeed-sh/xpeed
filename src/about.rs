use crate::{
    palette::{Mode, Palette},
    svg_icons::{self, Icon},
};
use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, RichText, Stroke, Vec2};

pub const WEBSITE: &str = "https://xpeed.ronaldbit.com";
pub const REPOSITORY: &str = "https://github.com/xpeed-sh/xpeed";
const ORGANIZATION: &str = "https://github.com/xpeed-sh";
const DOCUMENTATION: &str = "https://github.com/xpeed-sh/xpeed#readme";
const CHANGELOG: &str = "https://github.com/xpeed-sh/xpeed/blob/main/CHANGELOG.md";

fn card(p: Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.raised.gamma_multiply(0.4))
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(10)
        .inner_margin(12)
}

fn link(
    ui: &mut egui::Ui,
    label: &str,
    url: &str,
    width: f32,
    primary: bool,
    p: Palette,
) -> egui::Response {
    let text_color = if primary {
        if p.mode == Mode::Dark {
            p.bg
        } else {
            Color32::WHITE
        }
    } else {
        p.text
    };
    let response = ui
        .add_sized(
            [width, 38.],
            egui::Button::new(RichText::new(label).size(12.).color(text_color))
                .fill(if primary { p.accent } else { p.raised })
                .stroke(Stroke::new(
                    1.0_f32,
                    if primary { p.accent } else { p.border },
                ))
                .corner_radius(8),
        )
        .on_hover_text(url);
    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
    response
}

fn stat(ui: &mut egui::Ui, title: &str, subtitle: &str, icon: Icon, width: f32, p: Palette) {
    card(p).show(ui, |ui| {
        ui.set_width((width - 26.).max(0.));
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(34.), egui::Sense::hover());
            ui.painter().rect_filled(rect, 7, p.raised);
            svg_icons::draw(ui.painter(), rect.shrink(7.), icon, p.accent);
            ui.vertical(|ui| {
                ui.add(
                    egui::Label::new(RichText::new(title).size(11.).strong().color(p.text)).wrap(),
                );
                ui.add(egui::Label::new(RichText::new(subtitle).size(10.).color(p.muted)).wrap());
            });
        });
    });
}

pub fn show(ui: &mut egui::Ui, p: Palette, active: usize) {
    egui::ScrollArea::vertical().id_salt("about_scroll").show(ui, |ui| {
        egui::Frame::new().inner_margin(14).show(ui, |ui| {
            let width = ui.available_width();
            ui.add_space(10.);
            ui.vertical_centered(|ui| {
                let logo_width = width.min(290.);
                let logo_height = logo_width / svg_icons::logo_size(1.).x;
                let (rect, _) = ui.allocate_exact_size(Vec2::new(logo_width, logo_height), egui::Sense::hover());
                svg_icons::draw(ui.painter(), rect, Icon::Logo, p.accent);
                ui.add_space(14.);
                ui.horizontal(|ui| {
                    ui.add_space(((width - 80.) / 2.).max(0.));
                    egui::Frame::new().fill(p.raised).stroke(Stroke::new(1.0_f32, p.border)).corner_radius(8).inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| {
                        ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).monospace().size(11.).color(p.accent));
                    });
                });
                ui.add_space(12.);
                ui.add(egui::Label::new(RichText::new(xpeed_i18n::t("Tu espacio de trabajo para terminales y servidores.")).size(14.).strong().color(p.text)).wrap());
                ui.add_space(6.);
                ui.add(egui::Label::new(RichText::new(xpeed_i18n::t("Terminal local, conexiones SSH, archivos remotos y extensiones.")).size(11.).color(p.muted)).wrap());
            });
            ui.add_space(20.);
            let connections = format!("{active} {}", if active == 1 { xpeed_i18n::t("conexión abierta") } else { xpeed_i18n::t("conexiones abiertas") });
            let os = match std::env::consts::OS { "windows" => "Windows", "macos" => "macOS", "linux" => "Linux", other => other };
            let system = format!("{os} · {}", std::env::consts::ARCH);
            if width >= 310. {
                let half = (width - ui.spacing().item_spacing.x) / 2.;
                ui.horizontal_top(|ui| {
                    stat(ui, &connections, xpeed_i18n::t("Conexiones activas en esta sesión."), Icon::Connection, half, p);
                    stat(ui, &system, xpeed_i18n::t("Sistema operativo de esta aplicación."), Icon::Info, half, p);
                });
            } else {
                stat(ui, &connections, xpeed_i18n::t("Conexiones activas en esta sesión."), Icon::Connection, width, p);
                ui.add_space(8.);
                stat(ui, &system, xpeed_i18n::t("Sistema operativo de esta aplicación."), Icon::Info, width, p);
            }
            ui.add_space(16.);
            link(ui, xpeed_i18n::t("Web de XPEED ↗"), WEBSITE, width, true, p);
            ui.add_space(8.);
            link(ui, xpeed_i18n::t("Ver repositorio ↗"), REPOSITORY, width, false, p);
            ui.add_space(8.);
            if width >= 310. {
                let half = (width - ui.spacing().item_spacing.x) / 2.;
                ui.horizontal(|ui| {
                    link(ui, xpeed_i18n::t("Documentación"), DOCUMENTATION, half, false, p);
                    link(ui, xpeed_i18n::t("Novedades"), CHANGELOG, half, false, p);
                });
            } else {
                link(ui, xpeed_i18n::t("Documentación"), DOCUMENTATION, width, false, p);
                ui.add_space(8.);
                link(ui, xpeed_i18n::t("Novedades"), CHANGELOG, width, false, p);
            }
            ui.add_space(20.);
            card(p).show(ui, |ui| {
                ui.set_width((width - 26.).max(0.));
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(48.), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 24., p.accent);
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "RR", FontId::proportional(19.), if p.mode == Mode::Dark {p.bg} else {Color32::WHITE});
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Ronald Ramos").strong().size(13.).color(p.text));
                        ui.add(egui::Label::new(RichText::new(xpeed_i18n::t("Creador y desarrollador de XPEED")).size(10.).color(p.muted)).wrap());
                        ui.hyperlink_to(RichText::new(xpeed_i18n::t("Organización en GitHub ↗")).size(11.).color(p.accent), ORGANIZATION);
                    });
                });
            });
            ui.add_space(20.);
            ui.separator();
            ui.add_space(6.);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.menu_button(RichText::new(xpeed_i18n::t("Licencia MIT")).size(11.).color(p.accent), |ui| {
                    ui.set_max_width(440.);
                    egui::ScrollArea::vertical().max_height(300.).show(ui, |ui| { ui.add(egui::Label::new(include_str!("../LICENSE")).wrap()); });
                });
                let technical = xpeed_i18n::format("Xpeed {}\nSistema: {} / {}\nTema: {}\nConexiones activas: {}\nWeb: {}\nRepositorio: {}", &[("0", env!("CARGO_PKG_VERSION").to_string()), ("1", std::env::consts::OS.to_string()), ("2", std::env::consts::ARCH.to_string()), ("3", p.name.to_string()), ("4", format!("{}", active)), ("5", WEBSITE.to_string()), ("6", REPOSITORY.to_string())]);
                if ui.button(RichText::new(xpeed_i18n::t("Copiar información técnica")).size(11.)).clicked() { ui.ctx().copy_text(technical); }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn website_and_repository_buttons_open_the_declared_destinations() {
        let ctx = egui::Context::default();
        for url in [WEBSITE, REPOSITORY] {
            let mut rect = egui::Rect::NOTHING;
            let input = |events| egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(400., 200.),
                )),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input(vec![]), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rect = link(ui, "Abrir", url, 200., false, Palette::default()).rect;
                });
            });
            let pos = rect.center();
            let mut commands = vec![];
            for pressed in [true, false] {
                let output = ctx.run(
                    input(vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]),
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            link(ui, "Abrir", url, 200., false, Palette::default());
                        });
                    },
                );
                commands.extend(output.platform_output.commands);
            }
            assert!(commands.iter().any(
                |command| matches!(command, egui::OutputCommand::OpenUrl(open) if open.url == url)
            ));
        }
    }
}
