# ADR-006: Isolated MCP boundary

## Context

Lattice is both a northbound MCP server and a southbound Studio MCP client, while the Rust SDK and protocol can evolve.

## Decision

Own protocol-neutral `LatticeOperations`, `LatticeProtocolServer`, and `LatticeProtocolClient` contracts. Exact-pin RMCP 3.1.2 inside `lattice-mcp`; no core crate accepts or returns RMCP types.

## Alternatives

Using RMCP models as the domain model or exposing one tool per Roblox primitive were rejected.

## Consequences

SDK upgrades remain localized. DTO mapping is explicit work but prevents protocol churn from contaminating project intelligence.

