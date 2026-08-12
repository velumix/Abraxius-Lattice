# Abraxius Lattice

**Lattice — The Native Intelligence Layer for Roblox**

Lattice is a native Rust intelligence and transaction layer for Roblox projects. It maintains canonical resource identities, content-addressed revisions, structured Luau facts, searchable evidence, and safe adapter boundaries. MCP is an external protocol adapter; it is not the core data model.

## Current vertical slice

The bootstrap implementation can:

- open a local Roblox/Luau workspace without an LLM;
- assign persistent `rbx://` resource references;
- store metadata in SQLite and immutable source in a BLAKE3 object store;
- parse Luau through the pinned official Luau C++ AST implementation;
- extract functions, locals, type aliases, references, calls, and requires;
- incrementally update a Tantivy source index and a dependency graph;
- search from the native CLI and expose the authoritative daemon surface over
  the thin `lattice mcp stdio` bridge;
- model Studio sessions and capabilities behind a protocol-neutral adapter boundary.
- resolve native Windows/macOS and Vinegar XDG/Flatpak Studio environments behind typed semantic paths.
- accept automatically paired, bounded, per-window reports from the optional
  Studio companion plugin through a native Rust loopback service.

## Build and try it

```text
cargo build --locked
cargo test --locked
cargo run --locked -p lattice-cli -- workspace open fixtures/sample-project
cargo run --locked -p lattice-cli -- workspace status fixtures/sample-project
cargo run --locked -p lattice-cli -- search fixtures/sample-project inventory
cargo run --locked -p lattice-daemon -- --workspace fixtures/sample-project
cargo run --locked -p lattice-cli -- mcp status
cargo run --locked -p lattice-cli -- integration codex install
cargo run --locked -p lattice-cli -- integration codex status
cargo run --locked -p lattice-cli -- studio environment
cargo run --locked -p lattice-cli -- studio environment --verbose
```

The Avalonia desktop shell starts the optional loopback companion bridge in
bridge-only mode; the Studio plugin discovers and pairs with it automatically.
The daemon never starts or restarts Studio. See
[`docs/STUDIO_COMPANION_BRIDGE.md`](docs/STUDIO_COMPANION_BRIDGE.md) for the
standalone daemon workflow.

The platform diagnostic is read-only and never launches Studio. It reports explicit missing, unavailable, permission-denied, sandbox-denied, and ambiguous states. Live southbound Studio MCP transport remains a separate roadmap item; process detection is not presented as an MCP connection.
