use alacritty_terminal::vte::ansi::{Color, NamedColor};
use eframe::egui::{self, Color32, FontId, Stroke, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Dark,
    Light,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub name: &'static str,
    pub mode: Mode,
    pub bg: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub terminal: Color32,
    ansi: [Color32; 16],
}
const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}
impl Default for Palette {
    fn default() -> Self {
        Self::new(Mode::Dark, 0)
    }
}
impl Palette {
    pub fn names(mode: Mode) -> [&'static str; 3] {
        match mode {
            Mode::Dark => ["Carbon", "Midnight", "Forest"],
            Mode::Light => ["Porcelain", "Paper", "Ice"],
        }
    }
    pub fn new(mode: Mode, theme: usize) -> Self {
        let (bg, panel, raised, border, text, muted, dim, accent, terminal) =
            match (mode, theme.min(2)) {
                (Mode::Dark, 0) => (
                    0x0b0d10, 0x101216, 0x1e2229, 0x292d35, 0xe4e8f0, 0x98a1b2, 0x717b8f, 0x75a7ff,
                    0x17191d,
                ),
                (Mode::Dark, 1) => (
                    0x0e1020, 0x14182b, 0x212743, 0x303956, 0xe4e8ff, 0xa0accd, 0x7d8bb2, 0xb0a1ff,
                    0x181e33,
                ),
                (Mode::Dark, _) => (
                    0x0d1513, 0x111e19, 0x1c3027, 0x2d4439, 0xe1eee5, 0x9cb9a5, 0x739383, 0xa7db8d,
                    0x17261e,
                ),
                (Mode::Light, 0) => (
                    0xf3f4f7, 0xffffff, 0xe9eef9, 0xe1e5ed, 0x233044, 0x5b6880, 0x738098, 0x3269da,
                    0xffffff,
                ),
                (Mode::Light, 1) => (
                    0xf1ede6, 0xfffcf5, 0xede5d9, 0xded6c9, 0x3d352e, 0x736353, 0x8b7968, 0x996329,
                    0xfffdf8,
                ),
                (Mode::Light, _) => (
                    0xeff5f8, 0xfafefe, 0xdeeff2, 0xd2e1e8, 0x183c4a, 0x507480, 0x668791, 0x147e98,
                    0xffffff,
                ),
            };
        let ansi = match mode {
            Mode::Dark => [
                0x343b47, 0xf07e8c, 0xa2cf86, 0xe8ca86, 0x82adf1, 0xc49af1, 0x7ecece, 0xd9e1ef,
                0x7f8ca3, 0xff98a8, 0xc0eaa2, 0xffdfa6, 0xa5c7ff, 0xd8b7ff, 0xa6eeee, 0xf9fbff,
            ],
            Mode::Light => [
                0x293242, 0xb02d48, 0x267341, 0x916418, 0x285bb5, 0x78429e, 0x117682, 0x536071,
                0x748197, 0xd23e56, 0x38882d, 0xa77616, 0x356dd2, 0x9654b4, 0x18838b, 0xadb5c0,
            ],
        }
        .map(rgb);
        Self {
            name: Self::names(mode)[theme.min(2)],
            mode,
            bg: rgb(bg),
            panel: rgb(panel),
            raised: rgb(raised),
            border: rgb(border),
            text: rgb(text),
            muted: rgb(muted),
            dim: rgb(dim),
            accent: rgb(accent),
            terminal: rgb(terminal),
            ansi,
        }
    }
    pub fn color(&self, color: Color) -> Color32 {
        match color {
            Color::Spec(rgb) => Color32::from_rgb(rgb.r, rgb.g, rgb.b),
            Color::Indexed(index) if index < 16 => self.ansi[index as usize],
            Color::Indexed(index) if index < 232 => {
                let n = index - 16;
                let component = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Color32::from_rgb(component(n / 36), component(n / 6 % 6), component(n % 6))
            }
            Color::Indexed(index) => Color32::from_gray(8 + (index - 232) * 10),
            Color::Named(NamedColor::Background) => self.terminal,
            Color::Named(NamedColor::Cursor) => self.accent,
            Color::Named(NamedColor::DimForeground) => self.muted,
            Color::Named(named) => {
                let n = named as usize;
                if n < 16 {
                    self.ansi[n]
                } else if (259..267).contains(&n) {
                    self.ansi[n - 259].lerp_to_gamma(self.terminal, 0.3)
                } else {
                    self.text
                }
            }
        }
    }
    pub fn apply(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        style.visuals = if self.mode == Mode::Dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        style.visuals.override_text_color = Some(self.text);
        style.visuals.panel_fill = self.panel;
        style.visuals.window_fill = self.panel;
        style.visuals.extreme_bg_color = self.bg;
        style.visuals.faint_bg_color = self.raised;
        style.visuals.window_stroke = Stroke::new(1.0_f32, self.border);
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, self.border);
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, self.muted);
        style.visuals.selection.bg_fill = self.accent.lerp_to_gamma(self.panel, 0.82);
        style.visuals.selection.stroke = Stroke::new(1.0_f32, self.accent);
        style.visuals.widgets.inactive.bg_fill = self.raised;
        style.visuals.widgets.inactive.weak_bg_fill = self.raised;
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, self.border);
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, self.text);
        style.visuals.widgets.hovered.bg_fill = self.accent.lerp_to_gamma(self.panel, 0.86);
        style.visuals.widgets.hovered.weak_bg_fill = style.visuals.widgets.hovered.bg_fill;
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, self.accent);
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, self.text);
        style.spacing.item_spacing = Vec2::new(7.0, 7.0);
        style.spacing.button_padding = Vec2::new(10.0, 6.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(12.0));
        ctx.set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_theme_has_readable_terminal_defaults_and_changes_the_entire_surface() {
        for mode in [Mode::Dark, Mode::Light] {
            for index in 0..3 {
                let p = Palette::new(mode, index);
                let luminance = |c: Color32| {
                    (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
                };
                assert!((luminance(p.text) - luminance(p.terminal)).abs() > 0.6);
                assert_eq!(p.color(Color::Named(NamedColor::Background)), p.terminal);
                assert_eq!(p.color(Color::Named(NamedColor::Foreground)), p.text);
            }
        }
        assert_ne!(
            Palette::new(Mode::Dark, 0).panel,
            Palette::new(Mode::Dark, 1).panel
        );
    }
}
