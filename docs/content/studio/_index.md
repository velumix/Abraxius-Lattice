+++
title = "Roblox Studio"
weight = 4
+++

Lattice resolves a `StudioEnvironment`, correlates its Studio process, and
binds live MCP connections to a `StudioSession`. A detected `StudioMCP.exe`
binary is not itself proof of a live connection.

## Live connection states

Connections expose explicit states such as `Discovering`, `Connecting`,
`Initializing`, `Connected`, `Degraded`, `Reconnecting`, and `Disconnected`.
There is no single boolean that represents Studio health.

## Read-only proof

```sh
lattice studio environment --verbose
lattice studio mcp --connect
```

The proof must receive a real Studio response and correlate it to the bound
session. It must not claim success from process discovery alone.

See the [Studio MCP transport](../mcp/studio-transport/) and
[platform guide](../platform/).
