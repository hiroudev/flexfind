//! Show/hide/toggle commands for FlexFind's two windows. Both are declared
//! in `tauri.conf.json` with `visible: false` and always exist — this module
//! only ever shows/hides/focuses them, never creates or destroys, which
//! avoids get-or-create bookkeeping. The main window is a normal resizable
//! tool window (v2); its size/position/maximized are restored by
//! `tauri-plugin-window-state`, so there's no manual positioning here
//! anymore (v1's cursor-monitor overlay placement is gone).

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

fn main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "no main window".to_string())
}

fn settings_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("settings")
        .ok_or_else(|| "no settings window".to_string())
}

fn do_show_main(win: &WebviewWindow) -> Result<(), String> {
    if win.is_minimized().map_err(|e| e.to_string())? {
        win.unminimize().map_err(|e| e.to_string())?;
    }
    win.show().map_err(|e| e.to_string())?;
    win.set_focus().map_err(|e| e.to_string())?;
    // Tells the frontend to focus + select-all the search input (NOT reset —
    // tabs and their queries must survive a re-summon).
    win.emit("main-window-shown", ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) -> Result<(), String> {
    do_show_main(&main_window(&app)?)
}

#[tauri::command]
pub fn hide_main_window(app: AppHandle) -> Result<(), String> {
    main_window(&app)?.hide().map_err(|e| e.to_string())
}

/// Global-hotkey behavior for a normal (non-overlay) window: if it's visible
/// AND focused, hide it; if visible but not focused (behind other windows or
/// minimized), bring it to the front; if hidden, show it. Toggling on
/// visibility alone would hide a window the user can't even see.
#[tauri::command]
pub fn toggle_main_window(app: AppHandle) -> Result<(), String> {
    let win = main_window(&app)?;
    let visible = win.is_visible().map_err(|e| e.to_string())?;
    let minimized = win.is_minimized().map_err(|e| e.to_string())?;
    let focused = win.is_focused().map_err(|e| e.to_string())?;
    if visible && focused && !minimized {
        win.hide().map_err(|e| e.to_string())
    } else {
        do_show_main(&win)
    }
}

#[tauri::command]
pub fn show_settings(app: AppHandle) -> Result<(), String> {
    let win = settings_window(&app)?;
    win.show().map_err(|e| e.to_string())?;
    win.set_focus().map_err(|e| e.to_string())
}
