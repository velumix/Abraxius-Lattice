# Abraxius Lattice architecture

Status: bootstrap implementation, 2026-08-11.

## Purpose

Lattice is the authoritative native intelligence and transaction layer for Roblox projects. Project content enters through adapters and becomes canonical entities, immutable revisions, graph relationships, diagnostics, evidence, and eventually ChangeSets. MCP is a northbound and southbound adapter; it is not a core dependency or data model.

```text
External apps / LLMs / CI
            |
      Lattice MCP server             Avalonia workstation
            |                              |
            v                         native IPC
   protocol-neutral services               |
            +---------------+--------------+
                            v
                      Lattice Core
             model / storage / Luau / index / graph
                            |
            +---------------+----------------+
            |               |                |
       Studio MCP       files/Rojo       Open Cloud
       client adapter    /place files      adapter
```

## Dependency rule

The enforced conceptual direction is:

```text
lattice-resource <- lattice-model
       ^                 ^
       |                 |
storage / luau / search / graph / platform
       ^
       |
 tools <- connections
       ^
       |
lattice-core
       ^
       |
mcp / studio / daemon / cli / future IPC
```

`lattice-core` has no dependency on RMCP, Avalonia, model-provider APIs, rbx-dom, or Studio-native types. `lattice-mcp` owns all RMCP types. `lattice-luau-sys` owns all Luau C++ types and pointers; only owned Rust facts cross its CXX boundary.

## Implemented bootstrap path

1. Canonicalize a local workspace root and create its disposable `.lattice` cache.
2. Open SQLite with foreign keys, WAL, a busy timeout, and embedded migrations.
3. Discover `.luau`/`.lua` files without following symlinks or leaving the workspace.
4. Hash source with BLAKE3 and atomically store immutable content objects.
5. Parse source through official Luau 0.733 C++ AST code compiled through CXX.
6. Persist identities, revisions, symbols, references, `REQUIRES` edges, and parse diagnostics.
7. Incrementally replace each changed resource document in Tantivy.
8. Return compact search hits with stable `rbx://` references and evidence IDs through CLI or stdio MCP.
9. Register built-in/external providers behind stable identities and progressively discover content-addressed tool schemas.

The current graph process persists AST-derived require edges but resolves only entities already known by canonical reference. Module-path resolution and cross-source graph edges are the next graph increment.

## Concurrency and blocking work

Tokio is the I/O runtime. MCP calls dispatch SQLite/Tantivy work through `spawn_blocking`. A bounded `JobSystem` uses a semaphore; the event bus uses a bounded Tokio broadcast channel. CPU-wide parallel parsing is intentionally deferred until per-source behavior is correct and measured.

No unbounded application queue is present. The connection broker uses global and per-provider semaphores, bounded timeouts, explicit cancellation, and large-result limits.

## Trust boundaries

Source, comments, assets, project files, MCP clients, Studio responses, runtime output, and cloud responses are untrusted data. They never become policy or instructions. Core mutations will require a ChangeSet, content/revision preconditions, authorization, an audit event, and verification.

## Studio platform boundary

`lattice-platform` owns all native Windows, native macOS, Vinegar XDG, Vinegar Flatpak, process, prefix, deployment, semantic-root, and Wine translation logic. `lattice-studio` stores the resulting `StudioEnvironmentId` on each MCP session. Future recorder, IPC, and UI consumers receive sanitized semantic data and never assemble Vinegar paths. See [PLATFORM_ARCHITECTURE.md](PLATFORM_ARCHITECTURE.md).

## Studio companion boundary

`lattice-studio-bridge` is an optional second Studio transport. A minimal Luau
plugin initiates automatically paired loopback HTTP reports to the native Rust daemon,
providing stable per-window session registration, bounded events, and a bounded
read-only command channel. It is not MCP and does not replace the built-in
Studio MCP provider. Lattice binds a companion session to an already-resolved
`StudioSession`, `StudioEnvironmentId`, and process; the bridge never chooses a
"most recent" Studio for a mutation. See
[STUDIO_COMPANION_BRIDGE.md](STUDIO_COMPANION_BRIDGE.md).

## Baseline host

The initial host has Rust 1.97.1 and GCC 13.3.0; the system PATH does not
include .NET, CMake, or the sqlite3 CLI. The Avalonia 12.1.1 workstation shell
now builds with a current .NET SDK and starts independently of the daemon in an
explicit disconnected state. A live Roblox Studio process under Vinegar
Flatpak is resolved read-only. An explicit experimental launcher has completed
real single-Studio MCP discovery and state inspection without starting or
stopping Studio; real multiple-window mapping remains unverified because only
one Studio window is available. Flight Recorder and the native IPC transport
remain unimplemented.

See [ROADMAP.md](ROADMAP.md), [MCP_SURFACE.md](MCP_SURFACE.md), and the ADRs under `docs/adr`.
