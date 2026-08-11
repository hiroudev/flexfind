//! Global-hotkey registration via `tauri-plugin-global-shortcut`. The
//! trigger handler itself (toggle main-window visibility) is wired in
//! `lib.rs`'s plugin builder — this module only owns (re)registration.

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

/// Unregisters any previously-registered accelerator and registers
/// `accelerator` (e.g. "Ctrl+Space") as the new global hotkey. Called once
/// at startup from persisted settings, and again whenever the user changes
/// the binding in the settings window's hotkey-capture UI.
///
/// Parses `accelerator` *before* touching the existing registration, so a
/// malformed string (or one `unregister_all`+`register` would otherwise
/// reject) can't leave the app with no hotkey registered at all — the old
/// binding stays live until a syntactically valid replacement is confirmed.
/// An OS-level conflict (some other app already owns the combo) can still
/// fail at the `register` step below; the caller (the settings window) is
/// responsible for re-applying the previous accelerator in that case too.
pub fn apply_hotkey(app: &AppHandle, accelerator: &str) -> Result<(), String> {
    let shortcut: Shortcut = accelerator
        .parse()
        .map_err(|e| format!("invalid accelerator \"{accelerator}\": {e}"))?;
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    gs.register(shortcut)
        .map_err(|e| format!("failed to register accelerator \"{accelerator}\": {e}"))
}

#[tauri::command]
pub fn register_hotkey(app: AppHandle, accelerator: String) -> Result<(), String> {
    apply_hotkey(&app, &accelerator)
}
