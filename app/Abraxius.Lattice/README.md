# Abraxius Lattice workstation

This is the native Avalonia control surface for Lattice. It owns presentation
state only; the Rust daemon remains authoritative for project, provider,
Studio, trace, policy, and result state.

The project targets Avalonia `12.1.1` with compiled bindings enabled. The
desktop shell starts the native daemon's loopback Studio bridge in bridge-only
mode so the Roblox companion can discover and pair before a workspace is open.
Project and index state still come from the daemon's workspace lifecycle; the
shell does not fabricate it.

Avalonia 12.1's source generators require a current compiler; use the .NET 9
SDK (or newer supported SDK) for local builds.

Build when the .NET SDK is available:

```text
dotnet restore app/Abraxius.Lattice/Abraxius.Lattice.csproj
dotnet build app/Abraxius.Lattice/Abraxius.Lattice.csproj --no-restore
```

The native editor boundary is built by Cargo when a local artifact is missing
and copied beside the Avalonia executable:

```text
cargo build -p lattice-editor-native
```

The E0/E1 editor surface is available from the **Editor** navigation item and
by double-clicking a Luau/Lua file in Explorer. It renders visible lines from
the Rust rope through a custom Avalonia control; it does not use a TextBox or
AvaloniaEdit. Studio-backed revision-safe save and semantic Luau integration
are the next editor slices.

The desktop integration refreshes a user-scoped launcher on every startup,
including the current executable target and artwork. Closing the main window
hides it to the native tray; use the tray menu's explicit
`Exit Lattice` command to terminate the UI process. A user-scoped exclusive
lock prevents a second desktop instance from starting while the tray instance
is still alive. Left-clicking the tray icon restores the window.
