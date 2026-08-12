# Codex CLI and Lattice MCP

Lattice is available to Codex CLI from any terminal, including terminals
embedded in Zed, VS Code, Neovim, JetBrains, or another editor. No editor
extension is required.

## Install

With the `lattice` executable available, run:

```text
lattice integration codex install
```

The command verifies that `codex` is on `PATH`, then asks Codex to register a
global stdio server named `lattice`. It uses Codex's own `codex mcp add`
command and does not rewrite `~/.codex/config.toml` or touch unrelated servers.

Verify the registration with:

```text
lattice integration codex status
codex mcp list
```

The manual equivalent is:

```text
codex mcp add lattice -- lattice mcp stdio
```

When `lattice` is not on `PATH`, use the absolute path to the installed
executable in that command. Lattice's installer automatically uses the
absolute executable that launched the installer, so the configuration works
from any current working directory.

## Transport and ownership

Codex launches:

```text
lattice mcp stdio
```

The process is a thin stdio MCP adapter. It forwards MCP bytes to the
authoritative `lattice-daemon` over Lattice's local daemon IPC endpoint. It
does not index a project, open Studio, create a second provider registry, or
create a second Lattice core. The daemon remains alive after Codex exits.

The adapter writes no banners or diagnostics to stdout. stdout is reserved for
MCP protocol traffic; diagnostics belong on stderr.

## Use from any editor

Start Codex from the editor's terminal as usual:

```text
codex
```

Then ask Codex to use Lattice for project search, Studio inspection, resource
resolution, or tool discovery. The editor hosting the terminal is not part of
the integration and does not need a plugin.

Codex's working directory remains Codex's working directory. It is not
silently registered as a Lattice workspace. Workspace registration and
provider policy remain Lattice operations.

## Status and removal

```text
lattice mcp status
lattice integration codex status
lattice integration codex remove
```

`remove` asks Codex to remove only the server named `lattice`. It does not
modify other MCP servers.

## Security

The stdio adapter does not require API keys or forwarded secrets. The installer
may set the non-secret `LATTICE_RUNTIME_DIR` value so Codex's MCP subprocess can
find the daemon when Codex sanitizes desktop runtime variables. Provider
credentials remain owned by Lattice. Every operation still passes through
Lattice's policy and audit boundaries; Codex's local approval settings do not
grant an operation that Lattice denies.

## Acceptance checklist

With the desktop/daemon running, verify the registration and transport from an
unrelated directory:

```text
cd /tmp
codex mcp list
lattice mcp status
```

Start `codex` from that directory and use `/mcp` to confirm the live `lattice`
server. The bridge process is tied to that Codex process; when Codex exits, the
bridge exits while the daemon remains available for other clients.
