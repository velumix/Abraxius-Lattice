# Studio environment contract

`StudioEnvironment` is the Phase 1 contract consumed by the future Flight Recorder. Phase 2 must not contain platform or Vinegar path constants.

Important fields include:

- stable `StudioEnvironmentId` and `resolver_version`;
- `HostPlatform` and `StudioRuntime`;
- the correlated main `StudioProcessInfo` plus related process telemetry;
- a resolved `WineRuntime` derived from active process evidence where applicable;
- `StudioDeployment` and its stable deployment ID;
- semantic config, data, cache, prefix, AppData, logs, profiler, crash, export, and MCP-server roles;
- detected Wine drive mappings and host/guest paths;
- explicit capabilities and structured resolution diagnostics.

Callers use:

```rust
environment.path(StudioPathRole::Logs)
environment.path(StudioPathRole::Profiler)
environment.path(StudioPathRole::RobloxAppData)
environment.path(StudioPathRole::Deployment)
```

Each result reports availability and whether it was detected or configured. A missing path may retain the safely probed candidate for troubleshooting, while top-level resolved fields remain `None` unless available.

## Multiple instances

One installation/prefix may produce several environments when several main Studio processes run. Their IDs include process start evidence. `StudioManager` requires explicit session targeting when more than one session is connected and records `environment_id` and `process_id` on each `StudioSession`.

## Overrides

Automatic discovery is the default. Advanced overrides are:

```text
LATTICE_VINEGAR_DATA_ROOT
LATTICE_STUDIO_PREFIX
LATTICE_STUDIO_DEPLOYMENT
LATTICE_ROBLOX_APPDATA
LATTICE_STUDIO_MCP_COMMAND
```

Filesystem overrides must exist and be directories. The MCP command must be an existing file. Invalid overrides return structured errors; configured values are marked `configured`.

## Diagnostic command

```text
lattice studio environment
lattice studio environment --verbose
```

The default report is compact. `--verbose` includes every role state, resolver diagnostic, and related process. Both commands are read-only and do not start Studio or mutate a project.

The platform layer distinguishes an unlaunchable Linux `StudioMCP.exe` artifact from an evidence-complete experimental launcher. The latter contains the resolved Vinegar-managed Wine executable, prefix, current deployment target, and guest path. Execution is never automatic: only an explicit Studio MCP connection request consumes the launch specification. Windows and macOS use Roblox's documented native launch locations.
