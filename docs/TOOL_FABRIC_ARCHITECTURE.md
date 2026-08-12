# Universal Tool Fabric architecture

Status: Phase 3 foundation implemented; the completion gate remains open.

## Boundary

The caller selects an exact `ToolRef` or versioned `CapabilityId`. Lattice validates identity, schema, policy, availability, target constraints, concurrency, and transport, then dispatches to the selected provider. Lattice does not plan tasks, infer goals, create hypotheses, or choose an engineering strategy.

`lattice-tools` owns protocol-neutral identities and registries. `lattice-connections` owns lazy lifecycle, bounded calls, deny-by-default policy enforcement, timeouts, cancellation, and immutable result handling. `lattice-mcp` adapts RMCP providers without leaking RMCP types into either domain crate.

```text
caller -> ToolRef/capability -> catalog/router -> policy -> broker -> provider
```

Provider identity is derived from a stable configured/logical key. A connection has a separate ephemeral identity. Tool identity is derived from provider identity plus native tool name, so reconnects and schema revisions do not replace the logical tool.

## Trust and semantics

Provider descriptions and MCP annotations are untrusted provider data. `ReportedSemantics` preserves those hints. `OperationSemantics` contains only locally verified facts. Unknown external tools default to unknown side effects and receive no semantic capability mapping.

Capability mappings come only from built-in definitions, verified manifests, explicit configuration, or trusted extensions. `DeterministicRouter` filters by exact capability version, target, provider availability, and optional explicit provider preference. Multiple remaining implementations return `AMBIGUOUS_CAPABILITY`.

## Progressive discovery

The northbound MCP surface adds only:

```text
lattice.provider.list
lattice.tool.search
lattice.tool.inspect
```

Provider schemas are absent from search results and loaded only by inspection. Provider tools are not projected into northbound `tools/list`. A future opt-in projection profile may expose a bounded configured subset; projection is not authoritative and is not implemented in this slice.

## Current provider truth

The current build registers Lattice Core as healthy. Git and Flight Recorder are registered as unavailable because those adapters do not exist in this repository. Each resolved Studio environment receives a stable provider identity and, when the platform supplies a launch specification, a configured lazy Studio provider. A real single-Studio Vinegar MCP transport has been proven, but provider registration still does not imply a live connection. No capability is fabricated for unavailable providers.

The native external stdio adapter launches an exact absolute executable with an argument vector, cleared environment plus explicit allowlist, optional working directory, piped stdio, continuously drained stderr with bounded accounting, timeouts, and owned child cleanup. It never invokes a shell.

Streamable HTTP, provider resource federation, persistent catalog CRUD, dynamic catalog notifications, Cedar, Flight Recorder events, and Avalonia views remain completion-gate work.
