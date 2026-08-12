//! Narrow C ABI for the in-process Lattice editor core.
//!
//! The ABI intentionally exchanges only UTF-8 buffers and JSON viewport
//! snapshots.  Rust editor state remains opaque and lives behind a handle;
//! callers never receive a mutable rope or a Rust allocation.

#![allow(unsafe_code)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::slice;
use std::sync::Mutex;

use lattice_editor_core::{
    CaretMovement, EditOperation, EditTransaction, Editor, EditorError, EditorSelection,
};

pub struct EditorHandle {
    editor: Mutex<Editor>,
    last_error: Mutex<Option<String>>,
}

fn error_message(error: &dyn ToString) -> String {
    error.to_string()
}

unsafe fn bytes<'a>(pointer: *const u8, length: usize) -> Result<&'a [u8], String> {
    if length == 0 {
        return Ok(&[]);
    }
    if pointer.is_null() {
        return Err("a non-zero buffer length requires a non-null pointer".to_owned());
    }
    Ok(slice::from_raw_parts(pointer, length))
}

fn set_error(handle: &EditorHandle, message: impl Into<String>) {
    if let Ok(mut error) = handle.last_error.lock() {
        *error = Some(message.into());
    }
}

fn clear_error(handle: &EditorHandle) {
    if let Ok(mut error) = handle.last_error.lock() {
        *error = None;
    }
}

#[allow(clippy::needless_pass_by_value)]
fn write_error(handle: &EditorHandle, error: impl ToString) -> i32 {
    set_error(handle, error_message(&error));
    -1
}

fn with_editor<T>(
    handle: &EditorHandle,
    operation: impl FnOnce(&mut Editor) -> Result<T, EditorError>,
) -> Result<T, String> {
    let mut editor = handle.editor.lock().map_err(|_| "editor state is poisoned".to_owned())?;
    operation(&mut editor).map_err(|error| error_message(&error))
}

fn json_string(value: impl serde::Serialize) -> *mut c_char {
    let Ok(json) = serde_json::to_string(&value) else {
        return std::ptr::null_mut();
    };
    CString::new(json).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// Creates an empty native editor instance.
#[unsafe(no_mangle)]
pub extern "C" fn lattice_editor_create() -> *mut EditorHandle {
    Box::into_raw(Box::new(EditorHandle {
        editor: Mutex::new(Editor::default()),
        last_error: Mutex::new(None),
    }))
}

/// Destroys an editor returned by [`lattice_editor_create`].
///
/// # Safety
/// `handle` must be null or a pointer previously returned by
/// `lattice_editor_create` that has not already been destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_destroy(handle: *mut EditorHandle) {
    if handle.is_null() {
        return;
    }
    drop(Box::from_raw(handle));
}

/// Opens one document in the native editor.
///
/// # Safety
/// `handle` must be a valid editor handle. `source` must point to `length`
/// bytes of UTF-8 for the duration of the call (or be null when `length == 0`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_open(
    handle: *mut EditorHandle,
    document_id: u64,
    source: *const u8,
    length: usize,
    read_only: bool,
) -> i32 {
    if handle.is_null() {
        return -1;
    }
    let handle = &*handle;
    clear_error(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let source = bytes(source, length)?;
        let source = std::str::from_utf8(source)
            .map_err(|error| format!("document source is not UTF-8: {error}"))?;
        with_editor(handle, |editor| editor.open_document(document_id, source, read_only))
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => write_error(handle, error),
        Err(_) => write_error(handle, "native editor panicked while opening a document"),
    }
}

/// Closes a document.
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_close(
    handle: *mut EditorHandle,
    document_id: u64,
) -> i32 {
    if handle.is_null() {
        return -1;
    }
    let handle = &*handle;
    clear_error(handle);
    match catch_unwind(AssertUnwindSafe(|| {
        with_editor(handle, |editor| editor.close_document(document_id))
    })) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => write_error(handle, error),
        Err(_) => write_error(handle, "native editor panicked while closing a document"),
    }
}

