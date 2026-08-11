//! Shared path-matching + entry-field-derivation helpers, used by the
//! walker (exclude filtering), the engine (per-root retain, scope include
//! filtering, nested-root dedupe) and the persistence loader (rebuilding
//! `IndexedEntry` fields from a bare path). Keeping these in one place stops
//! the walker and the loader from deriving `name`/`dir`/`ext` in subtly
//! different ways.

use super::types::IndexedEntry;

/// True when `path_lower` is `prefix_lower` itself or lives underneath it,
/// with a separator boundary — so `c:\users\hiro` matches `c:\users\hiro`
/// and `c:\users\hiro\docs` but NOT `c:\users\hiroko`. Both arguments must
/// already be lowercased by the caller (hoisting the lowercasing out of hot
/// per-entry loops).
pub fn path_has_prefix(path_lower: &str, prefix_lower: &str) -> bool {
    path_lower
        .strip_prefix(prefix_lower)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['\\', '/']))
}

/// Derive the display/index fields of an `IndexedEntry` from a full path
/// plus the folder flag. Shared so the walker and the persisted-index
/// loader stay in lockstep. `size`/`modified` come from filesystem metadata
/// (walker) or the persisted record (loader), so they're passed in.
pub fn entry_from_path(path: String, folder: bool, size: u64, modified: i64) -> IndexedEntry {
    let name = path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(&path)
        .to_string();
    let name_lower = name.to_lowercase();
    let dir = match path.rfind(['\\', '/']) {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    };
    let ext = if folder {
        String::new()
    } else {
        match name.rfind('.') {
            // A leading dot (dotfile) isn't an extension.
            Some(i) if i > 0 => name[i + 1..].to_lowercase(),
            _ => String::new(),
        }
    };
    IndexedEntry {
        name,
        name_lower,
        path,
        dir,
        folder,
        ext,
        size,
        modified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_respects_separator_boundary() {
        assert!(path_has_prefix("c:\\users\\hiro", "c:\\users\\hiro"));
        assert!(path_has_prefix("c:\\users\\hiro\\docs", "c:\\users\\hiro"));
        assert!(!path_has_prefix("c:\\users\\hiroko", "c:\\users\\hiro"));
        assert!(!path_has_prefix("c:\\users\\hiroko\\docs", "c:\\users\\hiro"));
    }

    #[test]
    fn prefix_matches_unc_roots() {
        assert!(path_has_prefix("\\\\nas\\share\\a.txt", "\\\\nas\\share"));
        assert!(!path_has_prefix("\\\\nas\\share2\\a.txt", "\\\\nas\\share"));
    }

    #[test]
    fn derives_fields_from_path() {
        let e = entry_from_path("C:\\Projects\\Report.docx".into(), false, 42, 100);
        assert_eq!(e.name, "Report.docx");
        assert_eq!(e.name_lower, "report.docx");
        assert_eq!(e.dir, "C:\\Projects");
        assert_eq!(e.ext, "docx");
        assert_eq!(e.size, 42);
    }

    #[test]
    fn folder_has_no_ext_and_dotfile_has_no_ext() {
        let folder = entry_from_path("C:\\Projects\\node_modules".into(), true, 0, 0);
        assert_eq!(folder.ext, "");
        let dotfile = entry_from_path("C:\\Projects\\.gitignore".into(), false, 10, 0);
        assert_eq!(dotfile.ext, "");
    }
}
