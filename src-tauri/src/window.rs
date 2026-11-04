use tauri::{AppHandle, LogicalSize, Manager, Size};

/// Default logical window size used when a persisted geometry is unusable.
const DEFAULT_WIDTH: f64 = 1440.0;
const DEFAULT_HEIGHT: f64 = 900.0;

/// Clamp a restored window against the current monitor layout.
///
/// `tauri-plugin-window-state` persists physical pixels. On mixed-DPI setups
/// Windows can inflate a window to the Win32 16-bit coordinate limit while it is
/// dragged between monitors, and the plugin will happily restore that size on
/// every launch. This guard catches that case (and off-screen positions) and
/// falls back to a sane centered window.
pub fn clamp_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let Ok(monitors) = window.available_monitors() else {
        return;
    };
    if monitors.is_empty() {
        return;
    }

    let max_width = monitors
        .iter()
        .map(|monitor| monitor.size().width)
        .max()
        .unwrap_or(1920);
    let max_height = monitors
        .iter()
        .map(|monitor| monitor.size().height)
        .max()
        .unwrap_or(1080);

    // A genuine window is never smaller than this, and no real display pair
    // legitimately needs a window twice as large as the biggest monitor.
    let too_small = size.width < 800 || size.height < 560;
    let too_large = size.width > max_width.saturating_mul(2)
        || size.height > max_height.saturating_mul(2);

    if too_small || too_large {
        log::warn!(
            "clamping window {}x{} against max {}x{}",
            size.width,
            size.height,
            max_width,
            max_height
        );
        let _ = window.set_size(Size::Logical(LogicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT)));
        let _ = window.center();
        return;
    }

    if let Ok(position) = window.outer_position() {
        let center_x = position.x + (size.width as i32) / 2;
        let center_y = position.y + (size.height as i32) / 2;
        let inside = monitors.iter().any(|monitor| {
            let origin = monitor.position();
            let area = monitor.size();
            center_x >= origin.x
                && center_x <= origin.x + area.width as i32
                && center_y >= origin.y
                && center_y <= origin.y + area.height as i32
        });
        if !inside {
            log::warn!(
                "window position {},{} is outside every monitor, centering",
                position.x,
                position.y
            );
            let _ = window.center();
        }
    }
}

/// Install a `ScaleFactorChanged` guard: Windows emits this when a window is
/// dragged across monitors with different scaling, and the size can be
/// multiplied after the event. Re-check slightly later on the main thread.
pub fn install_scale_guard(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::ScaleFactorChanged { .. } = event {
            let app = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(180));
                let app_for_main = app.clone();
                let _ = app.run_on_main_thread(move || clamp_main_window(&app_for_main));
            });
        }
    });
}
