# ADR-008: `rbx://` stable resources

## Context

Roblox paths change on rename and move; models need cheap reusable handles.

## Decision

Use canonical `rbx://` references with UUIDv7 Lattice entity IDs. Paths are revisioned aliases, never primary keys. Resolution is central and policy-aware.

## Alternatives

DataModel paths, filesystem paths, raw adapter IDs, and content hashes as sole identity were rejected.

## Consequences

Renames can preserve identity and model context stays compact. Adapter correlation and cache-rebuild semantics must be honest.

