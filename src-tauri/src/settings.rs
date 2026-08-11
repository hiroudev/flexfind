//! Settings persistence (JSON), mirroring windows-grep's
//! `src-tauri/src/store.rs` pattern: a serde struct written to
//! `settings.json` under the app's config dir.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::Manager;

/// One indexing root: a drive (`"C:"`), a local folder (`"D:\\Data"`), or a
/// UNC file-server path (`"\\\\nas\\share"`).
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct ScanRoot {
    pub path: String,
    pub enabled: bool,
    /// Auto-detected drive (reconciled against live drives on load; not
    /// user-removable in the UI). Custom folder/UNC roots have `false`.
    pub is_drive: bool,
}

impl Default for ScanRoot {
    fn default() -> Self {
        ScanRoot { path: String::new(), enabled: true, is_drive: false }
    }
}

/// A named, saved search scope: a search restricted to entries under one of
/// `include_paths`. Distinct from scan roots (what gets indexed) — a scope
/// filters *within* the already-built index. The implicit "全体" scope
/// (no restriction) is not stored; the frontend represents it as a null
/// scope id.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchScope {
    pub id: String,
    pub name: String,
    pub include_paths: Vec<String>,
}

impl Default for SearchScope {
    fn default() -> Self {
        SearchScope { id: String::new(), name: String::new(), include_paths: Vec::new() }
    }
}

/// One result-table column's persisted layout. `id` is one of
/// "name"/"path"/"size"/"modified"; order in the `Vec` is the column order.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct ColumnConfig {
    pub id: String,
    pub width: u32,
}

impl Default for ColumnConfig {
    fn default() -> Self {
        ColumnConfig { id: String::new(), width: 160 }
    }
}

pub fn default_columns() -> Vec<ColumnConfig> {
    vec![
        ColumnConfig { id: "name".into(), width: 240 },
        ColumnConfig { id: "path".into(), width: 320 },
        ColumnConfig { id: "size".into(), width: 84 },
        ColumnConfig { id: "modified".into(), width: 132 },
    ]
}

/// Fill in any missing default columns and drop unknown ids, preserving the
/// user's saved order/width for the ones that are valid — so a config that
/// predates a column (or has junk) still yields all four in a sane state.
fn normalize_columns(cols: Vec<ColumnConfig>) -> Vec<ColumnConfig> {
    let valid = ["name", "path", "size", "modified"];
    let mut out: Vec<ColumnConfig> = cols
        .into_iter()
        .filter(|c| valid.contains(&c.id.as_str()))
        .collect();
    for def in default_columns() {
        if !out.iter().any(|c| c.id == def.id) {
            out.push(def);
        }
    }
    out
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: String,
    /// Legacy v1 field (drive letters enabled for indexing). Read-only, used
    /// only to migrate into `scan_roots`; `skip_serializing` drops it on the
    /// next save so it disappears once migrated.
    #[serde(default, skip_serializing)]
    pub enabled_drives: Vec<String>,
    /// Indexing roots (drives + custom folders/UNC paths).
    pub scan_roots: Vec<ScanRoot>,
    /// Saved named search scopes.
    pub scopes: Vec<SearchScope>,
    /// Absolute path prefixes (e.g. "C:\\Windows") or bare folder-name
    /// patterns (e.g. "node_modules"), both case-insensitive — see
    /// `index::walker::is_excluded`.
    pub exclude_paths: Vec<String>,
    /// Everything-style result filters, checked per entry during the walk
    /// (see `index::walker::FilterOpts`) — distinct from `exclude_paths`,
    /// which prunes by location rather than by kind.
    pub exclude_hidden: bool,
    pub exclude_system: bool,
    /// Hides `.lnk` shortcut files from results.
    pub exclude_shortcuts: bool,
    /// Global-hotkey accelerator string, e.g. "Ctrl+Space".
    pub hotkey: String,
    pub launch_at_login: bool,
    pub start_minimized_to_tray: bool,
    pub auto_check_updates: bool,
    /// Background index-refresh schedule: `"interval"` (every
    /// `reindex_interval_minutes`), `"daily"` (once a day at
    /// `reindex_daily_time`), or `"manual"` (never auto-refresh). Read live by
    /// the periodic-reindex tick in `lib.rs`, so changing it takes effect
    /// without a restart.
    pub reindex_mode: String,
    /// Minutes between refreshes when `reindex_mode == "interval"`.
    pub reindex_interval_minutes: u32,
    /// Local wall-clock time ("HH:MM", 24h) for the daily refresh when
    /// `reindex_mode == "daily"`.
    pub reindex_daily_time: String,
    /// Result-table column order + widths (persisted so a user's layout
    /// survives restarts). Empty in old settings → filled with defaults.
    pub columns: Vec<ColumnConfig>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "flex-light".into(),
            enabled_drives: Vec::new(),
            scan_roots: Vec::new(), // filled from detected drives on first load, see reconcile_roots
            scopes: Vec::new(),
            exclude_paths: default_exclude_paths(),
            exclude_hidden: false,
            exclude_system: false,
            // On by default: shortcuts are rarely what someone's searching
            // for by name/content, and their presence alongside the real
            // target is the exact complaint this option exists to fix.
            exclude_shortcuts: true,
            hotkey: "Ctrl+Space".into(),
            launch_at_login: true,
            start_minimized_to_tray: true,
            auto_check_updates: false,
            // Preserves the historical behavior (a 30-minute interval walk)
            // for anyone whose settings.json predates these fields.
            reindex_mode: "interval".into(),
            reindex_interval_minutes: 30,
            reindex_daily_time: "03:00".into(),
            columns: default_columns(),
        }
    }
}

