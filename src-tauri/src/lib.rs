mod icons;
mod index;
mod settings;
mod shell;
mod shortcut;
mod sibling;
mod tray;
mod windows;

use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::ShortcutState;

/// Index-freshness strategy (see project plan): no live filesystem watching,
/// just a full re-walk on a schedule. The thread wakes once a minute and
/// consults `settings.json` live, so the user can switch between an interval
/// walk, a once-a-day walk at a chosen time, or manual-only without a
/// restart. A cycle is skipped entirely while paused (tray "索引を一時停止")
/// or already scanning, rather than queuing a rebuild that would immediately
/// undo the pause / waste an in-flight walk.
const REINDEX_TICK: std::time::Duration = std::time::Duration::from_secs(60);

/// Parse a settings "HH:MM" (24h) into a `NaiveTime`; `None` on garbage so a
/// malformed value simply disables the daily trigger instead of panicking.
fn parse_daily_time(s: &str) -> Option<chrono::NaiveTime> {
    chrono::NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

fn spawn_periodic_reindex(engine: Arc<index::IndexEngine>, app: AppHandle) {
    use chrono::Local;
    std::thread::spawn(move || {
        // The startup scan (in `setup`) counts as "just scanned", so the
        // interval clock starts now. For daily mode, treat today as already
        // covered if we launched after today's target time — otherwise a
        // startup past 03:00 would fire a second full walk within a minute.
        let start = Local::now();
        let mut last_interval_scan = start;
        let s0 = settings::load_settings(&app);
        let mut last_daily_date = parse_daily_time(&s0.reindex_daily_time)
            .filter(|t| start.time() >= *t)
            .map(|_| start.date_naive());

        loop {
            std::thread::sleep(REINDEX_TICK);
            if engine.is_paused() || engine.any_scanning() {
                continue;
            }
            let s = settings::load_settings(&app);
            let now = Local::now();
            let should_scan = match s.reindex_mode.as_str() {
                "manual" => false,
                "daily" => match parse_daily_time(&s.reindex_daily_time) {
                    Some(t) => now.time() >= t && last_daily_date != Some(now.date_naive()),
                    None => false,
                },
                // "interval" (and any unknown value falls back to it).
                _ => {
                    let mins = s.reindex_interval_minutes.max(1) as i64;
                    now.signed_duration_since(last_interval_scan).num_minutes() >= mins
                }
            };
            if should_scan {
                last_interval_scan = now;
                if s.reindex_mode == "daily" {
                    last_daily_date = Some(now.date_naive());
                }
                index::IndexEngine::spawn_scan(engine.clone(), app.clone());
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // Only one accelerator is ever registered at a time (see
                    // `shortcut::apply_hotkey`), so any trigger toggles the
                    // main window without inspecting which shortcut fired.
                    if event.state() == ShortcutState::Pressed {
                        let _ = windows::toggle_main_window(app.clone());
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .plugin(tauri_plugin_dialog::init())
        // Persist/restore the main window's size/position/maximized across
        // sessions. VISIBLE is excluded so `start_minimized_to_tray` keeps
        // owning startup visibility; the settings window is denylisted so
        // its centered fixed geometry is unaffected.
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        & !tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .with_denylist(&["settings"])
                .build(),
        )
        .manage(Arc::new(index::IndexEngine::new()))
        .invoke_handler(tauri::generate_handler![
            index::engine::search_index,
            index::engine::get_index_status,
            index::engine::rebuild_index,
            index::engine::set_index_paused,
            index::engine::get_index_memory,
            index::engine::trim_working_set,
            settings::get_settings,
            settings::set_settings,
            settings::set_columns,
            settings::pick_folder,
            windows::show_main_window,
            windows::hide_main_window,
            windows::toggle_main_window,
            windows::show_settings,
            shortcut::register_hotkey,
            shell::open_path,
            shell::launch_app,
            shell::duplicate_as_dated_copy,
            shell::reveal_in_explorer,
            shell::shell_verb,
            shell::elevate_restart,
            shell::is_elevated,
            sibling::find_sibling_apps,
            icons::shell_icon,
            icons::shell_icon_for_path,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tray::build_tray(&handle)?;

            let s = settings::load_settings(&handle);
            if let Err(e) = shortcut::apply_hotkey(&handle, &s.hotkey) {
                eprintln!("FlexFind: failed to register hotkey \"{}\": {e}", s.hotkey);
            }

            // `settings.json` is the single source of truth for
            // launch-at-login — reconcile the actual Windows startup entry
            // to match it on every launch, rather than only ever touching
            // it from the settings-window toggle handler (which left a
            // fresh install showing "ON" with no entry actually created,
            // since the default is `true` but nothing had ever called
            // `enable()`).
            {
                use tauri_plugin_autostart::ManagerExt;
                let autolaunch = handle.autolaunch();
                let currently_enabled = autolaunch.is_enabled().unwrap_or(false);
                if s.launch_at_login && !currently_enabled {
                    let _ = autolaunch.enable();
                } else if !s.launch_at_login && currently_enabled {
                    let _ = autolaunch.disable();
                }
            }

            // Drop persisted index files for roots no longer configured, so
            // stale `.idx` files don't accumulate.
            if let Ok(data_dir) = handle.path().app_data_dir() {
                let known: Vec<String> = s.scan_roots.iter().map(|r| r.path.clone()).collect();
                index::persist::cleanup(&data_dir, &known);
            }

            let engine = handle.state::<Arc<index::IndexEngine>>().inner().clone();
            // Load the persisted per-root indexes first so results are
            // searchable immediately, then kick off the live refresh walk.
            engine.load_persisted(&handle);
            index::IndexEngine::spawn_scan(engine.clone(), handle.clone());
            spawn_periodic_reindex(engine, handle.clone());

            // "起動時にトレイに最小化" OFF means the main window should
            // actually appear at startup instead of staying hidden in tray.
            if !s.start_minimized_to_tray {
                let _ = windows::show_main_window(handle.clone());
            }

            // The main window only ever hides, never truly closes — this is
            // what makes the app tray-resident. Unlike v1 there is NO
            // focus-loss hide: it's a normal window now, not a launcher
            // overlay, so it stays put when the user clicks elsewhere.
            if let Some(main) = handle.get_webview_window("main") {
                let main_for_events = main.clone();
                main.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = main_for_events.hide();
                    }
                });
            }
            if let Some(settings_window) = handle.get_webview_window("settings") {
                let settings_for_events = settings_window.clone();
                settings_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = settings_for_events.hide();
                    }
                });
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building FlexFind")
        .run(|_app_handle, event| {
            // Both windows' CloseRequested handlers above turn "close" into
            // "hide". `ExitRequested` fires for BOTH that natural
            // "all windows closed" path (code: None) AND explicit
            // `AppHandle::exit()` calls (code: Some(_)) — the tray's "終了"
            // and `shell::elevate_restart` both rely on the latter actually
            // exiting, so only the code-less (user-window-closed) case gets
            // vetoed here.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
