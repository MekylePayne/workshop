//! Unified diffs for the confirmation step (PR-03): every text change is
//! shown before it is applied, and the change journal displays what an undo
//! or redo will write back.

use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSummary {
    pub added: usize,
    pub removed: usize,
}

/// Counts added and removed lines between two versions.
pub fn summarize(before: &str, after: &str) -> DiffSummary {
    let diff = TextDiff::from_lines(before, after);
    let mut summary = DiffSummary { added: 0, removed: 0 };
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => summary.removed += 1,
            ChangeTag::Insert => summary.added += 1,
            ChangeTag::Equal => {}
        }
    }
    summary
}

/// Renders a unified diff with `context_radius` lines of context.
pub fn unified_diff(before: &str, after: &str, context_radius: usize) -> String {
    let diff = TextDiff::from_lines(before, after);
    diff.unified_diff()
        .context_radius(context_radius)
        .header("before", "after")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_counts_changes() {
        let summary = summarize("<h1>Old</h1>\n<p>x</p>\n", "<h1>New</h1>\n<p>x</p>\n");
        assert_eq!(summary.added, 1);
        assert_eq!(summary.removed, 1);
    }

    #[test]
    fn identical_content_has_empty_summary_and_diff() {
        let summary = summarize("a\nb\n", "a\nb\n");
        assert_eq!(summary, DiffSummary { added: 0, removed: 0 });
        assert!(unified_diff("a\nb\n", "a\nb\n", 3).trim().is_empty());
    }

    #[test]
    fn diff_shows_minus_and_plus_lines() {
        let diff = unified_diff("<h1>Old</h1>\n", "<h1>New</h1>\n", 1);
        assert!(diff.contains("-<h1>Old</h1>"));
        assert!(diff.contains("+<h1>New</h1>"));
    }
}
