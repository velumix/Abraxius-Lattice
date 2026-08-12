# Avalonia workstation shell

The native desktop surface lives in `app/Abraxius.Lattice`. Its XAML is the
visual specification for the workstation: semantic theme dictionaries, dense
Fluent controls, a navigation rail, resizable context/center/inspector panes,
virtualized lists, and custom-rendered graph/timeline surfaces.

The project uses Avalonia `12.1.1` with:

```xml
<AvaloniaUseCompiledBindingsByDefault>true</AvaloniaUseCompiledBindingsByDefault>
```

ViewModels are presentation contracts only. They currently start in an
explicit disconnected state because the versioned native IPC contract in
`docs/LATTICE_IPC_PROTOCOL.md` is still a design boundary. No Studio, provider,
index, trace, or benchmark status is fabricated by the shell.

Build with the .NET 9 SDK or newer supported compiler:

```text
dotnet restore app/Abraxius.Lattice/Abraxius.Lattice.csproj
dotnet build app/Abraxius.Lattice/Abraxius.Lattice.csproj
```

Graph and timeline are intentionally `Control` subclasses that draw through
`DrawingContext`; they do not create one Avalonia element per graph node or
trace event. The daemon/IPC adapter can replace the disconnected ViewModel
source without changing the XAML surface.

## Desktop lifecycle

On every startup the desktop integration service refreshes a user-scoped
launcher, including its current executable target and Lattice artwork. Linux
receives an application-menu entry and, when the user's XDG desktop directory
is available, `Abraxius Lattice.desktop` on the desktop. Linux icon filenames
are content-hashed and the desktop/icon caches are refreshed so a new logo is
visible without manually reinstalling the shortcut. The Windows and macOS
implementations use native user-scoped shortcut/application locations. No
elevation or system-wide write is attempted.

The window close button is intentionally a hide operation. The process and
daemon-facing state remain alive in the native tray. The tray menu provides
`Show Lattice`, `Install desktop icon`, and the explicit `Exit Lattice` action.
