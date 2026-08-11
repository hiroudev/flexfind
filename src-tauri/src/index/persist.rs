//! On-disk persistence of the per-root index, so startup can serve results
//! immediately (and offline file-server roots keep serving their last-known
//! index) instead of blocking on a fresh full walk every launch.
//!
//! One file per root under `<app_data_dir>/index/<sanitized>_<fnv8>.idx`.
//! Per-root (rather than one big file) aligns with the engine's per-root
//! swap-at-end rebuild and lets an offline root's file survive untouched
//! while other roots refresh.
//!
//! Format: 6 magic bytes, then `bincode` of `(version: u32, root: String,
//! Vec<PersistedEntry>)`. A slim `PersistedEntry` (path/folder/size/modified
//! only) roughly halves file size vs. persisting the full `IndexedEntry` —
//! `name`/`name_lower`/`dir`/`ext` are recomputed from `path` on load via
//! the shared `pathmatch::entry_from_path`, so the derivation can't drift
//! from the walker's.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::pathmatch::entry_from_path;
use super::types::IndexedEntry;

const INDEX_MAGIC: &[u8; 6] = b"FFIDX1";
const INDEX_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct PersistedEntry {
    path: String,
    folder: bool,
    size: u64,
    modified: i64,
}

/// FNV-1a (32-bit) over the normalized root — a stable, dependency-free
/// hash. `std::hash::DefaultHasher`'s output isn't guaranteed stable across
/// Rust releases, so it can't be used for on-disk filenames.
fn fnv1a(s: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for b in s.bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn normalize_root(root: &str) -> String {
    root.trim_end_matches(['\\', '/']).to_lowercase()
}

fn index_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("index")
}

fn root_file(app_data_dir: &Path, root: &str) -> PathBuf {
    let norm = normalize_root(root);
    let sanitized: String = norm
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    index_dir(app_data_dir).join(format!("{sanitized}_{:08x}.idx", fnv1a(&norm)))
}

/// Serialize `entries` for `root` to its `.idx` file (atomic via temp +
/// rename). Call after a winning `Done` swap.
pub fn save_root(app_data_dir: &Path, root: &str, entries: &[IndexedEntry]) -> io::Result<()> {
    std::fs::create_dir_all(index_dir(app_data_dir))?;
    let slim: Vec<PersistedEntry> = entries
        .iter()
        .map(|e| PersistedEntry {
            path: e.path.clone(),
            folder: e.folder,
            size: e.size,
            modified: e.modified,
        })
        .collect();
    let payload = bincode::serialize(&(INDEX_VERSION, normalize_root(root), slim))
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let final_path = root_file(app_data_dir, root);
    let tmp_path = final_path.with_extension("idx.tmp");
    {
        let mut f = std::fs::File::create(&tmp_path)?;
        f.write_all(INDEX_MAGIC)?;
        f.write_all(&payload)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp_path, &final_path)
}

/// Load `root`'s persisted entries, or `None` if the file is missing,
/// corrupt, wrong magic/version, or for a different root (a hash collision
/// or a stale file). A file that fails to parse is deleted so it doesn't
/// keep failing every launch.
pub fn load_root(app_data_dir: &Path, root: &str) -> Option<Vec<IndexedEntry>> {
    let path = root_file(app_data_dir, root);
    let mut f = std::fs::File::open(&path).ok()?;
    let mut magic = [0u8; 6];
    if f.read_exact(&mut magic).is_err() || &magic != INDEX_MAGIC {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    let mut rest = Vec::new();
    if f.read_to_end(&mut rest).is_err() {
        return None;
    }
    match bincode::deserialize::<(u32, String, Vec<PersistedEntry>)>(&rest) {
        Ok((version, stored_root, slim))
            if version == INDEX_VERSION && stored_root == normalize_root(root) =>
        {
            Some(
                slim.into_iter()
                    .map(|p| entry_from_path(p.path, p.folder, p.size, p.modified))
                    .collect(),
            )
        }
        _ => {
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

/// Best-effort removal of `.idx` files for roots no longer configured.
pub fn cleanup(app_data_dir: &Path, known_roots: &[String]) {
    let known: Vec<PathBuf> = known_roots
        .iter()
        .map(|r| root_file(app_data_dir, r))
        .collect();
    let Ok(rd) = std::fs::read_dir(index_dir(app_data_dir)) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("idx") && !known.contains(&p) {
            let _ = std::fs::remove_file(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("flexfind_persist_test_{}", fnv1a(&format!("{:?}", std::time::SystemTime::now()))));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn round_trips_entries() {
        let dir = tmp_dir();
        let entries = vec![
            entry_from_path("C:\\Projects\\a.txt".into(), false, 10, 100),
            entry_from_path("C:\\Projects\\sub".into(), true, 0, 200),
        ];
        save_root(&dir, "C:", &entries).unwrap();
        let loaded = load_root(&dir, "C:").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "a.txt");
        assert_eq!(loaded[0].ext, "txt");
        assert_eq!(loaded[0].size, 10);
        assert!(loaded[1].folder);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_bad_magic() {
        let dir = tmp_dir();
        let path = root_file(&dir, "C:");
        std::fs::create_dir_all(index_dir(&dir)).unwrap();
        std::fs::write(&path, b"XXXXXXgarbage").unwrap();
        assert!(load_root(&dir, "C:").is_none());
        assert!(!path.exists()); // deleted on bad magic
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_is_none() {
        let dir = tmp_dir();
        assert!(load_root(&dir, "Z:").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
