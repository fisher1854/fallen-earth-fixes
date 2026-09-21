use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::RuntimeState;

/// Keep the in-game overlay click-through while playing.
/// Only touch Win32/WebView2 when the desired hit-test state changes —
/// hammering set_ignore_cursor_events every frame causes compositor hitching.
pub fn start_clickthrough_controller(app: AppHandle, state: Arc<RuntimeState>) {
    std::thread::spawn(move || {
        let mut last_ignore: Option<bool> = None;
        loop {
            let edit_mode = state.edit_mode.lock().map(|value| *value).unwrap_or(false);
            let layout_unlocked = state
                .layout_unlocked
                .lock()
                .map(|value| *value)
                .unwrap_or(false);
            // Overlay never steals cursor during normal play or while Control Center is open.
            // Only an explicit "Unlock layout" from Control Center enables map dragging.
            let ignore = !(layout_unlocked && !edit_mode);
            if last_ignore != Some(ignore) {
                if let Some(window) = app.get_webview_window("main") {
                    if window.set_ignore_cursor_events(ignore).is_ok() {
                        last_ignore = Some(ignore);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });
}
