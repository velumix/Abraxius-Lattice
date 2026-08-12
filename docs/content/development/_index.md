+++
title = "Development"
weight = 12
+++

## Verification

Run the locked workspace checks before changing architecture:

```sh
cargo fmt --all -- --check
cargo check --locked --workspace
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Documentation changes use the same source-of-truth discipline:

```sh
cargo run --locked -p lattice-docgen -- check
cargo run --locked -p lattice-docgen
```

Then run `zola check` and `zola build` from `docs/`.

## Contribution boundaries

Production code remains Rust, C, C++, C#, and Luau where Roblox requires it.
Do not add Node, npm, browser backends, or an editor-specific integration to
solve a Lattice platform problem.
