//! Best-effort discovery of the FlexExplorer/FlexGrep sibling apps, for the
//! result-row context menu's "FlexExplorerで表示" / "FlexGrepで検索" items.
//! Checked once per overlay session on the frontend (install state doesn't
//! change mid-session), not per right-click.
//!
//! Intentionally non-exhaustive: checks this repo's own dev-build layout
//! first, then a couple of common installed-product locations. A full
//! Windows uninstall-registry enumeration (RegEnumKeyExW over every
//! `...\Uninstall\*` subkey, reading `DisplayName`) would be more thorough
//! but is meaningfully more Win32 surface area for a feature whose only
//! effect is hiding two context-menu items — not worth it here. The context
//! menu simply hides an item when its path can't be found, no error shown.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SiblingAvailability {
    pub flex_explorer: Option<String>,
    pub flex_grep: Option<String>,
}

struct SiblingSpec {
    /// This repo's sibling folder name under the common `desktopApp\` parent.
    dev_folder: &'static str,
    /// Cargo binary filename(s) to look for under that folder's
    /// `src-tauri/target/{debug,release}/`.
    dev_bin_names: &'static [&'static str],
    /// `<installRoot>\<...>` suffix to try under common install locations.
    install_subpath: &'static str,
}

const FLEX_EXPLORER: SiblingSpec = SiblingSpec {
    dev_folder: "FlexExplorer",
    dev_bin_names: &["flex-explorer.exe"],
    install_subpath: "FlexExplorer\\FlexExplorer.exe",
};

const FLEX_GREP: SiblingSpec = SiblingSpec {
    dev_folder: "windows-grep",
    dev_bin_names: &["flexgrep.exe", "grepforge.exe"],
    install_subpath: "FlexGrep\\FlexGrep.exe",
};

#[tauri::command]
pub fn find_sibling_apps() -> SiblingAvailability {
    SiblingAvailability {
        flex_explorer: find_one(&FLEX_EXPLORER),
        flex_grep: find_one(&FLEX_GREP),
    }
}

fn find_one(spec: &SiblingSpec) -> Option<String> {
    find_dev_build(spec).or_else(|| find_via_common_install_dirs(spec))
}

/// This repo's actual on-disk layout: sibling app folders live alongside
/// this exe's own `desktopApp\FlexFind\` under a shared `desktopApp\`
/// parent. Only meaningful for local dev runs, not a real installed
/// product.
///
/// Deliberately checks the `release` profile only, never `debug` — a debug
/// Tauri build's webview loads its `devUrl` (e.g. `http://localhost:5175`)
/// rather than bundled assets, so launching a debug sibling exe standalone
/// (its own `vite dev` isn't running) shows a browser connection-error page
/// instead of the app. A release build is self-contained and always safe to
/// spawn. This does mean the context-menu items stay hidden until a sibling
/// has an actual release build on disk, which is the correct trade-off:
/// hiding an item beats sending the user to an error page.
fn find_dev_build(spec: &SiblingSpec) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let profile_dir = exe.parent()?; // .../FlexFind/src-tauri/target/{debug,release}
    let desktop_app_dir = profile_dir.parent()?.parent()?.parent()?.parent()?; // -> target -> src-tauri -> FlexFind -> desktopApp
    for bin in spec.dev_bin_names {
        let candidate = desktop_app_dir
            .join(spec.dev_folder)
            .join("src-tauri")
            .join("target")
            .join("release")
            .join(bin);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

fn find_via_common_install_dirs(spec: &SiblingSpec) -> Option<String> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(v) = std::env::var("LOCALAPPDATA") {
        roots.push(PathBuf::from(v).join("Programs"));
    }
    if let Ok(v) = std::env::var("ProgramFiles") {
        roots.push(PathBuf::from(v));
    }
    if let Ok(v) = std::env::var("ProgramFiles(x86)") {
        roots.push(PathBuf::from(v));
    }
    roots
        .into_iter()
        .map(|root| root.join(spec.install_subpath))
        .find(|candidate| candidate.is_file())
        .map(|p| p.to_string_lossy().to_string())
}
