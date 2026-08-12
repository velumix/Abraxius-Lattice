+++
title = "Tool Fabric"
weight = 7
+++

The Tool Fabric turns built-in integrations and external MCP servers into one
catalog without flattening provider identity.

## Provider and connection

A stable `ProviderId` represents a logical source. A transient connection has
its own lifecycle, health, protocol metadata, and catalog revision. Reconnects
do not create a new logical provider.

## Tools and capabilities

A `ToolId` is scoped to its provider. A versioned `CapabilityId` describes
semantic meaning, while explicit bindings connect tools to capabilities. Raw
unmapped tools remain callable by exact `ToolRef`.

Routing uses deterministic target constraints, provider availability, and
configured preference. Zero matches return unavailable; multiple matches
return ambiguity. Lattice never guesses.

## Schemas and results

Schemas are content-addressed with BLAKE3 revisions. Historical descriptors
remain addressable when a schema changes or a tool disappears. Large immutable
results are stored and returned through `lattice://result/...` references.

See the [generated capability](../reference/generated/capabilities/) and
[provider](../reference/generated/providers/) references.
