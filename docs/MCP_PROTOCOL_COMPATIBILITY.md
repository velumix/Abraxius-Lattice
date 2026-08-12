# MCP protocol compatibility

`lattice-mcp` owns a protocol compatibility profile so version behavior does not leak into Lattice Core, provider, Studio, routing, or catalog domain models.

## Profiles

`Modern2026_07_28` uses:

- `server/discover` rather than `initialize`;
- protocol metadata on each request;
- stateless protocol operation with no `Mcp-Session-Id`;
- `subscriptions/listen` for supported catalog changes;
- no standalone HTTP GET notification stream.

`Legacy2025` isolates initialization, initialized notification, legacy connection/session behavior, cancellation, and catalog notifications. Provider inspection exposes the negotiated revision, path, session model, catalog-change model, and fallback reason.

## Negotiation rule

The general external-provider path attempts `server/discover` exactly once. It falls back to legacy initialization only when discovery returns JSON-RPC `METHOD_NOT_FOUND`. Invalid requests, invalid metadata, malformed responses, connection failures, and other protocol errors do not downgrade.

The current Roblox Studio MCP build is a documented exception in observed behavior, not in the general rule: it closes after `server/discover` and writes an explicit legacy expectation to stderr. The trusted Studio launcher retries once only when both the client failure class and the exact bounded stderr evidence agree. The first child is reaped before the second is launched, and the resulting provider metadata records why legacy initialization was selected.

## Wire acceptance

The native Rust fixture is a raw JSON-RPC peer rather than another SDK endpoint. Tests assert observable methods and metadata:

- modern: `server/discover`, `tools/list`, and `tools/call`; no `initialize` or `notifications/initialized`;
- legacy: `server/discover` returns `METHOD_NOT_FOUND`, then initialization and tool discovery succeed;
- malformed discovery: connection fails and never attempts initialization.

This guards Lattice behavior independently of RMCP's internal success result.
