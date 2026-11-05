//! The launcher window is always 16:9, the ratio its screens are drawn for.
//!
//! It is never resizable by hand: a fixed size (minimum = maximum) keeps the
//! ratio on every OS, and tiling compositors (Hyprland, Sway…) open fixed-size
//! windows floating above the tiled ones. Agrandir switches between two 16:9
//! sizes instead.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{LogicalSize, State, WebviewWindow};

const RATIO: f64 = 16.0 / 9.0;
/// The size the screens are drawn for (App.svelte), unless the monitor is too small.
const BASE: f64 = 1280.0;
/// Share of the monitor the normal size may take at most.
const NORMAL_MAX: f64 = 0.9;
/// Share of the monitor the large size takes: on Wayland the work area still
/// counts the bars, so the whole of it would slide under them.
const LARGE: f64 = 0.95;

/// Whether the window is at its large size.
#[derive(Default)]
pub struct Enlarged(AtomicBool);

/// Width of the largest 16:9 window filling `share` of the monitor.
fn fit(win: &WebviewWindow, share: f64) -> Option<f64> {
    let monitor = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten())?;
    let area = monitor.work_area().size.to_logical::<f64>(monitor.scale_factor());
    Some((area.width * share).min(area.height * share * RATIO).floor())
}

fn apply(win: &WebviewWindow, width: f64) {
    let size = LogicalSize::new(width.round(), (width / RATIO).round());
    // A minimum equal to the maximum rather than `resizable: false`, with which
    // GTK grows the window to the page's natural height.
    let _ = win.set_min_size(None::<LogicalSize<f64>>);
    let _ = win.set_max_size(None::<LogicalSize<f64>>);
    let _ = win.set_size(size);
    let _ = win.set_min_size(Some(size));
    let _ = win.set_max_size(Some(size));
    let _ = win.center();
}

fn normal(win: &WebviewWindow) -> f64 {
    fit(win, NORMAL_MAX).map_or(BASE, |w| w.min(BASE))
}

/// Sizes the window for its monitor when the launcher starts.
pub fn init(win: &WebviewWindow) {
    apply(win, normal(win));
}

#[tauri::command]
pub fn window_toggle_size(window: WebviewWindow, enlarged: State<'_, Enlarged>) {
    let large = !enlarged.0.fetch_xor(true, Ordering::Relaxed);
    let width = if large { fit(&window, LARGE).unwrap_or(BASE) } else { normal(&window) };
    apply(&window, width);
}
