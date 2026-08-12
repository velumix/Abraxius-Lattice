# Dependency security audit

Last run: 2026-08-11 with cargo-audit 0.22.2 and the current RustSec advisory database (1,211 advisories loaded).

## Finding and resolution

RustSec reported `RUSTSEC-2026-0253` against transitive `lru 0.16.4`, required by Tantivy 0.26.1. The bug can leave dangling LRU list pointers if a key destructor panics during `pop`, potentially causing use-after-free in safe Rust after unwind recovery.

Upstream fixes the ordering in `lru 0.18.2`, but Tantivy's stable manifest requires `lru ^0.16.3`. Lattice therefore patches crates.io `lru` to the exact 0.16.4 source under `vendor/lru` and applies the exact semantic fix merged upstream in [lru-rs PR #238](https://github.com/jeromefroe/lru-rs/pull/238), merge commit `f9a7f00fcf2d33e00adb03758cb350aaaa52cddb`: detach the node before freeing it and dropping the key. The upstream panic-on-drop regression is included locally.

The advisory remains version-matched because RustSec cannot attest local backports. It is the only allowed audit exception and must be removed once a stable Tantivy release accepts fixed `lru >=0.18.2`.

Audit command:

```text
cargo audit --file Cargo.lock --deny warnings --ignore RUSTSEC-2026-0253
```

No vulnerability or warning other than this documented, patched version-range match is accepted.
