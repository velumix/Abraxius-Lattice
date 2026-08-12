+++
title = "Getting Started"
weight = 1
+++

Start with a local native build, then connect Lattice to a running Roblox
Studio session. The desktop application, CLI, Studio companion, and MCP
clients all use the same daemon-owned state.

## Build the repository

```sh
cargo build --locked --workspace
```

The current development toolchain is pinned in `rust-toolchain.toml`.

## First useful checks

```sh
lattice workspace status ./my-game
lattice studio environment --verbose
lattice provider list
lattice tool search "execute luau"
```

Continue with [Studio](../studio/), [CLI](../cli/), or [Codex MCP](../mcp/codex/).
