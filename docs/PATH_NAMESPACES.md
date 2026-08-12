# Path namespaces

Lattice distinguishes host paths from paths seen by Wine or Studio:

```text
Host         native operating-system filesystem path
WineGuest    absolute drive-qualified Windows path inside Wine
StudioGuest  a Studio-reported path whose runtime namespace is known
```

`HostPath` wraps `PathBuf`. `WinePath` is a parsed absolute drive path with validated components. `ResolvedPath` retains both when they identify the same location.

## Translation

`WinePathTranslator` uses drive mappings from the resolved environment. `C:` normally maps to the detected prefix's `drive_c`; other drives, including `Z:`, exist only when discovered beneath the active prefix's `dosdevices` directory.

Translation rules:

- drive letters are normalized to uppercase;
- `/` and `\` are accepted as guest separators;
- `.` is removed and `..` is rejected;
- UNC, NUL-bearing, colon-bearing components, and unmapped drives are rejected;
- guest-to-host results must remain within the drive mapping;
- existing symlink ancestors are canonicalized to prevent escape;
- host-to-guest chooses the most specific matching drive mapping.

Lexical normalization does not touch the filesystem and can represent missing paths. Filesystem canonicalization requires access and existence and is used only when its stronger semantics are needed.

Host path equality follows the host filesystem representation. Lattice does not lowercase paths as identity: Windows, macOS, Linux, and Wine may differ in case behavior. Persisted references should use environment ID, semantic role, and relative path instead of an absolute user-specific path.
