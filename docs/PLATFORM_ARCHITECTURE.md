# Lattice platform architecture

Status: resolver version 1, implemented 2026-08-11.

`lattice-platform` is the sole authority for host, Studio runtime, installation, deployment, process, filesystem-root, Wine-prefix, and path-namespace resolution. Consumers request semantic `StudioPathRole` values; they do not assemble operating-system or Vinegar paths.

```text
CLI / Studio adapter / future recorder / daemon IPC
                         |
                 StudioEnvironment
                         |
          +--------------+---------------+
          |              |               |
   native Windows   native macOS   Linux/Vinegar
                                      |       |
                                     XDG   Flatpak
                                      \       /
                                  Wine mappings
```

## Contracts

- `HostPlatform` represents Windows, Linux, macOS, or an explicit unsupported host.
- `StudioRuntime` distinguishes native Roblox, native/XDG Vinegar, Flatpak Vinegar, and unknown runtimes.
- `StudioEnvironmentId` fingerprints resolver version, runtime, installation root, PID, and process start time. PID alone is never identity.
- `HostPath` and `WinePath` prevent guest paths from reaching host filesystem APIs accidentally.
- `StudioEnvironment::path(role)` returns an availability state, origin, and host/guest representations.
- `StudioEnvironmentCapabilities` lets downstream systems avoid ad hoc probing.
- `StudioSession.environment_id` associates each MCP session with one resolved process environment.

`PlatformContext`, `PlatformFileSystem`, and `ProcessSource` are injectable. Production uses `dirs`, native environment discovery, the real filesystem, and `sysinfo`; tests use temporary roots and static process snapshots. Process environment capture is allowlisted to Wine-prefix/user and Flatpak correlation keys so unrelated credentials never enter resolver snapshots.

## Detection and correlation

Linux detection evaluates running-process evidence, Flatpak roots, native XDG roots, prefixes, user profiles, and deployments. Existing roots do not win merely because they were visited first. A process is classified as Studio only when its executable command is Studio itself; WebView, Wine, crash, and launcher processes remain related telemetry. Multiple unresolved candidates or Studio processes produce `ambiguous` rather than a global current path.

Deployment selection prefers an exact running-process correlation, then a single valid deployment, then a unique latest activity timestamp. Ties remain ambiguous. Wine user selection prefers process-reported Wine identity, then exactly one profile containing Roblox AppData. It never assumes the host username.

## Access and security

Path states distinguish `available`, `missing`, `permission_denied`, `sandbox_denied`, `unavailable`, and `ambiguous`. Lexical normalization does not require existence. Filesystem canonicalization is separate and used for symlink-escape checks.

Wine paths reject UNC input, malformed drives, NUL/colon components, and parent traversal. Guest-to-host translation validates the configured drive mapping and the deepest existing ancestor against symlink escape. `Z:` is usable only when the active prefix exposes a discoverable `dosdevices/z:` mapping.

Absolute paths are evidence, not canonical resource identity. Exported diagnostics and future trace manifests must redact user-specific components while retaining environment ID, runtime, role, relative path, and resolver version.

See [STUDIO_ENVIRONMENT.md](STUDIO_ENVIRONMENT.md), [VINEGAR_SUPPORT.md](VINEGAR_SUPPORT.md), and [PATH_NAMESPACES.md](PATH_NAMESPACES.md).
