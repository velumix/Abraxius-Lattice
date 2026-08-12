# Phase 3 blocker resolution status

Status date: 2026-08-11

## Blocker 1 — MCP protocol compatibility: closed

Implemented and verified:

- Lattice-owned modern and legacy profiles;
- modern `server/discover`, stateless requests, per-request metadata, and no retained protocol session;
- legacy initialization isolated behind controlled fallback;
- provider inspection metadata for revision, negotiation, session, catalog-change model, and fallback reason;
- native raw-wire modern, legacy, and malformed-discovery fixtures;
- malformed discovery cannot silently downgrade.

## Blocker 2 — real Studio MCP on Linux/Vinegar: blocked at remaining acceptance

Attempted and proven against the real already-running Studio:

- resolved the active Vinegar Flatpak environment, prefix, deployment, Wine drive mappings, and Vinegar-managed Wine runtime through `lattice-platform`;
- launched only the current deployment's `StudioMCP.exe` child, directly and without a shell;
- connected its stdio transport;
- observed explicit legacy negotiation evidence and recorded the compatibility fallback;
- discovered the real 25-tool catalog;
- called `list_roblox_studios` and observed the running `Sun City RP` instance;
- called `get_studio_state` with its explicit external Studio identifier and observed Edit mode;
- associated the proof with the resolved `StudioEnvironmentId` and Studio process;
- disconnected/reaped the MCP child and left Studio running;
- repeated the proof successfully to verify reconnect.

Remaining unverified conditions:

- real multiple-Studio enumeration, explicit selection, and environment mapping;
- live failure cases that require intentionally disabling, closing, crashing, or replacing the current Studio/MCP/Wine process.

Why blocked:

- only one Studio window is running;
- the user explicitly prohibited Lattice from opening another Studio instance;
- destructive/disruptive failure tests are not authorized against the user's current live Studio.

Capability state:

- single-Studio experimental Vinegar stdio launch and read-only access: available when all platform evidence resolves;
- automatic multi-Studio targeting: unavailable/ambiguous until explicit real mapping is verified;
- launch remains unavailable when Wine runtime, prefix, deployment, MCP target, or guest mapping evidence is absent.

## Later blockers: not started

Streamable HTTP and later blockers were intentionally not started because the mandated completion order requires Blocker 2 to close first.
