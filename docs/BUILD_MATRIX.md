# Native build matrix

The repository's canonical quality commands are:

```text
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

| Platform | Rust | Native compiler | Additional acceptance |
|---|---|---|---|
| Ubuntu 24.04 x86_64 | 1.97.1 | GCC/Clang C++20 | SQLite bundled, Luau bridge, CLI/daemon/MCP tests, XDG/Flatpak fixtures, optional read-only live Vinegar diagnostic |
| Windows x86_64 | 1.97.1 | MSVC C++20 | native Studio environment/process/known-folder fixture, launcher discovery, named-pipe IPC later |
| macOS arm64/x86_64 | 1.97.1 | Apple Clang C++20 | native app/data/log/process fixture, launcher discovery, UDS IPC later |

Only the Linux target is installed on the initial development machine. Windows and macOS resolver behavior is fixture-tested without pretending that this replaces native-target build acceptance.

Sanitizer builds for the CXX boundary and cargo-nextest are release-gate additions. No Node-based build runner has been added.
