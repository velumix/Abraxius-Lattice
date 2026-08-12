# ADR-001: Rust core

## Context

Lattice needs predictable memory use, native deployment, strong types, asynchronous I/O, safe concurrency, and C++ interoperability.

## Decision

Implement authoritative services in a Rust workspace pinned to Rust 1.97.1. Luau remains C++ behind CXX; Roblox-required bridge/test code may be Luau.

## Alternatives

C# core, C++ core, JavaScript/Node service, and Python service were rejected. C# remains appropriate for Avalonia UI only.

## Consequences

Core can run headlessly and share logic across CLI, daemon, IPC, and MCP. Native FFI and ecosystem dependency review remain explicit engineering costs.

