# Lattice internal IPC protocol

Status: design boundary; not implemented in the bootstrap.

The Avalonia UI and native CLI/daemon integrations will use a versioned, language-neutral local IPC contract independent of MCP.

- Windows transport: named pipe scoped to the current user.
- Linux/macOS transport: Unix domain socket with owner-only permissions.
- Encoding candidate: protobuf messages with length framing; adoption requires native Rust and .NET implementations that support these transports without a TCP-only assumption.
- Negotiation: protocol major/minor, daemon build, feature set, maximum frame size, and authentication mode.
- Calls: request ID, deadline, cancellation ID, principal, typed operation, and typed structured error.
- Events: monotonically ordered subscription sequence with resume cursor and bounded replay.
- Default maximum frame: 4 MiB. Larger payloads use immutable result/object references.

Loopback TCP and JSON-over-localhost are not the default. MCP remains the external AI interoperability protocol and does not become UI IPC.

