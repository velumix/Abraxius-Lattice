# Lattice implementation roadmap

## Current: native bootstrap

- [x] Rust 1.97.1 workspace and exact dependency pins
- [x] canonical `rbx://` parser and property tests
- [x] SQLite WAL schema and BLAKE3 object store
- [x] protocol-neutral model, structured errors, bounded events/jobs
- [x] official Luau 0.733 AST CXX bridge and compatibility fixtures
- [x] local source ingestion and persistent identities
- [x] symbol/reference/require extraction
- [x] Tantivy incremental document replacement
- [x] evidence-bearing graph abstraction
- [x] CLI, daemon bootstrap, and stdio Lattice MCP search
- [x] centralized cross-platform Studio environment, process, and path resolution
- [x] Vinegar XDG/Flatpak discovery and secure C:/Z: translation
- [x] Studio MCP session-to-environment identity contract
- [x] read-only live Vinegar Flatpak environment acceptance on the Linux host
- [x] protocol-isolated southbound RMCP client, lifecycle, tool discovery, calls, cancellation, timeout, disconnect, and reconnect tests
- [x] modern `server/discover` and controlled legacy MCP wire fixtures
- [x] endpoint diagnostics that distinguish an artifact, resolved launcher, and live transport
- [x] real single-Studio Vinegar Flatpak southbound MCP launch, discovery, read-only state, disconnect, and reconnect proof
- [ ] real multiple-Studio MCP mapping (blocked: one window is running and Lattice must not open another)
- [x] stable provider/tool/capability identities and content-addressed schema registry
- [x] catalog refresh with schema-change identity retention and unavailable historical tools
- [x] lazy bounded connection broker with deny-by-default policy and immutable large results
- [x] real native external MCP stdio process initialization, discovery, call, and disconnect
- [x] progressive provider/tool CLI and compact northbound MCP discovery
- [ ] Streamable HTTP provider, resources, catalog notifications, durable result/audit integration

## Next: finish first end-to-end slice

1. Complete real multiple-Studio mapping after the user independently opens a second Studio window; never open one for this test.
2. Keep Studio MCP launch resolved through the platform contract and explicitly experimental on Linux/Vinegar.
3. Ingest DataModel/scripts from Studio with adapter keys and alias correlation.
4. Resolve AST require expressions to canonical source references and build cross-source graph edges.
5. Add immutable paginated result resources and deterministic exact/symbol lookup before Tantivy.
6. Add filesystem watching and per-source invalidation; evaluate Salsa only after query keys/invariants are measured.
7. Implement ChangeSet plan/preview/precondition/apply/verify/rollback and audit before exposing mutation MCP tools.
8. Run Studio execution/playtest/console verification against deterministic fixtures.

## Later phases

- rbx-dom place/model ingestion and Rojo project mapping
- diagnostics plus StyLua/Selene signals
- git2 history intelligence
- Cedar authorization and OS-native secret store
- Open Cloud jobs and policy-restricted publication
- versioned UDS/named-pipe IPC feeding the Avalonia workstation in
  `app/Abraxius.Lattice`
- native evaluation runner, benchmarks, Wasmtime extensions
- optional ONNX/tokenizers/USearch semantics after deterministic retrieval

No phase introduces JavaScript, Node, Python services, or model-provider assumptions.
