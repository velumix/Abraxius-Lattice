# ADR-009: All mutation through ChangeSets

## Context

Direct LLM source replacement cannot safely handle concurrent edits, policy, review, rollback, or verification.

## Decision

Every mutation becomes a typed ChangeSet with state transitions, canonical targets, expected revision/hash, preview, authorization, audit, apply verification, and honest rollback data.

## Alternatives

Direct editor tools, best-effort overwrite, Git-only rollback, and host-only confirmation were rejected.

## Consequences

Mutation ships later than read-only intelligence but becomes deterministic, observable, conflict-safe, and provider-independent.