fn default_exclude_paths() -> Vec<String> {
    vec![
        "node_modules".to_string(),
        ".git".to_string(),
        "$recycle.bin".to_string(),
        "system volume information".to_string(),
    ]
}

fn config_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Currently mounted fixed drives as (letter, volume label) pairs — same
/// enumeration as FlexExplorer's `fs::list_drives`.
pub fn detect_drives() -> Vec<(String, String)> {
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();
    let mut out: Vec<(String, String)> = disks
        .list()
        .iter()
        .map(|d| {
            let mount = d.mount_point().to_string_lossy().to_string();
            let letter = mount.trim_end_matches(['\\', '/']).to_string();
            let label = d.name().to_string_lossy().to_string();
            (letter, label)
        })
        .filter(|(letter, _)| !letter.is_empty())
        .collect();
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));
    out
}

/// Reconcile `scan_roots` against the live set of `detected` drives, and
/// migrate the legacy v1 `enabled_drives` field. Pure so it's unit-testable
/// with an injected drive list.
///
/// - Empty `scan_roots` + non-empty `enabled_drives` (a v1 settings.json):
///   synthesize one drive `ScanRoot` per detected drive, enabled iff the
///   legacy list contained it (case-insensitive).
/// - Empty `scan_roots` + empty `enabled_drives` (fresh install): all
///   detected drives enabled (v1 parity).
/// - Non-empty `scan_roots`: add newly-detected drives (enabled); keep
///   existing entries even for drives that are momentarily absent (a USB
///   drive may return — the engine shows absent roots as offline/skipped
///   rather than dropping them). Custom (non-drive) roots pass through
///   untouched.
pub fn reconcile_roots(mut s: Settings, detected: &[(String, String)]) -> Settings {
    if s.scan_roots.is_empty() {
        let legacy = std::mem::take(&mut s.enabled_drives);
        let enable_all = legacy.is_empty();
        s.scan_roots = detected
            .iter()
            .map(|(letter, _)| ScanRoot {
                path: letter.clone(),
                enabled: enable_all || legacy.iter().any(|d| d.eq_ignore_ascii_case(letter)),
                is_drive: true,
            })
            .collect();
    } else {
        for (letter, _) in detected {
            let known = s
                .scan_roots
                .iter()
                .any(|r| r.path.eq_ignore_ascii_case(letter));
            if !known {
                s.scan_roots.push(ScanRoot {
                    path: letter.clone(),
                    enabled: true,
                    is_drive: true,
                });
            }
        }
        s.enabled_drives.clear();
    }
    s
}

pub fn load_settings(app: &tauri::AppHandle) -> Settings {
    let path = config_dir(app).join("settings.json");
    let mut s = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
        .unwrap_or_default();
    s.columns = normalize_columns(std::mem::take(&mut s.columns));
    reconcile_roots(s, &detect_drives())
}

pub fn save_settings(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
    let dir = config_dir(app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(app: tauri::AppHandle) -> Settings {
    load_settings(&app)
}

#[tauri::command]
pub fn set_settings(app: tauri::AppHandle, settings: Settings) -> Result<(), String> {
    save_settings(&app, &settings)
}

/// Persist just the column layout via read-modify-write, so the main window
/// (which edits columns) can't clobber scopes/roots the settings window may
/// have edited from its own stale settings snapshot.
#[tauri::command]
pub fn set_columns(app: tauri::AppHandle, columns: Vec<ColumnConfig>) -> Result<(), String> {
    let mut s = load_settings(&app);
    s.columns = normalize_columns(columns);
    save_settings(&app, &s)
}

/// `async` so this dispatches to the async runtime's thread pool rather
/// than running on the main/event-loop thread — `blocking_pick_folder`'s
/// own docs warn against calling it there (it would freeze window
/// show/hide/position handling and the tray for as long as the dialog is
/// open).
#[tauri::command]
pub async fn pick_folder(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog().file().blocking_pick_folder().map(|p| p.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drives() -> Vec<(String, String)> {
        vec![
            ("C:".into(), "Windows".into()),
            ("D:".into(), "Data".into()),
        ]
    }

    #[test]
    fn migrates_legacy_enabled_drives() {
        let s = Settings {
            enabled_drives: vec!["C:".into()], // v1: only C enabled
            ..Default::default()
        };
        let out = reconcile_roots(s, &drives());
        assert_eq!(out.scan_roots.len(), 2);
        let c = out.scan_roots.iter().find(|r| r.path == "C:").unwrap();
        let d = out.scan_roots.iter().find(|r| r.path == "D:").unwrap();
        assert!(c.enabled && c.is_drive);
        assert!(!d.enabled); // was not in the legacy enabled list
        assert!(out.enabled_drives.is_empty()); // legacy field cleared
    }

    #[test]
    fn fresh_install_enables_all_drives() {
        let out = reconcile_roots(Settings::default(), &drives());
        assert_eq!(out.scan_roots.len(), 2);
        assert!(out.scan_roots.iter().all(|r| r.enabled && r.is_drive));
    }

    #[test]
    fn adds_new_drive_and_keeps_custom_root() {
        let s = Settings {
            scan_roots: vec![
                ScanRoot { path: "C:".into(), enabled: true, is_drive: true },
                ScanRoot { path: "\\\\nas\\share".into(), enabled: true, is_drive: false },
            ],
            ..Default::default()
        };
        // D: is newly present; C: already known; the UNC root passes through.
        let out = reconcile_roots(s, &drives());
        assert!(out.scan_roots.iter().any(|r| r.path == "D:" && r.enabled && r.is_drive));
        assert!(out.scan_roots.iter().any(|r| r.path == "\\\\nas\\share" && !r.is_drive));
        assert_eq!(out.scan_roots.iter().filter(|r| r.path == "C:").count(), 1);
    }
}
