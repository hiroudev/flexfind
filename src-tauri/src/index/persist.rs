//! On-disk persistence of the per-root index, so startup can serve results
//! immediately (and offline file-server roots keep serving their last-known
//! index) instead of blocking on a fresh full walk every launch.
//!
//! One file per root under `<app_data_dir>/index/<sanitized>_<fnv8>.idx`.
//! Per-root (rather than one big file) aligns with the engine's per-root
//! swap-at-end rebuild and lets an offline root's file survive untouched
//! while other roots refresh.
//!
//! Format: 6 magic bytes, then bincode of `(version: u32, root: String,
//! IndexArena)`.
//!
//! The v1 format stored a slim per-entry record and rebuilt name/dir/ext
//! from each path on load — which meant ~5 string allocations per entry,
//! several million of them, on the startup path. v2 stores the arena
//! itself, so loading is essentially a handful of buffer reads and the
//! index is usable the moment it lands. The magic bumped from `FFIDX1` to
//! `FFIDX2`, so a v1 file simply fails the magic check and is deleted and
//! re-walked (the same path any corrupt file already took).

use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use bincode::Options;

use super::types::IndexArena;

const INDEX_MAGIC: &[u8; 6] = b"FFIDX2";
const INDEX_VERSION: u32 = 2;

/// Upper bound handed to bincode so a corrupt length prefix can't make the
/// deserializer try to allocate an absurd buffer before any of our own
/// validation gets to run.
const MAX_INDEX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Both directions must use the same configuration, so they share one
/// constructor. Fixint encoding keeps the `Vec<u32>` offset columns a flat
/// memcpy rather than a per-element varint decode.
fn codec() -> impl bincode::Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(MAX_INDEX_BYTES)
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

/// Serialize `arena` for `root` to its `.idx` file (atomic via temp +
/// rename). Call after a winning `Done` swap.
pub fn save_root(app_data_dir: &Path, root: &str, arena: &IndexArena) -> io::Result<()> {
    std::fs::create_dir_all(index_dir(app_data_dir))?;

    let final_path = root_file(app_data_dir, root);
    let tmp_path = final_path.with_extension("idx.tmp");
    {
        let f = std::fs::File::create(&tmp_path)?;
        let mut w = BufWriter::new(f);
        w.write_all(INDEX_MAGIC)?;
        // Streamed rather than `bincode::serialize` into a Vec first, which
        // would transiently double the index's memory footprint — the thing
        // this whole layout exists to keep small.
        codec()
            .serialize_into(&mut w, &(INDEX_VERSION, normalize_root(root), arena))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        w.flush()?;
        w.into_inner()?.sync_all()?;
    }
    std::fs::rename(&tmp_path, &final_path)
}

/// Load `root`'s persisted arena, or `None` if the file is missing, corrupt,
/// wrong magic/version, structurally inconsistent, or for a different root
/// (a hash collision or a stale file). A file that fails to parse is deleted
/// so it doesn't keep failing every launch — which is also how a v1 file
/// gets migrated: it fails the magic check and is re-walked.
pub fn load_root(app_data_dir: &Path, root: &str) -> Option<IndexArena> {
    let path = root_file(app_data_dir, root);
    let f = std::fs::File::open(&path).ok()?;
    let mut r = BufReader::new(f);

    let mut magic = [0u8; 6];
    if r.read_exact(&mut magic).is_err() || &magic != INDEX_MAGIC {
        let _ = std::fs::remove_file(&path);
        return None;
    }

    match codec().deserialize_from::<_, (u32, String, IndexArena)>(&mut r) {
        Ok((version, stored_root, arena))
            if version == INDEX_VERSION
                && stored_root == normalize_root(root)
                && arena.is_valid() =>
        {
            Some(arena)
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
    use crate::index::types::ArenaBuilder;

    fn tmp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "flexfind_persist_test_{}",
            fnv1a(&format!("{:?}", std::time::SystemTime::now()))
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample() -> IndexArena {
        let mut b = ArenaBuilder::new();
        b.push("C:\\Projects\\a.txt", false, 10, 100);
        b.push("C:\\Projects\\sub", true, 0, 200);
        b.finish()
    }

    #[test]
    fn round_trips_the_arena() {
        let dir = tmp_dir();
        save_root(&dir, "C:", &sample()).unwrap();
        let loaded = load_root(&dir, "C:").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.name(0), "a.txt");
        assert_eq!(loaded.ext(0), "txt");
        assert_eq!(loaded.size(0), 10);
        assert_eq!(loaded.dir(0), "C:\\Projects");
        assert_eq!(loaded.full_path(0), "C:\\Projects\\a.txt");
        assert!(loaded.folder(1));
        assert!(loaded.is_valid());
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

    /// A v1 file must not be mistaken for a v2 one — it fails the magic
    /// check, gets removed, and the root is re-walked from scratch.
    #[test]
    fn rejects_and_removes_a_v1_file() {
        let dir = tmp_dir();
        let path = root_file(&dir, "C:");
        std::fs::create_dir_all(index_dir(&dir)).unwrap();
        std::fs::write(&path, b"FFIDX1\x01\x00\x00\x00whatever").unwrap();
        assert!(load_root(&dir, "C:").is_none());
        assert!(!path.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_a_truncated_payload() {
        let dir = tmp_dir();
        save_root(&dir, "C:", &sample()).unwrap();
        let path = root_file(&dir, "C:");
        let full = std::fs::read(&path).unwrap();
        std::fs::write(&path, &full[..full.len() / 2]).unwrap();
        assert!(load_root(&dir, "C:").is_none());
        assert!(!path.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_is_none() {
        let dir = tmp_dir();
        assert!(load_root(&dir, "Z:").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
