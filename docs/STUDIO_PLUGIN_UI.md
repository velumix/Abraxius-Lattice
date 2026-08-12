# Lattice Roblox Studio plugin UI

The Studio plugin is a compact cockpit over the automatically paired Lattice companion
bridge. The daemon owns authoritative environment, connection, indexing,
recorder, capability, client, and authorization state. The plugin presents that
state and reports Studio-local selection and lifecycle observations.

## Architecture

```text
Lattice daemon / bridge response
             |
             v
BridgeController + Studio controllers
             |
             v
central PluginState
             |
             v
FusionAdapter -> Lattice components -> DockWidgetPluginGui
```

Only `PluginUI/UI/FusionAdapter.luau` consumes Fusion's constructor and scope
API. Controllers never import Fusion. Components never perform HTTP requests or
poll backend systems. High-frequency bridge requests are coalesced into one
bounded activity sample per second, activity retains 100 rows, logs retain 200
entries, and the sparkline uses 29 persistent segments.

## Surfaces and layouts

One `PluginToolbar` button toggles one `DockWidgetPluginGui`. The widget defaults
to 340 by 520 pixels, has a 280 by 320 minimum, and switches between narrow,
normal, and wide compositions at 320 and 520 pixels. The same component
instances are repositioned; layouts do not duplicate controls or state.

The main surface contains Header, Connection, Activity, Studio, Selection,
Flight Recorder, bottom status, Details, and bounded Logs. Full graphs, trace
timelines, registries, benchmark dashboards, source editing, and chat remain in
the native Lattice application.

## Theme and accessibility

`ThemeController` reacts to Studio theme changes and regenerates semantic
tokens from `StudioTheme`. Components consume tokens rather than literal
colors. Statuses use symbols and text as well as color, mutations use different
shapes from reads, and normal controls retain Studio-sized click targets.

## Truth boundary

The bridge response carries the actual Phase 1 environment fingerprint. Studio
MCP health, canonical `rbx://` references, recorder state, clients, and policy
metadata remain unavailable until their native services bind them. The plugin
shows and disables unavailable functions rather than installing mock live
state. `PluginUI/Fixtures/TestStates.luau` exists solely for visual development
and is never required by production code.

## Packaging and installation

Rojo packages the root plugin script, Lattice modules, and exact-pinned Fusion
source into `dist/LatticeCompanion.rbxm`. Installation copies that immutable
build artifact into the Studio plugins directory resolved by `lattice-platform`.
Installation never starts or restarts Studio. If Studio is already running, the
developer explicitly reloads installed plugins when convenient.
