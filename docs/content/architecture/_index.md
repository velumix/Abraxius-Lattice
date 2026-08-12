+++
title = "Architecture"
weight = 3
+++

Lattice is a native Rust core with Avalonia presentation and a lightweight
Roblox Studio companion. The daemon owns canonical state and exposes the same
capabilities to the desktop, CLI, MCP clients, and future applications.

```text
Client (Avalonia / CLI / MCP)
             |
             v
       lattice-daemon
       /      |      \
   Graph   Tool Fabric  Studio
             |           |
          Providers    Studio MCP
```

## Core boundaries

- `lattice-platform` resolves host/runtime environments and semantic paths.
- `lattice-studio` binds Studio sessions to resolved environments.
- `lattice-tools` owns provider, tool, schema, capability, and routing models.
- `lattice-mcp` is the native protocol boundary; SDK types do not escape it.
- `lattice-storage` persists authoritative state and migrations.
- Avalonia renders state; it does not rediscover Studio or provider topology.

The repository retains detailed architecture records in the source `docs/`
directory and ADRs.
