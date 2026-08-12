# Dependency record

Verified 2026-08-11 against crates.io package metadata and the upstream repositories linked below. Every direct Rust dependency is exact-pinned in the workspace manifest and resolved transitively in `Cargo.lock`.

| Dependency | Pin | License | Purpose and review note |
|---|---:|---|---|
| Axum | 0.8.9 | MIT | Authenticated loopback HTTP for the optional Studio companion bridge; official Tokio project, Rust 1.80 minimum, default features disabled and only HTTP/1, JSON, Tokio, and tracing enabled. |
| BLAKE3 | 1.8.6 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | Fast content identity and object addressing; official BLAKE3 team repository, actively released. |
| clap | 4.6.6 | MIT OR Apache-2.0 | Native CLI parsing; actively maintained official repository. |
| CXX / cxx-build | 1.0.199 | MIT OR Apache-2.0 | Narrow ownership-safe Rust/C++ Luau boundary; actively maintained by dtolnay. |
| dirs | 6.0.0 | MIT OR Apache-2.0 | XDG, macOS standard-directory, and Windows Known Folder resolution; maintained `dirs-rs` release. |
| petgraph | 0.8.3 | MIT OR Apache-2.0 | In-memory structural graph; mature, actively released. |
| proptest | 1.11.0 | MIT OR Apache-2.0 | Property tests for reference and future ChangeSet invariants. Development only. |
| ropey | 1.6.1 | MIT | Persistent text rope for `lattice-editor-core`; exact-pinned, benchmarkable, and used only for active document storage. |
| RMCP | 3.1.2 | Apache-2.0 | Official Rust MCP SDK, isolated in `lattice-mcp`; `client`, `server`, `macros`, and native async-I/O features are enabled for separated northbound/server and southbound/client roles. |
| rusqlite | 0.40.2 | MIT | Embedded SQLite; `bundled` selected, default wasm backend disabled. |
| serde / serde_json | 1.0.229 / 1.0.151 | MIT OR Apache-2.0 | Versioned native data contracts and structured output. |
| sysinfo | 0.39.6 | MIT | Cross-platform process executable, parent, command, environment, and start-time snapshots; only the `system` feature is enabled. |
| Tantivy | 0.26.1 | MIT | Incremental full-text index; default NLP features disabled to reduce surface. |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | Isolated filesystem tests. Development only. |
| thiserror | 2.0.20 | MIT OR Apache-2.0 | Typed error implementations. |
| Tokio | 1.53.1 | MIT | Bounded asynchronous I/O, process, signal, and synchronization runtime. |
| tracing / tracing-subscriber | 0.1.44 / 0.3.23 | MIT | Structured local observability. |
| UUID | 1.24.0 | MIT OR Apache-2.0 | Time-ordered UUIDv7 Lattice identifiers. |
| Luau | tag 0.733, commit `ca128af4c531310d6f5c1b354df4b79fdd782ede` | MIT | Official parser/AST implementation, pinned Git submodule; no raw pointer crosses the FFI boundary. |
| Fusion | tag `v0.3-beta`, commit `77e603534ff4013f4049611826ff0309d6000b15` | MIT | Reactive Roblox Studio plugin UI, pinned Git submodule. All Fusion usage is isolated behind `FusionAdapter` and Lattice-owned components. No transitive packages. |
| Material.Icons.Avalonia | 3.0.2 | MIT | Strongly typed Material Design icon controls for the Avalonia workstation; exact-pinned and isolated to the presentation layer. Compatible with Avalonia 12 and .NET 8. |
| lru (transitive backport) | 0.16.4 + upstream PR #238 | MIT | Tantivy constrains 0.16.x. Local exact source includes the upstream panic-safety fix and regression test; see `SECURITY_AUDIT.md`. |

Native development tools are exact-pinned in `rokit.toml`: Rojo 7.7.0 packages the Studio plugin model, StyLua 2.5.2 formats Luau, and Selene 0.31.0 provides supplemental Roblox-aware linting. They are build tools, not runtime services; all are native executables and introduce no Node or Python runtime.

Primary repositories: [RMCP](https://github.com/modelcontextprotocol/rust-sdk), [Luau](https://github.com/luau-lang/luau), [BLAKE3](https://github.com/BLAKE3-team/BLAKE3), [rusqlite](https://github.com/rusqlite/rusqlite), [Tantivy](https://github.com/quickwit-oss/tantivy), [petgraph](https://github.com/petgraph/petgraph), [Tokio](https://github.com/tokio-rs/tokio), [sysinfo](https://github.com/GuillaumeGomez/sysinfo), [dirs-rs](https://github.com/dirs-dev/dirs-rs), and [CXX](https://github.com/dtolnay/cxx).

## Deliberately not adopted yet

rbx-dom libraries, Salsa, notify, Rayon, git2, Cedar, ONNX Runtime, USearch,
Wasmtime, and OpenTelemetry remain deferred until the phase that exercises
them. Avalonia is now adopted by the native workstation shell at exact version
`12.1.1`; it remains isolated to `app/Abraxius.Lattice` and does not enter the
Rust core.

No JavaScript, Node, npm, Python runtime, database daemon, Redis, or external search service is used. CI workflow selection is deferred because common marketplace actions execute JavaScript; native build commands and a platform matrix are recorded in `BUILD_MATRIX.md` without introducing that dependency.

## Required audit gate

RustSec auditing is active. Run `cargo audit --deny warnings --ignore RUSTSEC-2026-0253`; the sole ignored version-range match is a locally backported and regression-tested copy of the advisory's upstream fix. Before a release also run `cargo deny check`, complete source/license review for every transitive package, and produce the final SBOM.
