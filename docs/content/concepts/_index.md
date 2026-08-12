+++
title = "Concepts"
weight = 2
+++

## Canonical references

Lattice addresses project resources with stable `rbx://` references and
runtime history with `trace://` references. Clients exchange these references
instead of filesystem paths, Wine paths, or process-specific identifiers.

## Structure and history

Phase 1 models the project and Studio environment. Phase 2 records execution
history, telemetry, and evidence. Phase 3 makes providers, tools, resources,
and capabilities discoverable through one deterministic fabric.

## Caller, Lattice, provider

The caller chooses what should happen. Lattice resolves the requested target,
validates inputs and policy, and routes deterministically. A provider performs
the operation. Lattice does not plan or make engineering decisions.
