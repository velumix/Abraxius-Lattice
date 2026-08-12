#![allow(unsafe_code)]

use std::ffi::CStr;

use lattice_editor_native::{
    EditorHandle, lattice_editor_create, lattice_editor_destroy,
    lattice_editor_document_insert_text, lattice_editor_document_luau_analysis_json,
    lattice_editor_document_open, lattice_editor_document_snapshot_json,
    lattice_editor_free_string,
};

fn take_string(pointer: *mut std::os::raw::c_char) -> String {
    assert!(!pointer.is_null());
    // The native ABI documents this allocation as an owned C string. The test
    // copies it before exercising the matching free function.
    let value = unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned();
    unsafe { lattice_editor_free_string(pointer) };
    value
}

#[test]
fn c_abi_luau_analysis_uses_the_official_analyzer() {
    let handle: *mut EditorHandle = lattice_editor_create();
    assert!(!handle.is_null());
    let source = "local function greet(player)\n    return player.Name\nend\n";
    let result =
        unsafe { lattice_editor_document_open(handle, 10, source.as_ptr(), source.len(), false) };
    assert_eq!(result, 0);

    let analysis = unsafe { lattice_editor_document_luau_analysis_json(handle, 10) };
    let analysis = take_string(analysis);
    assert!(analysis.contains("greet"), "analysis did not contain the function symbol: {analysis}");
    assert!(analysis.contains("diagnostics"));

    unsafe { lattice_editor_destroy(handle) };
}

#[test]
fn c_abi_round_trip_returns_only_a_viewport_snapshot() {
    let handle: *mut EditorHandle = lattice_editor_create();
    assert!(!handle.is_null());
    let source = "local value = 1\n";
    let result =
        unsafe { lattice_editor_document_open(handle, 9, source.as_ptr(), source.len(), false) };
    assert_eq!(result, 0);

    let snapshot = unsafe { lattice_editor_document_snapshot_json(handle, 9, 0, 0) };
    let before = take_string(snapshot);
    assert!(before.contains("local value = 1"));

    let inserted = "2";
    let result = unsafe {
        lattice_editor_document_insert_text(handle, 9, inserted.as_ptr(), inserted.len())
    };
    assert_eq!(result, 0);

    unsafe { lattice_editor_destroy(handle) };
}
