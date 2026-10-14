//! Structured extension dashboards, rendered with the current application theme.
use eframe::egui::{self, RichText, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub context_id: Option<u64>,
    pub caption: String,
    pub status: String,
    #[serde(default)]
    pub metrics: Vec<Metric>,
    #[serde(default)]
    pub processes: Vec<Process>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Metric {
    pub label: String,
    pub display: String,
    pub max: f64,
    pub history: Vec<f64>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Process {
    pub pid: u32,
    pub start: u64,
    pub name: String,
    pub cpu: f64,
    pub memory: f64,
    pub memory_bytes: u64,
}
impl Dashboard {
    pub fn valid(&self) -> bool {
        self.caption.len() <= 512
            && self.status.len() <= 1024
            && self.metrics.len() <= 12
            && self.processes.len() <= 100
            && self.metrics.iter().all(|m| {
                m.label.len() <= 128
                    && m.display.len() <= 128
                    && m.max.is_finite()
                    && m.max > 0.
                    && m.history.len() <= 120
                    && m.history.iter().all(|n| n.is_finite() && *n >= 0.)
            })
            && self.processes.iter().all(|p| {
                p.pid > 0
                    && p.name.len() <= 256
                    && p.cpu.is_finite()
                    && p.cpu >= 0.
                    && p.memory.is_finite()
                    && p.memory >= 0.
            })
    }
}
#[derive(Default)]
pub struct State {
    memory_sort: bool,
    filter: String,
    selected: Option<(u32, u64)>,
}
pub fn show(ui: &mut egui::Ui, dashboard: &Dashboard, state: &mut State) {
    ui.add(egui::Label::new(RichText::new(&dashboard.caption).strong()).wrap());
    ui.add(egui::Label::new(xpeed_i18n::t(&dashboard.status)).wrap());
    let color = ui.visuals().selection.stroke.color;
    for metric in &dashboard.metrics {
        ui.add_space(5.);
        ui.horizontal(|ui| {
            ui.label(RichText::new(xpeed_i18n::t(&metric.label)).strong());
            ui.label(xpeed_i18n::t(&metric.display));
        });
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 45.), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 5, ui.visuals().faint_bg_color);
        if metric.history.len() > 1 {
            let points = metric
                .history
                .iter()
                .enumerate()
                .map(|(i, value)| {
                    egui::pos2(
                        rect.left() + i as f32 / (metric.history.len() - 1) as f32 * rect.width(),
                        rect.bottom()
                            - ((*value / metric.max).clamp(0., 1.) as f32 * (rect.height() - 4.))
                            - 2.,
                    )
                })
                .collect();
            ui.painter()
                .add(egui::Shape::line(points, egui::Stroke::new(1.8_f32, color)));
        }
    }
    if dashboard.processes.is_empty() {
        return;
    }
    ui.separator();
    ui.label(RichText::new(xpeed_i18n::t("Procesos que más consumen")).strong());
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.memory_sort, false, xpeed_i18n::t("CPU"));
        ui.selectable_value(&mut state.memory_sort, true, xpeed_i18n::t("RAM"));
    });
    ui.add(
        egui::TextEdit::singleline(&mut state.filter)
            .hint_text(xpeed_i18n::t("Buscar proceso o PID"))
            .desired_width(f32::INFINITY),
    );
    let filter = state.filter.to_lowercase();
    let mut processes: Vec<_> = dashboard
        .processes
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&filter) || p.pid.to_string().contains(&filter))
        .collect();
    processes.sort_by(|a, b| {
        let value = |p: &Process| if state.memory_sort { p.memory } else { p.cpu };
        value(b)
            .total_cmp(&value(a))
            .then_with(|| a.pid.cmp(&b.pid))
    });
    if let Some((pid, start)) = state.selected {
        if let Some(process) = dashboard
            .processes
            .iter()
            .find(|p| p.pid == pid && p.start == start)
        {
            ui.separator();
            ui.add(egui::Label::new(RichText::new(&process.name).strong()).wrap());
            ui.label(xpeed_i18n::format(
                "PID {} · CPU {:.1}% del servidor",
                &[
                    ("0", format!("{}", pid)),
                    ("1:.1", format!("{:.1}", process.cpu)),
                ],
            ));
            ui.label(xpeed_i18n::format(
                "RAM residente: {:.1} MiB",
                &[(
                    "0:.1",
                    format!("{:.1}", process.memory_bytes as f64 / 1048576.),
                )],
            ));
            if ui.button(xpeed_i18n::t("Copiar PID")).clicked() {
                ui.ctx().copy_text(pid.to_string());
            }
        } else {
            ui.label(xpeed_i18n::t(
                "El proceso seleccionado ya no figura en la muestra.",
            ));
        }
    }
    for process in processes.iter().take(20) {
        ui.push_id((process.pid, process.start), |ui| {
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 115.).max(40.);
                let label = format!("{} · {}", process.pid, process.name);
                if ui
                    .add_sized(
                        [width, 22.],
                        egui::Button::selectable(
                            state.selected == Some((process.pid, process.start)),
                            label,
                        )
                        .truncate(),
                    )
                    .on_hover_text(&process.name)
                    .clicked()
                {
                    state.selected = Some((process.pid, process.start));
                }
                ui.label(format!("{:.1}%", process.cpu))
                    .on_hover_text(xpeed_i18n::t("CPU del servidor"));
                ui.label(format!("{:.1}%", process.memory))
                    .on_hover_text(xpeed_i18n::t("RAM del servidor"));
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dashboards_reject_unbounded_and_non_finite_data() {
        let mut dashboard = Dashboard {
            caption: "Test".into(),
            metrics: vec![Metric {
                label: "CPU".into(),
                display: "25%".into(),
                max: 100.,
                history: vec![25.],
            }],
            ..Default::default()
        };
        assert!(dashboard.valid());
        dashboard.metrics[0].history.push(f64::NAN);
        assert!(!dashboard.valid());
        dashboard.metrics[0].history = vec![0.; 121];
        assert!(!dashboard.valid());
    }
}
