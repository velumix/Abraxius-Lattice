+++
title = "MCP"
weight = 6
+++

Lattice is both a northbound MCP server and, where required, a southbound MCP
client. The northbound surface stays deliberately small: clients search and
inspect the exact tool or capability they need instead of receiving every
provider schema at startup.

## Local clients

For Codex and other local MCP clients:

```sh
lattice mcp status
lattice integration codex install
codex mcp list
```

The preferred local transport is `lattice mcp stdio`, a thin bridge to the
already-running daemon. It does not create a second Lattice core or write
protocol diagnostics to stdout.

## Protocol profiles

Modern and legacy MCP lifecycles are isolated behind a compatibility layer.
The negotiated protocol profile, transport, session model, and catalog-change
model are observable provider state.

- [Codex integration](codex/)
- [Studio transport](studio-transport/)
- [Generated MCP tool reference](../reference/generated/mcp/)
