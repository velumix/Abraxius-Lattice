# ADR-013: Fusion Studio cockpit

## Context

Lattice needs a small Studio-native surface for bridge status, current activity,
selection, and recorder controls without duplicating the Avalonia workstation.
Studio requires Luau and native Roblox UI objects. Fusion remains pre-1.0, so
allowing its API into controllers would make upgrades expensive.

## Decision

Use Fusion `v0.3-beta` at exact commit
`77e603534ff4013f4049611826ff0309d6000b15`. Isolate it behind a Lattice-owned
`FusionAdapter`, central reactive state, presentation-only components, and
controller-owned side effects. Package the plugin as an `.rbxm` with exact-
pinned native Rojo tooling. The daemon remains authoritative.

## Alternatives

- Direct imperative UI would avoid a dependency but make responsive derived
  state and cleanup substantially more error-prone.
- React-Lua/Roact would violate the selected stack and introduce a second UI
  model.
- A webview would violate the native architecture and Studio UI requirement.
- Moving daemon logic into Luau would duplicate policy and project truth.

## Consequences

The plugin has deterministic packaging and reactive theme/layout behavior.
Fusion upgrades are concentrated in one adapter and component layer. The exact
Fusion source and MIT notice must ship with plugin distributions. The plugin is
not useful as an independent intelligence service and must represent unavailable
daemon capabilities honestly.
