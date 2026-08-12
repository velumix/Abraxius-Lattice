# License audit status

Status: preliminary, not a release approval.

All direct dependencies currently report permissive MIT, Apache-2.0, CC0-1.0, MPL-2.0, or compatible disjunctive licensing in package metadata and upstream license files. `sysinfo` 0.39.6 and Axum 0.8.9 are MIT; `dirs` 6.0.0 is MIT OR Apache-2.0. Luau 0.733, Fusion `v0.3-beta`, Material.Icons.Avalonia 3.0.2, and ropey 1.6.1 are MIT. The reviewed local `lru` backport retains its upstream MIT license. Rojo 7.7.0 and StyLua 2.5.2 are MPL-2.0 native build tools; Selene 0.31.0 remains subject to final distribution notice verification. The archived Roblox Studio Rust MCP reference is MIT but is not a dependency and no code was copied.

Outstanding before distribution:

- inspect every `Cargo.lock` transitive package and feature-selected native component;
- run cargo-deny license and advisory checks;
- confirm notice-generation and source-offer obligations for the final artifacts;
- review Avalonia, rbx-dom, Cedar, and every later dependency when actually adopted;
- obtain project counsel approval for the chosen Abraxius Lattice product license.
