# Lattice documentation

The site is built with [Zola](https://www.getzola.org/) and the native Rust
`lattice-docgen` crate. The build has no Node, npm, JavaScript framework, or
runtime service dependency.

## Local build

Install the pinned Zola release used by CI, then run:

```sh
cargo run --locked -p lattice-docgen
cd docs
zola check
zola build
zola serve
```

Generated reference pages live under `content/reference/generated/` and are
committed. When a CLI, MCP tool, capability, provider, or error contract
changes, regenerate and review those files with the code change.

```sh
cargo run --locked -p lattice-docgen -- check
```

The GitHub Actions workflow runs the same generator and validation commands.
