# ADR-010: Cedar authorization boundary

## Context

Read, source mutation, execution, DataStore, Git, extension, and publish rights require granular deny-by-default policy independent of an LLM host.

## Decision

Adopt Cedar in the authorization phase behind a Lattice policy interface. User profiles expand to granular principal/action/resource decisions; the database stores decisions and secret identifiers, never secrets.

## Alternatives

Hard-coded roles, prompt-based safety, and relying solely on MCP elicitation were rejected.

## Consequences

Cedar is not yet a dependency because mutation is not exposed. Policy fixtures and decision-audit compatibility become release gates when adopted.

