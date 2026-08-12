# ADR-012: Studio-initiated companion bridge

## Context

Studio MCP provides official request/response primitives but does not provide
all continuous editor events and telemetry needed by later runtime recording.
Linux/Vinegar also makes host-initiated compatibility-environment transports
more fragile. Abraxius demonstrated that a small Studio plugin can instead
register outward to a loopback native service.

## Decision

Implement an optional Luau companion that initiates automatically paired loopback HTTP
to `lattice-studio-bridge`, a native Rust/Axum subsystem. Keep it separate from
Studio MCP. Require explicit host-side binding from every bridge session to the
existing Studio session, environment, and process identities.

Use bounded per-session command/event storage, sequenced replay-safe reports,
command ID deduplication, loopback-only binding, request-size limits, and bearer
authentication. Initially permit read-only commands. All future mutations must
enter the existing ChangeSet and policy pipeline.

## Alternatives

- Reuse the Abraxius Node server: rejected because Lattice production code may
  not depend on Node or JavaScript.
- Replace Studio MCP with the plugin: rejected because Roblox's built-in MCP is
  the official implementation for its advertised Studio capabilities.
- Attach to an inferred localhost port: rejected because the companion is not
  MCP and Studio MCP is stdio.
- Load a native library into Studio: rejected because Studio requires Luau for
  plugins and in-process native code would create an unsafe extension boundary.

## Consequences

The same plugin wire behavior works across native Windows, native macOS, and
Linux/Vinegar without duplicating platform path logic. The plugin remains small
and intentionally lacks business logic. Users must explicitly install it,
enable Studio HTTP requests, and pair a local token. Live availability cannot be
claimed until authenticated registration occurs.
