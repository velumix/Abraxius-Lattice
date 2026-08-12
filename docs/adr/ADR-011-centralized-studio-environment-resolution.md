# ADR-011: Centralized Studio environment resolution

## Context

Roblox Studio runs natively on Windows and macOS and through Vinegar/Wine on Linux. Flatpak adds another access boundary. Flight Recorder telemetry must correlate logs, deployments, prefixes, paths, and multiple processes without embedding these layouts throughout the codebase.

## Decision

All OS/runtime-specific Roblox Studio path, installation, process, and environment discovery belongs behind `lattice-platform`. Consumers use typed environment IDs, path namespaces, semantic roles, capability states, and translation services. `lattice-studio` associates MCP sessions with resolved environment IDs but does not construct paths.

## Alternatives

- Per-consumer `cfg(target_os)` logic was rejected because assumptions would diverge.
- String paths were rejected because Wine guest paths could reach host filesystem APIs.
- One global Studio root was rejected because users can run multiple deployments, prefixes, or sessions.
- Always choosing the newest directory was rejected because active process/session evidence is stronger.

## Consequences

Phase 2 can ask semantic questions without knowing whether Studio is native or Wine-hosted. Detection is independently fixture-testable. Unknown, missing, denied, and ambiguous states remain explicit. Vinegar layout changes are isolated to one resolver. The daemon must sanitize personal path components before exporting future traces.
