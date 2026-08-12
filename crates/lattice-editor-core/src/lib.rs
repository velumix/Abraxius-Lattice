//! Native, document-local editor state for Lattice.
//!
//! This crate deliberately owns only the hot editing path.  Project identity,
//! Studio synchronization, and saves remain daemon responsibilities.  A
//! [`Document`] stores text in a rope and emits bounded viewport snapshots so
//! the Avalonia renderer never needs a full-document copy for a keystroke.

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use blake3::Hash;
use ropey::Rope;
use serde::Serialize;

pub type EditorDocumentId = u64;
pub type EditorRevision = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteOffset(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EditorSelection {
    pub anchor: usize,
    pub head: usize,
}

impl EditorSelection {
    #[must_use]
    pub const fn caret(offset: usize) -> Self {
        Self { anchor: offset, head: offset }
    }

    #[must_use]
    pub const fn start(self) -> usize {
        if self.anchor < self.head { self.anchor } else { self.head }
    }

    #[must_use]
    pub const fn end(self) -> usize {
        if self.anchor > self.head { self.anchor } else { self.head }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditOperation {
    pub start_byte: usize,
    pub end_byte: usize,
    pub inserted: String,
}

impl EditOperation {
    #[must_use]
    pub fn replace(start_byte: usize, end_byte: usize, inserted: impl Into<String>) -> Self {
        Self { start_byte, end_byte, inserted: inserted.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditTransaction {
    pub base_revision: EditorRevision,
    pub operations: Vec<EditOperation>,
    pub before_selection: EditorSelection,
    pub after_selection: EditorSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppliedEdit {
    start_byte: usize,
    deleted: String,
    inserted: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HistoryEntry {
    edits: Vec<AppliedEdit>,
    before_selection: EditorSelection,
    after_selection: EditorSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorError {
    DocumentExists(EditorDocumentId),
    DocumentNotFound(EditorDocumentId),
    ReadOnly,
    RevisionConflict { expected: EditorRevision, actual: EditorRevision },
    InvalidRange { start_byte: usize, end_byte: usize, length: usize },
    InvalidUtf8Boundary(usize),
    InvalidEdit(String),
    NoUndo,
    NoRedo,
}

impl Display for EditorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DocumentExists(id) => write!(formatter, "document {id} already exists"),
            Self::DocumentNotFound(id) => write!(formatter, "document {id} was not found"),
            Self::ReadOnly => formatter.write_str("document is read-only"),
            Self::RevisionConflict { expected, actual } => {
                write!(formatter, "revision conflict: expected {expected}, actual {actual}")
            }
            Self::InvalidRange { start_byte, end_byte, length } => {
                write!(formatter, "invalid edit range {start_byte}..{end_byte} for {length} bytes")
            }
            Self::InvalidUtf8Boundary(offset) => {
                write!(formatter, "offset {offset} is not a UTF-8 boundary")
            }
            Self::InvalidEdit(message) => formatter.write_str(message),
            Self::NoUndo => formatter.write_str("no undo transaction is available"),
            Self::NoRedo => formatter.write_str("no redo transaction is available"),
        }
    }
}

impl std::error::Error for EditorError {}

#[derive(Debug, Clone, Serialize)]
pub struct ViewportLine {
    pub line_index: usize,
    pub number: usize,
    pub start_byte: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditorViewportSnapshot {
    pub document_id: EditorDocumentId,
    pub revision: EditorRevision,
    pub content_hash: Option<String>,
    pub first_line: usize,
    pub last_line: usize,
    pub total_lines: usize,
    pub total_bytes: usize,
    pub selection: EditorSelection,
    pub modified: bool,
    pub read_only: bool,
    pub lines: Vec<ViewportLine>,
}

fn hash_rope(rope: &Rope) -> Hash {
    let mut hasher = blake3::Hasher::new();
    for chunk in rope.chunks() {
        hasher.update(chunk.as_bytes());
    }
    hasher.finalize()
}

#[derive(Debug, Clone)]
pub struct Document {
    id: EditorDocumentId,
    rope: Rope,
    revision: EditorRevision,
    clean_revision: EditorRevision,
    read_only: bool,
    selection: EditorSelection,
    cached_content_hash: Option<Hash>,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl Document {
    #[must_use]
    pub fn new(id: EditorDocumentId, source: &str, read_only: bool) -> Self {
        let rope = Rope::from_str(source);
        let cached_content_hash = Some(hash_rope(&rope));
        Self {
            id,
            rope,
            revision: 0,
            clean_revision: 0,
            read_only,
            selection: EditorSelection::caret(0),
            cached_content_hash,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    #[must_use]
    pub const fn id(&self) -> EditorDocumentId {
        self.id
    }

    #[must_use]
    pub const fn revision(&self) -> EditorRevision {
        self.revision
    }

    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.revision != self.clean_revision
    }

    #[must_use]
    pub const fn selection(&self) -> EditorSelection {
        self.selection
    }

    pub fn set_selection(&mut self, selection: EditorSelection) -> Result<(), EditorError> {
        if selection.anchor > self.rope.len_bytes() || selection.head > self.rope.len_bytes() {
            return Err(EditorError::InvalidRange {
                start_byte: selection.start(),
                end_byte: selection.end(),
                length: self.rope.len_bytes(),
            });
        }
        self.ensure_boundary(selection.anchor)?;
        self.ensure_boundary(selection.head)?;
        self.selection = selection;
        Ok(())
    }

    pub fn mark_clean(&mut self, revision: EditorRevision) -> Result<(), EditorError> {
        if revision != self.revision {
            return Err(EditorError::RevisionConflict {
                expected: self.revision,
                actual: revision,
            });
        }
        self.clean_revision = revision;
        Ok(())
    }

    pub fn apply_transaction(&mut self, transaction: EditTransaction) -> Result<(), EditorError> {
        if self.read_only {
            return Err(EditorError::ReadOnly);
        }
        if transaction.base_revision != self.revision {
            return Err(EditorError::RevisionConflict {
                expected: transaction.base_revision,
                actual: self.revision,
            });
        }

        let mut operations = transaction.operations;
        operations.sort_by_key(|operation| std::cmp::Reverse(operation.start_byte));
        let mut applied = Vec::with_capacity(operations.len());
        let mut next_lower_bound = self.rope.len_bytes();

        for operation in operations {
            if operation.start_byte > operation.end_byte {
                return Err(EditorError::InvalidRange {
                    start_byte: operation.start_byte,
                    end_byte: operation.end_byte,
                    length: self.rope.len_bytes(),
                });
            }
            if operation.end_byte > next_lower_bound {
                return Err(EditorError::InvalidRange {
                    start_byte: operation.start_byte,
                    end_byte: operation.end_byte,
                    length: self.rope.len_bytes(),
                });
            }
            self.ensure_boundary(operation.start_byte)?;
            self.ensure_boundary(operation.end_byte)?;
            let char_start = self
                .rope
                .try_byte_to_char(operation.start_byte)
                .map_err(|_| EditorError::InvalidUtf8Boundary(operation.start_byte))?;
            let char_end = self
                .rope
                .try_byte_to_char(operation.end_byte)
                .map_err(|_| EditorError::InvalidUtf8Boundary(operation.end_byte))?;
            let deleted = self.rope.slice(char_start..char_end).to_string();
            self.rope
                .try_remove(char_start..char_end)
                .map_err(|error| EditorError::InvalidEdit(error.to_string()))?;
            self.rope
                .try_insert(char_start, &operation.inserted)
                .map_err(|error| EditorError::InvalidEdit(error.to_string()))?;
            applied.push(AppliedEdit {
                start_byte: operation.start_byte,
                deleted,
                inserted: operation.inserted,
            });
            next_lower_bound = operation.start_byte;
        }

        if !applied.is_empty() {
            self.cached_content_hash = None;
            self.revision = self.revision.saturating_add(1);
            self.undo.push(HistoryEntry {
                edits: applied,
                before_selection: transaction.before_selection,
                after_selection: transaction.after_selection,
            });
            self.redo.clear();
            self.selection = transaction.after_selection;
        }
        Ok(())
    }

    pub fn insert_text(&mut self, text: &str) -> Result<(), EditorError> {
        let selection = self.selection;
        let start = selection.start();
        let end = selection.end();
        let inserted_length = text.len();
        self.apply_transaction(EditTransaction {
            base_revision: self.revision,
            operations: vec![EditOperation::replace(start, end, text)],
            before_selection: selection,
            after_selection: EditorSelection::caret(start + inserted_length),
        })
    }

    pub fn delete_backward(&mut self) -> Result<(), EditorError> {
        let selection = self.selection;
        let (start, end) = if selection.start() != selection.end() {
            (selection.start(), selection.end())
        } else if selection.head == 0 {
            return Ok(());
        } else {
            let head_char = self
                .rope
                .try_byte_to_char(selection.head)
                .map_err(|_| EditorError::InvalidUtf8Boundary(selection.head))?;
            (
                self.rope
                    .try_char_to_byte(head_char.saturating_sub(1))
                    .map_err(|_| EditorError::InvalidUtf8Boundary(selection.head))?,
                selection.head,
            )
        };
        self.apply_transaction(EditTransaction {
            base_revision: self.revision,
            operations: vec![EditOperation::replace(start, end, "")],
            before_selection: selection,
            after_selection: EditorSelection::caret(start),
        })
    }

    pub fn delete_forward(&mut self) -> Result<(), EditorError> {
        let selection = self.selection;
        let (start, end) = if selection.start() != selection.end() {
            (selection.start(), selection.end())
        } else if selection.head >= self.rope.len_bytes() {
            return Ok(());
        } else {
            let head_char = self
                .rope
                .try_byte_to_char(selection.head)
                .map_err(|_| EditorError::InvalidUtf8Boundary(selection.head))?;
            (
                selection.head,
                self.rope
                    .try_char_to_byte(head_char + 1)
                    .map_err(|_| EditorError::InvalidUtf8Boundary(selection.head))?,
            )
        };
        self.apply_transaction(EditTransaction {
            base_revision: self.revision,
            operations: vec![EditOperation::replace(start, end, "")],
            before_selection: selection,
            after_selection: EditorSelection::caret(start),
        })
    }

    pub fn move_caret(&mut self, direction: CaretMovement) -> Result<(), EditorError> {
        let selection = self.selection;
        let offset = match direction {
            CaretMovement::Left => {
                if selection.start() != selection.end() {
                    selection.start()
                } else if selection.head == 0 {
                    0
                } else {
                    let char_index = self
                        .rope
                        .try_byte_to_char(selection.head)
                        .map_err(|_| EditorError::InvalidUtf8Boundary(selection.head))?;
                    self.rope
                        .try_char_to_byte(char_index.saturating_sub(1))
                        .map_err(|_| EditorError::InvalidUtf8Boundary(selection.head))?
                }
            }
            CaretMovement::Right => {
                if selection.start() != selection.end() {
                    selection.end()
                } else if selection.head >= self.rope.len_bytes() {
                    self.rope.len_bytes()
                } else {
                    let char_index = self
                        .rope
                        .try_byte_to_char(selection.head)
                        .map_err(|_| EditorError::InvalidUtf8Boundary(selection.head))?;
                    self.rope
                        .try_char_to_byte(char_index + 1)
                        .map_err(|_| EditorError::InvalidUtf8Boundary(selection.head))?
                }
            }
            CaretMovement::Home | CaretMovement::End => {
                let char_index = self
                    .rope
                    .try_byte_to_char(selection.head)
                    .map_err(|_| EditorError::InvalidUtf8Boundary(selection.head))?;
                let line = self
                    .rope
                    .try_char_to_line(char_index)
                    .map_err(|_| EditorError::InvalidUtf8Boundary(selection.head))?;
                let line_start = self.rope.line_to_char(line);
                if direction == CaretMovement::Home {
                    self.rope.char_to_byte(line_start)
                } else {
                    let line_slice = self.rope.line(line);
                    let mut line_length = line_slice.len_chars();
                    let line_text = line_slice.to_string();
                    if line_text.ends_with('\n') {
                        line_length = line_length.saturating_sub(1);
                        if line_text.ends_with("\r\n") {
                            line_length = line_length.saturating_sub(1);
                        }
                    }
                    let line_end = line_start + line_length;
                    self.rope.char_to_byte(line_end)
                }
            }
        };
        self.selection = EditorSelection::caret(offset);
        Ok(())
    }

    pub fn undo(&mut self) -> Result<(), EditorError> {
        if self.read_only {
            return Err(EditorError::ReadOnly);
        }
        let Some(entry) = self.undo.pop() else {
            return Err(EditorError::NoUndo);
        };
        for edit in entry.edits.iter().rev() {
            self.replace_bytes(
                edit.start_byte,
                edit.start_byte + edit.inserted.len(),
                &edit.deleted,
            )?;
        }
        self.revision = self.revision.saturating_add(1);
        self.cached_content_hash = None;
        self.selection = entry.before_selection;
        self.redo.push(entry);
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), EditorError> {
        if self.read_only {
            return Err(EditorError::ReadOnly);
        }
        let Some(entry) = self.redo.pop() else {
            return Err(EditorError::NoRedo);
        };
        for edit in &entry.edits {
            self.replace_bytes(
                edit.start_byte,
                edit.start_byte + edit.deleted.len(),
                &edit.inserted,
            )?;
        }
        self.revision = self.revision.saturating_add(1);
        self.cached_content_hash = None;
        self.selection = entry.after_selection;
        self.undo.push(entry);
        Ok(())
    }

    #[must_use]
    pub fn content_hash(&self) -> Hash {
        // This is an explicit identity operation, not part of viewport
        // rendering. Callers that need a save precondition may pay for the
        // complete hash deliberately.
        hash_rope(&self.rope)
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    #[must_use]
    pub fn selected_text(&self) -> String {
        let start = self.selection.start();
        let end = self.selection.end();
        if start == end {
            return String::new();
        }
        let Ok(char_start) = self.rope.try_byte_to_char(start) else {
            return String::new();
        };
        let Ok(char_end) = self.rope.try_byte_to_char(end) else {
            return String::new();
        };
        self.rope.slice(char_start..char_end).to_string()
    }

    #[must_use]
    pub fn viewport(&self, first_line: usize, last_line: usize) -> EditorViewportSnapshot {
        let total_lines = self.rope.len_lines();
        let first_line = first_line.min(total_lines.saturating_sub(1));
        let last_line = last_line.max(first_line).min(total_lines.saturating_sub(1));
        let lines = (first_line..=last_line)
            .map(|line_index| ViewportLine {
                line_index,
                number: line_index + 1,
                start_byte: self.rope.char_to_byte(self.rope.line_to_char(line_index)),
                text: self
                    .rope
                    .line(line_index)
                    .to_string()
                    .trim_end_matches(['\r', '\n'])
                    .to_owned(),
            })
            .collect();
        EditorViewportSnapshot {
            document_id: self.id,
            revision: self.revision,
            // Hashing an entire document is intentionally not on the render
            // path. A missing value means the content changed since the last
            // explicit hash request; save/revision code asks `content_hash()`.
            content_hash: self.cached_content_hash.map(|hash| hash.to_hex().to_string()),
            first_line,
            last_line,
            total_lines,
            total_bytes: self.rope.len_bytes(),
            selection: self.selection,
            modified: self.is_modified(),
            read_only: self.read_only,
            lines,
        }
    }

    fn ensure_boundary(&self, offset: usize) -> Result<(), EditorError> {
        if offset > self.rope.len_bytes() {
            return Err(EditorError::InvalidRange {
                start_byte: offset,
                end_byte: offset,
                length: self.rope.len_bytes(),
            });
        }
        let char_index = self
            .rope
            .try_byte_to_char(offset)
            .map_err(|_| EditorError::InvalidUtf8Boundary(offset))?;
        let canonical = self
            .rope
            .try_char_to_byte(char_index)
            .map_err(|_| EditorError::InvalidUtf8Boundary(offset))?;
        if canonical != offset {
            return Err(EditorError::InvalidUtf8Boundary(offset));
        }
        Ok(())
    }

    fn replace_bytes(
        &mut self,
        start_byte: usize,
        end_byte: usize,
        replacement: &str,
    ) -> Result<(), EditorError> {
        self.ensure_boundary(start_byte)?;
        self.ensure_boundary(end_byte)?;
        if start_byte > end_byte {
            return Err(EditorError::InvalidRange {
                start_byte,
                end_byte,
                length: self.rope.len_bytes(),
            });
        }
        let start = self
            .rope
            .try_byte_to_char(start_byte)
            .map_err(|_| EditorError::InvalidUtf8Boundary(start_byte))?;
        let end = self
            .rope
            .try_byte_to_char(end_byte)
            .map_err(|_| EditorError::InvalidUtf8Boundary(end_byte))?;
        self.rope
            .try_remove(start..end)
            .map_err(|error| EditorError::InvalidEdit(error.to_string()))?;
        self.rope
            .try_insert(start, replacement)
            .map_err(|error| EditorError::InvalidEdit(error.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretMovement {
    Left,
    Right,
    Home,
    End,
}

#[derive(Debug, Default)]
pub struct Editor {
    documents: BTreeMap<EditorDocumentId, Document>,
}

impl Editor {
    pub fn open_document(
        &mut self,
        id: EditorDocumentId,
        source: &str,
        read_only: bool,
    ) -> Result<(), EditorError> {
        if self.documents.contains_key(&id) {
            return Err(EditorError::DocumentExists(id));
        }
        self.documents.insert(id, Document::new(id, source, read_only));
        Ok(())
    }

    pub fn close_document(&mut self, id: EditorDocumentId) -> Result<(), EditorError> {
        self.documents.remove(&id).map(|_| ()).ok_or(EditorError::DocumentNotFound(id))
    }

    pub fn document(&self, id: EditorDocumentId) -> Result<&Document, EditorError> {
        self.documents.get(&id).ok_or(EditorError::DocumentNotFound(id))
    }

    pub fn document_mut(&mut self, id: EditorDocumentId) -> Result<&mut Document, EditorError> {
        self.documents.get_mut(&id).ok_or(EditorError::DocumentNotFound(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transaction(document: &Document, operation: EditOperation) -> EditTransaction {
        EditTransaction {
            base_revision: document.revision(),
            operations: vec![operation],
            before_selection: document.selection(),
            after_selection: EditorSelection::caret(1),
        }
    }

    #[test]
    fn rope_edit_does_not_require_full_document_replacement() {
        let mut document = Document::new(7, "alpha\nbeta\n", false);
        let result = document
            .apply_transaction(transaction(&document, EditOperation::replace(6, 10, "gamma")));
        assert!(result.is_ok());
        assert_eq!(document.text(), "alpha\ngamma\n");
        assert_eq!(document.revision(), 1);
        assert!(document.is_modified());
    }

    #[test]
    fn undo_redo_restore_exact_unicode_content() {
        let mut document = Document::new(1, "🙂 café\n", false);
        let start = "🙂 ".len();
        let end = start + "café".len();
        let result = document.apply_transaction(EditTransaction {
            base_revision: 0,
            operations: vec![EditOperation::replace(start, end, "世界")],
            before_selection: EditorSelection::caret(start),
            after_selection: EditorSelection::caret(start + "世界".len()),
        });
        assert!(result.is_ok());
        assert_eq!(document.text(), "🙂 世界\n");
        assert!(document.undo().is_ok());
        assert_eq!(document.text(), "🙂 café\n");
        assert!(document.redo().is_ok());
        assert_eq!(document.text(), "🙂 世界\n");
    }

    #[test]
    fn viewport_is_bounded_and_serializable() {
        let document = Document::new(5, "one\ntwo\nthree\n", false);
        let snapshot = document.viewport(1, 1);
        assert_eq!(snapshot.lines.len(), 1);
        assert_eq!(snapshot.lines[0].text, "two");
        // Ropey preserves the final empty line after a trailing newline.
        assert_eq!(snapshot.total_lines, 4);
    }

    #[test]
    fn invalid_utf8_boundary_is_rejected() {
        let mut document = Document::new(1, "🙂", false);
        let result = document.apply_transaction(EditTransaction {
            base_revision: 0,
            operations: vec![EditOperation::replace(1, 1, "x")],
            before_selection: EditorSelection::caret(0),
            after_selection: EditorSelection::caret(1),
        });
        assert!(matches!(result, Err(EditorError::InvalidUtf8Boundary(1))));
    }
}
