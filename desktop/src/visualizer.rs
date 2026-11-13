use crate::app;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, Webview};

static UI_ACTIVE: AtomicBool = AtomicBool::new(false);
static LAST_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn active() -> bool {
    LAST_ACTIVE.load(Ordering::Relaxed)
}

pub fn sync(app: &AppHandle) {
    let active = UI_ACTIVE.load(Ordering::Relaxed)
        && app.get_window("main").is_some_and(|window| {
            window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(true)
        });
    if LAST_ACTIVE.swap(active, Ordering::Relaxed) != active {
        app::configure_player(app);
        if !active {
            let _ = app.emit_to("main", "visualizer", [0u8; 32]);
        }
    }
}

#[tauri::command]
pub fn visualizer_active(window: Webview, app: AppHandle, active: bool) -> Result<(), String> {
    app::local(&window)?;
    UI_ACTIVE.store(active, Ordering::Relaxed);
    sync(&app);
    Ok(())
}
