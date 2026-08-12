# Lattice ChangeSet specification

Version: design 0.1; mutation is intentionally not enabled in the bootstrap.

## State machine

```text
Proposed -> Validated -> Previewed -> Authorized -> Applying -> Applied -> Verified
     \          \            \             \           \
      Rejected   Conflict     Rejected       Conflict    Failed -> RolledBack
```

Transitions are append-only audit facts. Replaying an already consumed authorization or applying from an unexpected state is invalid.

## Operation

Each ordered operation contains an ID, kind, canonical target reference, expected revision, expected BLAKE3 hash when content-backed, typed transformation payload, resulting hash after validation, evidence references, risk, and reversibility declaration.

Initial kinds are `EditSource`, `CreateSource`, `DeleteSource`, `MoveInstance`, `SetProperty`, `CreateInstance`, `DeleteInstance`, `AddTest`, and `ModifyConfiguration`.

## Preconditions and apply

1. Resolve the canonical target in an explicitly selected adapter/session.
2. Re-read current state from the authoritative target.
3. Compare expected revision/hash. Any mismatch returns `REVISION_CONFLICT` with current metadata.
4. Evaluate granular policy for principal/action/resource.
5. Record authorization and the pre-apply rollback object.
6. Apply exactly once with an idempotency/replay guard.
7. Re-read and compare the resulting hash/state.
8. Run declared validation and record evidence.

No LLM response directly enters project state.

## Preview

Preview contains affected references, bounded inline diff or immutable result URI, risk, preconditions, predicted diagnostic changes, recommended tests, and an honest rollback strategy. An irreversible adapter operation says so before authorization.

## Rollback

Source rollback is a new guarded operation using the verified post-apply hash as its precondition and the prior immutable object as its desired content. Studio-native undo may supplement this journal but cannot replace it unless its target/session and history position are verified.

