# Lattice MCP surface

Protocol target: MCP 2026-07-28. SDK pin: official Rust SDK `rmcp` 3.1.2.

The current specification is stateless and self-describing, with per-request capabilities and optional discovery. Tasks are an opt-in extension; roots, sampling, logging, and legacy HTTP+SSE are deprecated for new architecture. Lattice will capability-negotiate extensions and preserve a core path without them.

## Implemented tools

| Tool | Input | Output |
|---|---|---|
| `lattice.workspace.status` | none | canonical workspace ID, revision, source count, graph count |
| `lattice.search` | `{query, limit?}` | at most 50 compact hits with `rbx://` reference, hash, score, evidence ID |
| `lattice.capabilities` | none | protocol adapter and enabled semantic operations |
| `lattice.provider.list` | none | compact configured provider health; performs no connection |
| `lattice.tool.search` | `{query, limit?}` | compact tool references without full schemas |
| `lattice.tool.inspect` | `{tool_ref}` | one exact schema revision, trust boundary, and semantics |

The stdio server writes protocol bytes only to stdout; tracing goes to stderr. Filesystem/database/search work runs on Tokio's blocking pool. The core accepts no RMCP types.

## Planned stable semantic surface

```text
lattice.workspace.open       lattice.workspace.status
lattice.search               lattice.inspect
lattice.context              lattice.graph
lattice.diagnose             lattice.change.plan
lattice.change.preview       lattice.change.apply
lattice.change.rollback      lattice.execute
lattice.test                 lattice.logs
lattice.publish              lattice.capabilities
```

Provider execution remains absent northbound until persistent audit and production policy are implemented. The broker itself is deny-by-default. Mutation tools remain absent until ChangeSet preconditions, policy, audit, verification, and rollback are implemented.

## Large results

Broker responses over 64 KiB are stored immutably and returned as `lattice://result/<result-id>` metadata. The current broker store is process-local; durable object-store integration remains required before northbound tool execution is enabled.

## Studio southbound adapter

Roblox's documented built-in Studio MCP launcher is stdio on Windows/macOS. Its current tools include session selection, scripts, DataModel inspection, Luau execution, playtest, console, screenshot, player input, assets, and documentation retrieval. Lattice will negotiate actual tools and map them to `RobloxCapability`; it will never infer mutation support merely from a Studio version.

Research note: Roblox's archived Rust server showed a useful separation between external stdio and a loopback Studio bridge. Lattice does not copy its unbounded MPSC queue, indefinite response wait, or single fixed-port assumptions.
