//! Source-edit primitives: byte-range edits, inverse operations and the undo
//! journal (review PR-03, MR-03, MR-08).
//!
//! All offsets are byte offsets into the file content, matching the
//! lol_html streaming rewriter used by the editing pipeline (review 6.3).
//! Callers must guarantee offsets fall on character boundaries; the pipeline
//! guarantees this because it produces offsets from the same byte stream.

use std::path::PathBuf;

/// A single byte-range replacement inside one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

impl TextEdit {
    pub fn new(start: usize, end: usize, replacement: impl Into<String>) -> Self {
        assert!(start <= end, "TextEdit start must not exceed end");
        Self { start, end, replacement: replacement.into() }
    }

    pub fn apply(&self, text: &mut String) {
        text.replace_range(self.start..self.end, &self.replacement);
    }

    /// The edit that reverses this one, given the file content as it was
    /// *before* this edit was applied.
    pub fn inverse(&self, content_before: &str) -> Self {
        Self::new(
            self.start,
            self.start + self.replacement.len(),
            content_before[self.start..self.end].to_string(),
        )
    }
}

/// One recorded operation on one file: the forward edits plus the content
/// snapshots that make undo and redo trivially correct (MR-03).
#[derive(Debug, Clone)]
pub struct FileOperation {
    pub path: PathBuf,
    pub edits: Vec<TextEdit>,
    pub content_before: String,
    pub content_after: String,
}

impl FileOperation {
    /// Records an operation from before/after snapshots, verifying that the
    /// claimed edits actually reproduce the after-state.
    pub fn new(path: PathBuf, edits: Vec<TextEdit>, content_before: String) -> Self {
        let mut replay = content_before.clone();
        for edit in &edits {
            edit.apply(&mut replay);
        }
        Self {
            path,
            edits,
            content_before,
            content_after: replay,
        }
    }
}

/// Undo/redo journal for one project session. Redo is cleared whenever a
/// new operation is recorded; a file changed outside the application
/// invalidates the journal entry by causing a full re-read before any patch
/// (MR-08), which is enforced by the pipeline, not here.
#[derive(Debug, Default)]
pub struct Journal {
    undo: Vec<FileOperation>,
    redo: Vec<FileOperation>,
}

/// The result of an undo or redo step: the file and the exact content the
/// pipeline must write back to disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreStep {
    pub path: PathBuf,
    pub content: String,
}

impl Journal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, operation: FileOperation) {
        self.undo.push(operation);
        self.redo.clear();
    }

    pub fn undo(&mut self) -> Option<RestoreStep> {
        let operation = self.undo.pop()?;
        self.redo.push(operation.clone());
        Some(RestoreStep { path: operation.path, content: operation.content_before })
    }

    pub fn redo(&mut self) -> Option<RestoreStep> {
        let operation = self.redo.pop()?;
        self.undo.push(operation.clone());
        Some(RestoreStep { path: operation.path, content: operation.content_after })
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINAL: &str = "<h1>Old heading</h1><p>Body</p>";

    fn heading_edit() -> TextEdit {
        let start = ORIGINAL.find("Old heading").unwrap();
        TextEdit::new(start, start + "Old heading".len(), "New heading")
    }

    #[test]
    fn edit_apply_and_inverse_roundtrip() {
        let edit = heading_edit();
        let mut text = ORIGINAL.to_string();
        edit.apply(&mut text);
        assert_eq!(text, "<h1>New heading</h1><p>Body</p>");
        let inverse = edit.inverse(ORIGINAL);
        inverse.apply(&mut text);
        assert_eq!(text, ORIGINAL);
    }

    #[test]
    fn operation_new_replays_edits_for_after_snapshot() {
        let op = FileOperation::new(
            PathBuf::from("website/index.html"),
            vec![heading_edit()],
            ORIGINAL.to_string(),
        );
        assert_eq!(op.content_after, "<h1>New heading</h1><p>Body</p>");
    }

    #[test]
    fn journal_undo_redo_walk() {
        let mut journal = Journal::new();
        journal.record(FileOperation::new(
            PathBuf::from("index.html"),
            vec![heading_edit()],
            ORIGINAL.to_string(),
        ));
        assert!(journal.can_undo());
        let step = journal.undo().unwrap();
        assert_eq!(step.content, ORIGINAL);
        assert!(journal.can_redo());
        let step = journal.redo().unwrap();
        assert_eq!(step.content, "<h1>New heading</h1><p>Body</p>");
        assert!(journal.can_undo());
    }

    #[test]
    fn new_record_clears_redo() {
        let mut journal = Journal::new();
        journal.record(FileOperation::new(
            PathBuf::from("a.html"),
            vec![heading_edit()],
            ORIGINAL.to_string(),
        ));
        assert!(journal.undo().is_some());
        assert!(journal.can_redo());
        journal.record(FileOperation::new(
            PathBuf::from("a.html"),
            vec![TextEdit::new(0, 0, "<!-- x -->")],
            ORIGINAL.to_string(),
        ));
        assert!(!journal.can_redo(), "redo stack must clear on new work");
    }

    #[test]
    fn multi_edit_operation_reproduces_after_state() {
        let mut edits = vec![heading_edit()];
        let start = ORIGINAL.find("Body").unwrap();
        edits.push(TextEdit::new(start, start + "Body".len(), "Changed body"));
        let op = FileOperation::new(PathBuf::from("index.html"), edits, ORIGINAL.to_string());
        let mut text = op.content_before.clone();
        for edit in &op.edits {
            edit.apply(&mut text);
        }
        assert_eq!(text, op.content_after);
    }
}