/// Applies one replacement transaction to a document.
///
/// # Safety
/// `handle` must be valid and `inserted` must point to `inserted_length` bytes
/// of UTF-8 for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_apply_edit(
    handle: *mut EditorHandle,
    document_id: u64,
    base_revision: u64,
    start_byte: usize,
    end_byte: usize,
    inserted: *const u8,
    inserted_length: usize,
    selection_anchor: usize,
    selection_head: usize,
) -> i32 {
    if handle.is_null() {
        return -1;
    }
    let handle = &*handle;
    clear_error(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let inserted = bytes(inserted, inserted_length)?;
        let inserted = std::str::from_utf8(inserted)
            .map_err(|error| format!("inserted text is not UTF-8: {error}"))?;
        with_editor(handle, |editor| {
            let before_selection = editor.document(document_id)?.selection();
            editor.document_mut(document_id)?.apply_transaction(EditTransaction {
                base_revision,
                operations: vec![EditOperation::replace(start_byte, end_byte, inserted)],
                before_selection,
                after_selection: EditorSelection { anchor: selection_anchor, head: selection_head },
            })
        })
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => write_error(handle, error),
        Err(_) => write_error(handle, "native editor panicked while applying an edit"),
    }
}

/// Inserts text at the current selection and collapses the selection after it.
///
/// # Safety
/// `handle` must be valid and `inserted` must point to `inserted_length` bytes
/// of UTF-8 for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_insert_text(
    handle: *mut EditorHandle,
    document_id: u64,
    inserted: *const u8,
    inserted_length: usize,
) -> i32 {
    document_text_operation(handle, document_id, inserted, inserted_length, |document, text| {
        document.insert_text(text)
    })
}

/// Deletes one Unicode scalar before the caret, or the current selection.
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_delete_backward(
    handle: *mut EditorHandle,
    document_id: u64,
) -> i32 {
    document_edit_operation(handle, document_id, lattice_editor_core::Document::delete_backward)
}

/// Deletes one Unicode scalar after the caret, or the current selection.
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_delete_forward(
    handle: *mut EditorHandle,
    document_id: u64,
) -> i32 {
    document_edit_operation(handle, document_id, lattice_editor_core::Document::delete_forward)
}

/// Moves the caret without modifying document content. `movement` is 0=left,
/// 1=right, 2=home, 3=end.
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_move_caret(
    handle: *mut EditorHandle,
    document_id: u64,
    movement: u8,
) -> i32 {
    let movement = match movement {
        0 => CaretMovement::Left,
        1 => CaretMovement::Right,
        2 => CaretMovement::Home,
        3 => CaretMovement::End,
        _ => {
            if handle.is_null() {
                return -1;
            }
            let handle = &*handle;
            return write_error(handle, "unknown caret movement");
        }
    };
    document_edit_operation(handle, document_id, |document| document.move_caret(movement))
}

unsafe fn document_text_operation(
    handle: *mut EditorHandle,
    document_id: u64,
    inserted: *const u8,
    inserted_length: usize,
    operation: impl FnOnce(&mut lattice_editor_core::Document, &str) -> Result<(), EditorError>,
) -> i32 {
    if handle.is_null() {
        return -1;
    }
    let handle = &*handle;
    clear_error(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let inserted = bytes(inserted, inserted_length)?;
        let inserted = std::str::from_utf8(inserted)
            .map_err(|error| format!("inserted text is not UTF-8: {error}"))?;
        with_editor(handle, |editor| operation(editor.document_mut(document_id)?, inserted))
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => write_error(handle, error),
        Err(_) => write_error(handle, "native editor panicked while inserting text"),
    }
}

unsafe fn document_edit_operation(
    handle: *mut EditorHandle,
    document_id: u64,
    operation: impl FnOnce(&mut lattice_editor_core::Document) -> Result<(), EditorError>,
) -> i32 {
    if handle.is_null() {
        return -1;
    }
    let handle = &*handle;
    clear_error(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_editor(handle, |editor| operation(editor.document_mut(document_id)?))
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => write_error(handle, error),
        Err(_) => write_error(handle, "native editor panicked while editing the document"),
    }
}

/// Undoes the latest transaction.
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_undo(
    handle: *mut EditorHandle,
    document_id: u64,
) -> i32 {
    document_history_call(handle, document_id, lattice_editor_core::Document::undo)
}

/// Redoes the latest undone transaction.
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_redo(
    handle: *mut EditorHandle,
    document_id: u64,
) -> i32 {
    document_history_call(handle, document_id, lattice_editor_core::Document::redo)
}

unsafe fn document_history_call(
    handle: *mut EditorHandle,
    document_id: u64,
    operation: impl FnOnce(&mut lattice_editor_core::Document) -> Result<(), EditorError>,
) -> i32 {
    if handle.is_null() {
        return -1;
    }
    let handle = &*handle;
    clear_error(handle);
    match catch_unwind(AssertUnwindSafe(|| {
        with_editor(handle, |editor| operation(editor.document_mut(document_id)?))
    })) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => write_error(handle, error),
        Err(_) => write_error(handle, "native editor panicked while changing history"),
    }
}

