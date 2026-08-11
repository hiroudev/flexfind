//! Native tray icon + right-click menu (design screen 3 — pure Rust, no
//! frontend involved): 開く / 索引を一時停止⇄索引を再開 / 設定 / 終了.

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::index::IndexEngine;
use crate::windows;

/// Kept in managed state so the click handler can flip the pause/resume
/// item's label live via `set_text` instead of rebuilding the whole menu.
pub struct TrayHandles {
    pub toggle_pause: MenuItem<tauri::Wry>,
}

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open", "開く", true, None::<&str>)?;
    let toggle_pause_item =
        MenuItem::with_id(app, "toggle_pause", "索引を一時停止", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "設定", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[
            &open_item,
            &toggle_pause_item,
            &sep1,
            &settings_item,
            &sep2,
            &quit_item,
        ],
    )?;

    app.manage(TrayHandles {
        toggle_pause: toggle_pause_item,
    });

    TrayIconBuilder::new()
        .icon(app.default_window_icon().expect("no default window icon").clone())
        .menu(&menu)
        // Left click opens the window and activates the search box (menu is
        // right-click only); the menu's "開く" item does the same thing.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = windows::show_main_window(tray.app_handle().clone());
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => {
                let _ = windows::show_main_window(app.clone());
            }
            "toggle_pause" => {
                let engine = app.state::<Arc<IndexEngine>>();
                let now_paused = !engine.is_paused();
                engine.set_paused(now_paused);
                if let Some(handles) = app.try_state::<TrayHandles>() {
                    let label = if now_paused { "索引を再開" } else { "索引を一時停止" };
                    let _ = handles.toggle_pause.set_text(label);
                }
            }
            "settings" => {
                let _ = windows::show_settings(app.clone());
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}
