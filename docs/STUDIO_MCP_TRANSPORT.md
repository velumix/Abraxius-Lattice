# Southbound Studio MCP transport

Status: a real read-only Linux/Vinegar Flatpak connection has passed single-Studio acceptance. Real multiple-Studio acceptance remains blocked because only one Studio window is running and Lattice is forbidden from opening another one.

## Boundary

`lattice-mcp` owns RMCP 3.1.2 and implements the southbound client. `lattice-studio` owns protocol-neutral session, connection, tool-catalog, and result models. RMCP types do not cross into Lattice Core or the Studio domain crate.

Every accepted target is bound to a `StudioSessionId`, `StudioEnvironmentId`, and Studio PID. `StudioManager::bind_mcp_connection` rejects mismatched environment/process identities. Roblox's Studio MCP child can enumerate more than one Studio window, so a live catalog result is not automatically treated as one provider per window. The current proof accepts exactly one enumerated Studio and returns `AMBIGUOUS_STUDIO_SESSION` otherwise; explicit multi-window selection/mapping remains behind the blocked real acceptance test.

The connection lifecycle is explicit: `Unavailable`, `Discovering`, `Available`, `Connecting`, `Initializing`, `Connected`, `Degraded`, `Reconnecting`, `Disconnected`, and `Failed`. Only `Connected` and `Degraded` accept requests. Snapshots record negotiated protocol profile, negotiation path, session model, fallback evidence, server identity and capabilities, a BLAKE3 tool-catalog revision, endpoint identity, RTT, failures, and request timestamps.

## Attach and launch policy

`StudioMcpClient` accepts an already-open RMCP transport. `ResolvedStudioMcpProcessLauncher` is the separate process boundary: it consumes only the current `StudioEnvironment` launch specification, starts only the resolved Studio MCP child, owns its stdio and bounded stderr, and reaps it on disconnect. It never starts or restarts Roblox Studio and never searches for platform paths.

Native Windows/macOS launcher discovery remains in `lattice-platform`. On Linux/Vinegar, Roblox does not document a launch command, so support is explicitly experimental. The platform resolver correlates the already-running Studio process with its active prefix, deployment, Wine drive mappings, and actual Vinegar-managed Wine runtime. Only when all required evidence is available does it return a shell-free launch specification for the current deployment's `StudioMCP.exe`. Consumers do not reconstruct those paths or use host `/usr/bin/wine` by assumption.

Run the read-only diagnostic repeatedly with:

```text
lattice studio mcp
```

This default command performs no launch. The explicit proof command is:

```text
lattice studio mcp --connect
```

It launches only the resolved Studio MCP child, calls `list_roblox_studios`, requires an unambiguous Studio result, calls `get_studio_state` with that Studio's explicit identifier, then disconnects and reaps the child. Its output always reports `studio_launched: false`.

## Protocol compatibility

Every connection first uses Lattice's protocol compatibility profile. Modern MCP uses `server/discover`, per-request metadata, stateless operation, and no protocol session. General legacy fallback occurs only after JSON-RPC `METHOD_NOT_FOUND`; malformed discovery is a hard failure.

The Studio MCP build verified on the current Vinegar host closes its first child instead of returning `METHOD_NOT_FOUND`, while its bounded stderr explicitly reports that it expected `initialize` and received `server/discover`. The trusted Studio launcher alone recognizes that exact two-part evidence, reaps the first child, relaunches once with the isolated legacy profile, and records the reason. It does not generalize connection closure into a downgrade rule. See [MCP_PROTOCOL_COMPATIBILITY.md](MCP_PROTOCOL_COMPATIBILITY.md).

## Safety and capability negotiation

The client retrieves the server's actual negotiated state and complete paginated tool catalog. A tool must be advertised before it can be called. Tool annotations are retained as untrusted hints; mutation policy must not rely on `readOnlyHint` alone. Tool requests require JSON-object arguments and use a configured timeout. Disconnect closes only the Lattice-owned child transport and never terminates Studio.

## Acceptance status

Native raw-wire fixtures prove modern `server/discover` without initialization, controlled legacy initialization fallback, per-request modern metadata, tool discovery/call, and rejection of malformed downgrade attempts.

The real Vinegar acceptance established a stdio connection to the already-running Studio, discovered the actual 25-tool catalog, returned the real `Sun City RP` Studio identity, returned its Edit state, associated it with the resolved `StudioEnvironmentId` and process, disconnected cleanly, and left Studio running. A second independent run reconnected successfully. The launch remains experimental because Roblox does not document Linux/Vinegar support.

The multiple-window acceptance item is still `BLOCKED`: only one Studio window is available, and Lattice will not open another. Downstream blocker work must not treat that unperformed test as passed.
