# ADR-005: rbx-dom place-file boundary

## Context

Offline `.rbxl`, `.rbxlx`, `.rbxm`, and `.rbxmx` ingestion must not require Studio.

## Decision

Use rbx-dom behind a future `PlaceFileAdapter`, mapping its types into Lattice-owned canonical entities. Do not expose rbx-dom types to core consumers.

## Alternatives

Custom format parsing and Studio-only ingestion were rejected.

## Consequences

Adoption is deferred until the adapter slice so unused transitive dependencies are not accepted early. Binary/XML hostile-input tests are mandatory.

