+++
title = "Codex MCP Integration"
weight = 1
+++

Configure once, then run Codex from any editor terminal:

```sh
lattice integration codex install
lattice integration codex status
codex mcp list
```

Codex launches `lattice mcp stdio`; the bridge connects to the local daemon
through Lattice's platform-independent IPC. It does not depend on VS Code,
Zed, Neovim, JetBrains, or the Lattice desktop being the active editor.

The daemon remains the policy authority. Codex approval settings do not grant
permissions that Lattice denies.

See the repository's detailed [Codex MCP guide](https://github.com/abraxius/lattice/blob/main/docs/CODEX_MCP.md).
