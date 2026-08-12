# ADR-002: C# Avalonia control surface

## Context

The workstation must be native and cross-platform without turning the UI into the engine.

## Decision

Use C#/Avalonia for the future desktop application. It communicates with `lattice-daemon` through versioned native IPC and owns no authoritative project state.

## Alternatives

Electron/browser UI was rejected by the native language rule. A Rust GUI was considered but Avalonia provides the selected desktop control framework.

## Consequences

The daemon survives UI failure and CI remains headless. The Avalonia shell is
implemented in `app/Abraxius.Lattice`; its daemon state source remains behind
the versioned IPC boundary until that transport is implemented.
