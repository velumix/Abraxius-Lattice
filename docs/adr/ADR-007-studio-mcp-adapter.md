# ADR-007: Studio built-in MCP as adapter

## Context

Roblox Studio now provides built-in MCP tools for scripts, DataModel, execution, playtest, console, viewport, input, assets, and sessions.

## Decision

Consume Studio MCP through `StudioManager`, `StudioSession`, and `StudioMcpAdapter`. Capability-discover every connection and require explicit session targets when ambiguous. Do not recreate supported primitives.

## Alternatives

Embedding Studio behavior in core and immediately shipping a custom Luau plugin were rejected.

## Consequences

Studio is a live sensor/actuator while Lattice remains authority. A tiny custom bridge is permitted only for documented gaps.

