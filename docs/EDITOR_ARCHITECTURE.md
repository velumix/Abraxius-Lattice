# Lattice Editor Architecture

The editor is a native visual surface over Lattice's existing resource and
revision model. It is deliberately split into a hot in-process Rust core and
an Avalonia presentation layer.

```text
Avalonia EditorSurface
        │ visible viewport snapshots / input commands
        ▼
Lattice.Editor.Interop (narrow C ABI wrapper)
        │ opaque handles and UTF-8 buffers
        ▼
lattice-editor-native (cdylib boundary)
        ▼
lattice-editor-core
        ├── rope-backed document buffers
        ├── byte-safe selections and transactions
        ├── revisioned undo/redo
        └── immutable bounded viewport snapshots
```

The current E0/E1 slice intentionally does not implement project-wide
semantics, Tree-sitter, official Luau analysis, Git, Studio save, or ChangeSet
mutation. Those remain daemon-owned integrations. The core's document ID,
revision, UTF-8 byte coordinates, content hash, and read-only state are the
contract those services will consume.

## Hot-path rules

- A keystroke is an edit transaction against a `ropey` rope. It does not copy
  the complete document into C#.
- Avalonia asks for only the visible line range. Returned snapshots are
  immutable serialized values and contain line start byte offsets for hit
  testing.
- The native ABI uses opaque handles, explicit lengths, UTF-8 validation,
  contained panics, and a separately freed string allocation.
- Full document extraction exists only for explicit save workflows. It is not
  used by rendering or input handling.
- Rust owns undo/redo and selection state. Avalonia owns pixels, hit testing,
  keyboard/IME event delivery, and clipboard integration.

## First vertical slice

The desktop app exposes an **Editor** navigation surface and opens the first
Luau/Lua file in the selected workspace. Files can also be double-clicked in
Explorer. The surface supports native line rendering, caret movement, text
input, deletion, Home/End, copy-independent undo/redo, wheel scrolling, and a
visible modified/revision status. The native library is loaded from the app
directory when present; if it is unavailable the UI states that truthfully.

## Follow-on integration points

1. Map `EditorDocumentId` to canonical `rbx://` resource references.
2. Replace local file opening with daemon source/revision acquisition.
3. Add incremental Tree-sitter presentation and asynchronous official Luau
   semantic diagnostics.
4. Route save through the existing revision/ChangeSet/Studio adapters and
   verify the returned hash before marking the buffer clean.
5. Add Git and Flight Recorder decorations as overlays, never as source-state
   replacements.

The native library is intentionally a separate crate so later Windows and
macOS packaging can ship the same ABI with `.dll`/`.dylib` artifacts while
keeping the editor domain model platform-neutral.
