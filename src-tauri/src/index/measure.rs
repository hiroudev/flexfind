//! One-off, opt-in measurement of the index layout change.
//!
//! This is not a unit test — it is the evidence for *why* `IndexArena`
//! exists. It rebuilds both the current packed layout and the previous
//! struct-per-entry layout from the same real path list, then reports what
//! each costs and how long an identical search takes on each, including
//! after the process working set has been released (the cold case that
//! actually bit users, since FlexFind sits in the tray and Windows reclaims
//! idle working sets).
//!
//! Ignored by default and driven by an environment variable so it stays out
//! of `cargo test` and carries no machine-specific paths:
//!
//! ```text
//! FLEXFIND_BENCH_IDX=<path to a v1 .idx> \
//!   cargo test --release measure -- --ignored --nocapture
//! ```
//!
//! A v1 `.idx` is used as the input because it stores plain full paths, so
//! it is a convenient corpus of a real machine's filenames. Any file in that
//! format works.

use serde::Deserialize;

use super::query::{parse_query, CompiledQuery};
use super::types::ArenaBuilder;

/// The v1 on-disk record, kept here only so this measurement can read an
/// old index file as a corpus. Nothing in the app writes this any more.
#[derive(Deserialize)]
struct PersistedEntryV1 {
    path: String,
    folder: bool,
    size: u64,
    modified: i64,
}

/// The pre-arena in-memory layout, reproduced verbatim so the comparison
/// reflects its real allocation pattern: five separately allocated strings
/// per entry, reached through a pointer each.
struct OldEntry {
    #[allow(dead_code)]
    name: String,
    name_lower: String,
    #[allow(dead_code)]
    path: String,
    #[allow(dead_code)]
    dir: String,
    #[allow(dead_code)]
    ext: String,
    #[allow(dead_code)]
    size: u64,
    #[allow(dead_code)]
    modified: i64,
}

fn old_entry_from_path(path: String, folder: bool, size: u64, modified: i64) -> OldEntry {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string();
    let name_lower = name.to_lowercase();
    let dir = match path.rfind(['\\', '/']) {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    };
    let ext = if folder {
        String::new()
    } else {
        match name.rfind('.') {
            Some(i) if i > 0 => name[i + 1..].to_lowercase(),
            _ => String::new(),
        }
    };
    OldEntry { name, name_lower, path, dir, ext, size, modified }
}

/// Heap bytes actually held by the old layout: the contiguous struct array
/// plus every string's own allocation. This excludes per-allocation
/// allocator overhead (header plus size-class rounding), which the old
/// layout pays ~5 times per entry and the arena pays a dozen times in
/// total — so the comparison below understates the gap rather than
/// inflating it.
fn old_layout_bytes(entries: &[OldEntry]) -> u64 {
    let struct_bytes = (entries.len() * std::mem::size_of::<OldEntry>()) as u64;
    let string_bytes: u64 = entries
        .iter()
        .map(|e| {
            (e.name.capacity()
                + e.name_lower.capacity()
                + e.path.capacity()
                + e.dir.capacity()
                + e.ext.capacity()) as u64
        })
        .sum();
    struct_bytes + string_bytes
}

fn mb(bytes: u64) -> String {
    format!("{:>8.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

fn trim_working_set() {
    #[cfg(windows)]
    unsafe {
        use ::windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

#[test]
#[ignore = "opt-in measurement; needs FLEXFIND_BENCH_IDX"]
fn compare_index_layouts() {
    let Ok(idx_path) = std::env::var("FLEXFIND_BENCH_IDX") else {
        eprintln!("set FLEXFIND_BENCH_IDX to a v1 .idx file");
        return;
    };

    let raw = std::fs::read(&idx_path).expect("read idx");
    assert_eq!(&raw[..6], b"FFIDX1", "expected a v1 index file");
    let (_version, _root, slim): (u32, String, Vec<PersistedEntryV1>) =
        bincode::deserialize(&raw[6..]).expect("parse v1 idx");
    drop(raw);

    println!("\ncorpus: {} entries from {}", slim.len(), idx_path);

    // ---- build both layouts from the identical input ----
    let t = std::time::Instant::now();
    let mut builder = ArenaBuilder::with_capacity(slim.len());
    for e in &slim {
        builder.push(&e.path, e.folder, e.size, e.modified);
    }
    let arena = builder.finish();
    let arena_build = t.elapsed();

    let t = std::time::Instant::now();
    let old: Vec<OldEntry> = slim
        .iter()
        .map(|e| old_entry_from_path(e.path.clone(), e.folder, e.size, e.modified))
        .collect();
    let old_build = t.elapsed();

    let m = arena.memory();
    let old_bytes = old_layout_bytes(&old);

    println!("\n--- memory ---");
    println!("old  total          {}", mb(old_bytes));
    println!("new  total          {}   ({:.1}x smaller)", mb(m.total_bytes), old_bytes as f64 / m.total_bytes as f64);
    println!("     names_lower    {}   <- all a bare-term search reads", mb(m.names_lower_bytes));
    println!("     names          {}", mb(m.names_bytes));
    println!("     dirs (x2)      {}   ({} distinct)", mb(m.dir_bytes), m.dirs);
    println!("     exts           {}   ({} distinct)", mb(m.ext_bytes), m.exts);
    println!("     columns        {}", mb(m.column_bytes));
    println!(
        "\nbytes touched by one bare-term search: old {} -> new {}   ({:.1}x less)",
        mb(old_bytes),
        mb(m.scanned_bytes),
        old_bytes as f64 / m.scanned_bytes as f64
    );
    println!("\nbuild time: old {:?}, new {:?}", old_build, arena_build);

    // ---- search timing ----
    // A term deliberately chosen to be common enough to exercise the whole
    // scan rather than exiting early on a rare first byte.
    let queries = ["report", "config", "a"];

    for q in queries {
        let parsed = parse_query(q);

        // Cold: release the working set first, so both layouts have to be
        // faulted back in exactly as they would after the app sat idle.
        trim_working_set();
        let t = std::time::Instant::now();
        let mut old_hits = 0u64;
        for e in &old {
            if e.name_lower.contains(q) {
                old_hits += 1;
            }
        }
        let old_cold = t.elapsed();

        trim_working_set();
        let compiled = CompiledQuery::compile(&parsed, &arena, chrono::Local::now());
        let t = std::time::Instant::now();
        let mut new_hits = 0u64;
        for i in 0..arena.len() {
            if compiled.matches(&arena, i).is_some() {
                new_hits += 1;
            }
        }
        let new_cold = t.elapsed();

        // Warm: immediately again, everything resident.
        let t = std::time::Instant::now();
        let mut n = 0u64;
        for e in &old {
            if e.name_lower.contains(q) {
                n += 1;
            }
        }
        let old_warm = t.elapsed();
        std::hint::black_box(n);

        let t = std::time::Instant::now();
        let mut n = 0u64;
        for i in 0..arena.len() {
            if compiled.matches(&arena, i).is_some() {
                n += 1;
            }
        }
        let new_warm = t.elapsed();
        std::hint::black_box(n);

        assert_eq!(old_hits, new_hits, "both layouts must match the same entries for {q:?}");
        println!(
            "\nquery {:>8?}  {} hits\n  cold: old {:>10?}  new {:>10?}\n  warm: old {:>10?}  new {:>10?}",
            q, new_hits, old_cold, new_cold, old_warm, new_warm
        );
    }
}
