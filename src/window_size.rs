//! Restored dimensions stay in logical units; native monitor bounds stay in pixels.
use eframe::egui::{self, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
struct Bounds {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}
#[derive(Clone, Copy)]
struct Snapshot {
    rect: Bounds,
    work: Bounds,
    monitor: usize,
    scale: f32,
    maximized: bool,
    minimized: bool,
    moving: bool,
}
#[cfg_attr(not(windows), derive(Default))]
pub struct WindowSize {
    restored: Option<Vec2>,
    previous_monitor: Option<(usize, Bounds, f32)>,
    was_maximized: bool,
    restore_requested: bool,
    previous_rect: Option<Bounds>,
    #[cfg(windows)]
    created: std::time::Instant,
}
#[cfg(windows)]
impl Default for WindowSize {
    fn default() -> Self {
        Self {
            restored: None,
            previous_monitor: None,
            was_maximized: false,
            restore_requested: false,
            previous_rect: None,
            #[cfg(windows)]
            created: std::time::Instant::now(),
        }
    }
}
impl WindowSize {
    pub fn toggle_maximized(&mut self, ctx: &egui::Context) {
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        self.restore_requested = maximized;
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    fn observe(&mut self, sample: Snapshot, _now: f64) -> Option<Bounds> {
        if sample.minimized
            || !sample.scale.is_finite()
            || sample.scale <= 0.0
            || sample.work.width < 1
            || sample.work.height < 1
        {
            return None;
        }
        if sample.moving {
            // Moving a maximized window restores it natively under the pointer.
            // Do not treat that as a request to move it back to its old display.
            if !sample.maximized {
                self.was_maximized = false;
                self.restore_requested = false;
            }
            return None;
        }
        let monitor = (sample.monitor, sample.work, sample.scale);
        let first_frame = self.previous_monitor.is_none();
        let moved_monitor = self.previous_monitor.is_some_and(|old| old != monitor);
        self.previous_monitor = Some(monitor);
        if sample.maximized {
            self.was_maximized = true;
            return None;
        }
        let restoring = self.was_maximized || self.restore_requested;
        let current = Vec2::new(sample.rect.width as f32, sample.rect.height as f32) / sample.scale;
        let manually_resized = !moved_monitor
            && self.previous_rect.is_some_and(|old| {
                old.width != sample.rect.width || old.height != sample.rect.height
            });
        if first_frame || (!restoring && manually_resized) {
            self.restored = Some(current);
        }
        self.was_maximized = false;
        self.restore_requested = false;
        self.previous_rect = Some(sample.rect);
        // Windows/winit own ordinary moves and DPI transitions. Never issue
        // geometry corrections while crossing a monitor or after releasing a drag.
        if !first_frame && !restoring {
            return None;
        }
        let preferred = self.restored.unwrap_or(Vec2::new(1100.0, 720.0));
        let margin = 16
            .min((sample.work.width - 1) / 2)
            .min((sample.work.height - 1) / 2);
        let width = (preferred.x * sample.scale)
            .round()
            .clamp(1.0, (sample.work.width - 2 * margin) as f32) as i32;
        let height = (preferred.y * sample.scale)
            .round()
            .clamp(1.0, (sample.work.height - 2 * margin) as f32) as i32;
        let target = Bounds {
            x: sample.rect.x.clamp(
                sample.work.x + margin,
                sample.work.x + sample.work.width - width - margin,
            ),
            y: sample.rect.y.clamp(
                sample.work.y + margin,
                sample.work.y + sample.work.height - height - margin,
            ),
            width,
            height,
        };
        // The fitted size is temporary. Keep the user's preferred dimensions
        // so a smaller monitor does not permanently shrink subsequent restores.
        self.previous_rect = Some(target);
        (target != sample.rect).then_some(target)
    }
    #[cfg(windows)]
    pub fn update(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::{
            Foundation::RECT,
            Graphics::Gdi::{
                GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
            },
            UI::{
                HiDpi::GetDpiForWindow,
                WindowsAndMessaging::{
                    GUI_INMOVESIZE, GUITHREADINFO, GetGUIThreadInfo, GetWindowRect,
                    GetWindowThreadProcessId, IsIconic, IsZoomed, SWP_NOACTIVATE, SWP_NOZORDER,
                    SetWindowPos,
                },
            },
        };
        let Ok(handle) = frame.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
        // The borrowed handle is valid for this frame; all outputs are initialized,
        // and Win32 calls run on the native window's UI thread.
        unsafe {
            if IsIconic(hwnd) != 0 {
                return;
            }
            let mut thread_info: GUITHREADINFO = std::mem::zeroed();
            thread_info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
            let moving = ctx.input(|i| i.pointer.primary_down())
                || GetGUIThreadInfo(
                    GetWindowThreadProcessId(hwnd, std::ptr::null_mut()),
                    &mut thread_info,
                ) != 0
                    && thread_info.flags & GUI_INMOVESIZE != 0;
            let maximized = IsZoomed(hwnd) != 0;
            let actual_monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            // Restoration may initially return coordinates from the other
            // screen. Use the display where the maximized window was in use.
            let restoring = !moving && !maximized && (self.was_maximized || self.restore_requested);
            let mut monitor = if restoring {
                self.previous_monitor
                    .map_or(actual_monitor, |old| old.0 as _)
            } else {
                actual_monitor
            };
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            let mut rect: RECT = std::mem::zeroed();
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                monitor = actual_monitor;
                if GetMonitorInfoW(monitor, &mut info) == 0 {
                    return;
                }
            }
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return;
            }
            let scale = if restoring && monitor != actual_monitor {
                self.previous_monitor.map_or(1.0, |old| old.2)
            } else {
                GetDpiForWindow(hwnd) as f32 / 96.0
            };
            let work = Bounds {
                x: info.rcWork.left,
                y: info.rcWork.top,
                width: info.rcWork.right - info.rcWork.left,
                height: info.rcWork.bottom - info.rcWork.top,
            };
            if scale <= 0.0 {
                return;
            }
            let ui_scale = ctx.pixels_per_point();
            let minimum = Vec2::new(
                (720.0 * scale).min((work.width - 32).max(1) as f32),
                (440.0 * scale).min((work.height - 32).max(1) as f32),
            ) / ui_scale;
            if !moving && (self.previous_monitor.is_none() || restoring) {
                ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(minimum));
            }
            let sample = Snapshot {
                rect: Bounds {
                    x: rect.left,
                    y: rect.top,
                    width: rect.right - rect.left,
                    height: rect.bottom - rect.top,
                },
                work,
                monitor: monitor as usize,
                scale,
                maximized,
                minimized: false,
                moving,
            };
            if let Some(target) = self.observe(sample, self.created.elapsed().as_secs_f64()) {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    target.x,
                    target.y,
                    target.width,
                    target.height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
                ctx.request_repaint();
            }
        }
    }
    #[cfg(not(windows))]
    pub fn update(&mut self, ctx: &egui::Context, _frame: &eframe::Frame) {
        let viewport = ctx.input(|i| i.viewport().clone());
        let (Some(rect), Some(monitor)) = (viewport.outer_rect, viewport.monitor_size) else {
            return;
        };
        let sample = Snapshot {
            rect: Bounds {
                x: 0,
                y: 0,
                width: rect.width() as i32,
                height: rect.height() as i32,
            },
            work: Bounds {
                x: 0,
                y: 0,
                width: monitor.x as i32,
                height: monitor.y as i32,
            },
            monitor: 0,
            scale: 1.0,
            maximized: viewport.maximized.unwrap_or(false),
            minimized: viewport.minimized.unwrap_or(false),
            moving: false,
        };
        if let Some(target) = self.observe(sample, ctx.input(|i| i.time)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                target.width as f32,
                target.height as f32,
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(rect: Bounds, work: Bounds, scale: f32) -> Snapshot {
        Snapshot {
            rect,
            work,
            scale,
            monitor: work.x as usize,
            maximized: false,
            minimized: false,
            moving: false,
        }
    }
    fn bounds(x: i32, y: i32, width: i32, height: i32) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }
    #[test]
    fn large_initial_window_fits_laptop_work_area() {
        let mut state = WindowSize::default();
        let work = bounds(0, 0, 1366, 728);
        let fixed = state
            .observe(sample(bounds(100, 100, 2560, 1640), work, 2.0), 0.0)
            .unwrap();
        assert_eq!(fixed, bounds(16, 16, 1334, 696));
    }
    #[test]
    fn restore_on_a_different_dpi_monitor_never_reuses_maximized_dimensions() {
        let mut state = WindowSize::default();
        let laptop = bounds(0, 0, 1920, 1040);
        assert!(
            state
                .observe(sample(bounds(50, 50, 1200, 800), laptop, 1.5), 0.0)
                .is_none()
        );
        let large = bounds(1920, -200, 3840, 2120);
        let mut maximized = sample(large, large, 2.0);
        maximized.maximized = true;
        assert!(state.observe(maximized, 1.0).is_none());
        let restored = state
            .observe(sample(bounds(1900, -200, 6000, 4000), large, 2.0), 2.0)
            .unwrap();
        assert_eq!((restored.width, restored.height), (1600, 1067));
        assert!(restored.x >= large.x && restored.x + restored.width <= large.x + large.width);
        assert!(restored.y >= large.y && restored.y + restored.height <= large.y + large.height);
    }
    #[test]
    fn minimized_geometry_is_ignored_and_dragging_can_cross_monitor_edges() {
        let mut state = WindowSize::default();
        let work = bounds(-1920, 0, 1920, 1040);
        assert!(
            state
                .observe(sample(bounds(-1850, 50, 1000, 700), work, 1.0), 0.0)
                .is_none()
        );
        let mut minimized = sample(bounds(-32000, -32000, 160, 30), work, 1.0);
        minimized.minimized = true;
        assert!(state.observe(minimized, 1.0).is_none());
        // Don't pin a user's drag inside the old monitor before it can cross.
        assert!(
            state
                .observe(sample(bounds(-100, 50, 1000, 700), work, 1.0), 2.0)
                .is_none()
        );
        assert_eq!(state.restored, Some(Vec2::new(1000.0, 700.0)));
    }
    #[test]
    fn crossing_monitors_never_repositions_or_resizes_the_window() {
        let mut state = WindowSize::default();
        let large = bounds(0, 0, 3840, 2120);
        state.observe(sample(bounds(100, 100, 2400, 1500), large, 1.0), 0.0);
        let preferred = state.restored;
        let small = bounds(-1366, -300, 1366, 728);
        for (i, work, scale) in [(1, small, 1.5), (2, large, 1.0), (3, small, 1.5)] {
            assert!(
                state
                    .observe(
                        sample(bounds(-1000, -200, 3600, 2250), work, scale),
                        i as f64
                    )
                    .is_none()
            );
            assert_eq!(state.restored, preferred);
        }
    }
    #[test]
    fn drag_from_maximized_and_mouse_release_never_snap_back() {
        let mut state = WindowSize::default();
        let large = bounds(0, 0, 3840, 2120);
        state.observe(sample(bounds(100, 100, 1200, 800), large, 1.0), 0.0);
        let mut maximized = sample(large, large, 1.0);
        maximized.maximized = true;
        state.observe(maximized, 1.0);
        let small = bounds(-1366, -300, 1366, 728);
        let mut drag = sample(bounds(-1000, -200, 1800, 1200), small, 1.5);
        drag.moving = true;
        assert!(state.observe(drag, 2.0).is_none());
        drag.moving = false;
        assert!(state.observe(drag, 3.0).is_none());
        assert_eq!(state.restored, Some(Vec2::new(1200.0, 800.0)));
    }
    #[test]
    fn small_monitor_restore_does_not_replace_preferred_size() {
        let mut state = WindowSize::default();
        let large = bounds(0, 0, 3840, 2120);
        state.observe(sample(bounds(100, 100, 2400, 1500), large, 1.0), 0.0);
        let small = bounds(-1366, -300, 1366, 728);
        let mut maximized = sample(small, small, 1.5);
        maximized.maximized = true;
        state.observe(maximized, 1.0);
        let fitted = state
            .observe(sample(bounds(-1000, -200, 3600, 2250), small, 1.5), 2.0)
            .unwrap();
        assert_eq!(fitted, bounds(-1350, -284, 1334, 696));
        assert!(state.observe(sample(fitted, small, 1.5), 3.0).is_none());
        assert_eq!(state.restored, Some(Vec2::new(2400.0, 1500.0)));
        maximized = sample(large, large, 1.0);
        maximized.maximized = true;
        state.observe(maximized, 4.0);
        let restored = state
            .observe(sample(bounds(100, 100, 1334, 696), large, 1.0), 5.0)
            .unwrap();
        assert_eq!((restored.width, restored.height), (2400, 1500));
    }
}
