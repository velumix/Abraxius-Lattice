+++
title = "Lattice Editor"
weight = 9
+++

The native editor is a shared capability, not only an Avalonia feature. Rust
owns ropes, transactions, revisions, incremental syntax, and headless document
state. Avalonia owns the custom-rendered surface and native input. CLI and MCP
use the headless service.

Studio-backed saves flow through revision checks, ChangeSets, the target
adapter, re-read, and hash verification. A dirty local buffer is never
silently overwritten by a Studio or filesystem change.

The editor remains independent from provider brokering, project graph storage,
Studio transport, and Flight Recorder analysis.
