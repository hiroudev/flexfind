//! Shared path-matching helper, used by the walker (exclude filtering), the
//! engine (nested-root dedupe, scope include filtering) and the query
//! compiler.
//!
//! Field derivation (name / parent directory / extension) used to live here
//! too, so the walker and the persisted-index loader couldn't drift apart.
//! It now lives in `types::ArenaBuilder::push`, which is the single place
//! entries enter the index at all: the persisted format stores the arena
//! itself, so loading no longer re-derives anything.

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
}
