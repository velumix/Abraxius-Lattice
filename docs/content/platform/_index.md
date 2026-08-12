+++
title = "Platform and Vinegar"
weight = 5
+++

The platform boundary is authoritative for Windows, macOS, Linux/Vinegar, and
Flatpak resolution. Consumers use semantic roles and typed host/guest paths;
they do not hardcode Vinegar directories, Wine prefixes, or operating-system
branches.

## Linux/Vinegar

Vinegar support is resolved through the existing platform layer. A launcher
may use the resolved Wine runtime and `StudioMCP.exe` path, but must not search
for paths again or assume host Wine is the runtime inside Flatpak.

## Path safety

Host and Wine namespaces remain distinct. Translation enforces traversal,
prefix-escape, and symlink-escape protection. See the detailed
[Path Namespaces](https://github.com/abraxius/lattice/blob/main/docs/PATH_NAMESPACES.md)
reference in the source document set.
