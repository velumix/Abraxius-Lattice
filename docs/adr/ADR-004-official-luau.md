# ADR-004: Official Luau C++ integration

## Context

Regex and embeddings cannot authoritatively represent Luau syntax or scope.

## Decision

Pin official Luau 0.733 at commit `ca128af4c531310d6f5c1b354df4b79fdd782ede`. Compile the AST library in `lattice-luau-sys`; cross CXX only with owned symbols, references, requires, spans, and diagnostics.

## Alternatives

Regex extraction, a third-party Rust parser, invoking a Python service, or leaking Luau AST pointers into core were rejected.

## Consequences

Current facts use the real parser. Analyzer/type-checker integration and upstream compatibility tests must expand carefully when the pin changes.