/// Marks the current revision as clean after an external save has verified it.
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_mark_clean(
    handle: *mut EditorHandle,
    document_id: u64,
    revision: u64,
) -> i32 {
    if handle.is_null() {
        return -1;
    }
    let handle = &*handle;
    clear_error(handle);
    match catch_unwind(AssertUnwindSafe(|| {
        with_editor(handle, |editor| editor.document_mut(document_id)?.mark_clean(revision))
    })) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => write_error(handle, error),
        Err(_) => write_error(handle, "native editor panicked while marking a document clean"),
    }
}

/// Sets the active selection in byte coordinates.
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_set_selection(
    handle: *mut EditorHandle,
    document_id: u64,
    anchor: usize,
    head: usize,
) -> i32 {
    if handle.is_null() {
        return -1;
    }
    let handle = &*handle;
    clear_error(handle);
    match catch_unwind(AssertUnwindSafe(|| {
        with_editor(handle, |editor| {
            editor.document_mut(document_id)?.set_selection(EditorSelection { anchor, head })
        })
    })) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => write_error(handle, error),
        Err(_) => write_error(handle, "native editor panicked while setting selection"),
    }
}

/// Returns an immutable JSON viewport snapshot. The returned string must be
/// released with [`lattice_editor_free_string`].
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_snapshot_json(
    handle: *mut EditorHandle,
    document_id: u64,
    first_line: usize,
    last_line: usize,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let handle = &*handle;
    clear_error(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_editor(handle, |editor| {
            Ok(editor.document(document_id)?.viewport(first_line, last_line))
        })
    }));
    match result {
        Ok(Ok(snapshot)) => json_string(snapshot),
        Ok(Err(error)) => {
            set_error(handle, error);
            std::ptr::null_mut()
        }
        Err(_) => {
            set_error(handle, "native editor panicked while creating a viewport snapshot");
            std::ptr::null_mut()
        }
    }
}

/// Returns the current selection as a JSON string value. This is bounded by
/// the selection, not the complete document, and is intended for clipboard
/// operations.
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_selection_text(
    handle: *mut EditorHandle,
    document_id: u64,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let handle = &*handle;
    clear_error(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_editor(handle, |editor| Ok(editor.document(document_id)?.selected_text()))
    }));
    match result {
        Ok(Ok(text)) => json_string(text),
        Ok(Err(error)) => {
            set_error(handle, error);
            std::ptr::null_mut()
        }
        Err(_) => {
            set_error(handle, "native editor panicked while reading the selection");
            std::ptr::null_mut()
        }
    }
}

/// Runs the official Lattice Luau analyzer against a document snapshot. The
/// caller should invoke this off the UI thread and discard stale revisions.
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_luau_analysis_json(
    handle: *mut EditorHandle,
    document_id: u64,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let handle = &*handle;
    clear_error(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_editor(handle, |editor| {
            let document = editor.document(document_id)?;
            Ok(lattice_luau::analyze(&document.text()))
        })
    }));
    match result {
        Ok(Ok(analysis)) => json_string(analysis),
        Ok(Err(error)) => {
            set_error(handle, error);
            std::ptr::null_mut()
        }
        Err(_) => {
            set_error(handle, "official Luau analysis panicked");
            std::ptr::null_mut()
        }
    }
}

/// Returns a full document copy for explicit save operations only.
///
/// This function is intentionally absent from the per-keystroke path.
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_document_text(
    handle: *mut EditorHandle,
    document_id: u64,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let handle = &*handle;
    clear_error(handle);
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_editor(handle, |editor| Ok(editor.document(document_id)?.text()))
    }));
    match result {
        Ok(Ok(text)) => json_string(text),
        Ok(Err(error)) => {
            set_error(handle, error);
            std::ptr::null_mut()
        }
        Err(_) => {
            set_error(handle, "native editor panicked while reading document text");
            std::ptr::null_mut()
        }
    }
}

/// Returns and clears the latest ABI error. Release the result with
/// [`lattice_editor_free_string`].
///
/// # Safety
/// `handle` must be a valid editor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_last_error(handle: *mut EditorHandle) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let handle = &*handle;
    let message = handle.last_error.lock().ok().and_then(|mut error| error.take());
    message
        .and_then(|value| CString::new(value).ok())
        .map_or(std::ptr::null_mut(), CString::into_raw)
}

/// Releases a string returned by this library.
///
/// # Safety
/// `value` must be null or a pointer returned by this library that has not
/// already been released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lattice_editor_free_string(value: *mut c_char) {
    if value.is_null() {
        return;
    }
    drop(CString::from_raw(value));
}
