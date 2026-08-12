# Vinegar support

Lattice supports native/XDG Vinegar and the `org.vinegarhq.Vinegar` Flatpak as distinct runtimes. AUR, Nix, source, and other native packages are covered when they follow the XDG layout or use validated overrides.

## Discovery

The resolver:

1. snapshots processes and their parent, executable, command, environment, and start-time evidence;
2. probes the Flatpak application root;
3. probes native XDG config, data, and cache roots;
4. validates candidate `prefixes/studio` and `versions` structures;
5. correlates the Studio executable and `WINEPREFIX` with candidates;
6. discovers Wine users and Roblox data using process and filesystem evidence;
7. reads `dosdevices` drive mappings;
8. resolves the actual Vinegar-managed Wine runtime from correlated process evidence;
9. returns one environment per correlated Studio main process or an explicit ambiguity.

Typical native roots are `$XDG_CONFIG_HOME/vinegar`, `$XDG_DATA_HOME/vinegar`, and `$XDG_CACHE_HOME/vinegar`, using XDG defaults only when variables are unset. A typical Flatpak root is `$HOME/.var/app/org.vinegarhq.Vinegar`, with `config/vinegar`, `data/vinegar`, and `cache/vinegar` beneath it. These are examples, not guaranteed constants; every path is probed.

Recent Vinegar versions may redirect Roblox user data to a host-side `appdata/Roblox` tree rather than using only the prefix's `drive_c/users/<wine-user>/AppData/Local/Roblox`. Lattice selects the redirect only when active process arguments prove it, preserves its discovered Wine drive representation, and otherwise falls back to Wine-profile discovery.

## Flatpak considerations

Flatpak Studio access and Lattice host access are separate facts. A path visible through Wine `Z:` may still be unavailable to Studio because of Flatpak filesystem permissions. Conversely, a path visible inside Studio may be denied to a sandboxed Lattice process. Resolver output distinguishes missing, permission-denied, and sandbox-denied states.

Roblox currently documents Studio MCP launcher commands for Windows and macOS, not Linux/Vinegar. Linux support is therefore marked experimental. When the resolver proves the active Wine runtime, prefix, deployment, `StudioMCP.exe`, and guest mapping, it exposes a shell-free `StudioMcpLauncher`. An explicit `lattice studio mcp --connect` request may start that child through the resolved Vinegar runtime; it never starts Studio. If the evidence is incomplete, launch remains unavailable rather than falling back to guessed paths or host Wine.

## Troubleshooting

Run these read-only commands:

```text
lattice studio environment
lattice studio environment --verbose
lattice studio list
lattice studio mcp
```

The first three commands are read-only diagnostics. `lattice studio mcp --connect` is an explicit read-only live proof that launches and owns only the Studio MCP child. Look for `VINEGAR_FLATPAK_ROOT_DETECTED`, `VINEGAR_XDG_ROOT_DETECTED`, `STUDIO_PROCESS_CORRELATED`, `WINE_USER_RESOLVED`, or explicit missing/ambiguous/access states. Overrides are documented in [STUDIO_ENVIRONMENT.md](STUDIO_ENVIRONMENT.md).
