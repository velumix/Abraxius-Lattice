# Lattice `rbx://` resource reference specification

Version: 0.1.0.

## Principle

A reference identifies a Lattice entity, never its current display path. Rename and move operations update aliases and metadata while preserving the entity ID whenever the backing adapter can correlate identity safely.

## Grammar

```text
rbx-ref       = workspace-ref / studio-ref / cloud-ref / test-ref
workspace-ref = "rbx://workspace/" lattice-id "/" kind "/" lattice-id
studio-ref    = "rbx://studio/" lattice-id "/" kind "/" lattice-id
cloud-ref     = "rbx://cloud/" uint "/" uint "/version/" uint "/" kind "/" lattice-id
test-ref      = "rbx://test/" lattice-id "/" kind "/" lattice-id
lattice-id    = canonical UUID string
uint          = one or more decimal ASCII digits
kind          = lower-case stable token with no slash
```

Examples:

```text
rbx://workspace/019aaaaaaaaa-.../script/019bbbbbbbbb-...
rbx://studio/019aaaaaaaaa-.../instance/019bbbbbbbbb-...
rbx://cloud/100/200/version/42/script/019bbbbbbbbb-...
rbx://test/019aaaaaaaaa-.../result/019bbbbbbbbb-...
```

The implementation uses canonical hyphenated UUIDv7 identifiers. The abbreviated examples above are illustrative only and will not parse.

## Lifecycle

- A workspace ID persists in its local metadata store.
- An entity ID persists across content revisions, renames, and moves when correlation is certain.
- A display path is an alias. Historical aliases may be retained with revision bounds.
- Deletion tombstones the reference; it must not be reassigned.
- Cache deletion removes Lattice's local correlation history but never user source. Re-ingestion may therefore create new IDs unless an adapter provides an external stable key.
- Cloud version is part of the authority scope. Cross-version entity correlation is metadata, not URI equivalence.
- Result/test references are immutable and may expire. Resolution then returns `RESULT_EXPIRED` rather than another object.

## Security

Parsers reject empty segments, path syntax, traversal segments, malformed numbers, and non-UUID entity IDs. Resolution always checks the referenced authority and the caller's policy; possession of a reference is not authorization.

