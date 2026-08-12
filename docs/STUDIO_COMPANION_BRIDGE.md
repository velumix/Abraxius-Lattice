# Studio companion bridge

Status: native Rust transport and read-only Luau client implemented; installation
and live-Studio acceptance are intentionally not performed automatically.

## Purpose

The companion bridge supplies continuous Studio-owned information that is a poor
fit for one-shot MCP tools: window/session registration, heartbeat, selection,
Studio mode, bounded event telemetry, and future adaptive observation. The
official Studio MCP provider remains the preferred implementation for supported
Studio primitives.

```text
Roblox Studio window (Luau plugin)
               |
               | loopback HTTP with automatic local pairing
               v
lattice-studio-bridge (Rust/Axum)
               |
               +-- bounded per-session event ring
               +-- bounded per-session command leases
               +-- explicit StudioEnvironment binding
               +-- no provider selection or reasoning
```

The plugin initiates every network request. The daemon never launches or restarts
Studio and never needs to derive a Wine command for this channel. This makes the
wire design identical on Windows, macOS, Vinegar native, and Vinegar Flatpak.
The plugin does not require a user-managed key: it discovers the loopback bridge,
redeems a one-time challenge, and uses a short-lived in-memory session credential
automatically.

## Identity and correlation

On each Edit DataModel plugin load, the plugin creates an
`external_session_id`. Registration returns a Lattice
`StudioBridgeSessionId`. Re-registration with the same external identifier
returns the same bridge identity while the session remains alive.

A registration begins `Unbound`. A trusted host-side correlator must explicitly
bind it to all of:

- `StudioSessionId`
- `StudioEnvironmentId`
- Studio process ID

The bridge rejects command dispatch to an unbound session. It never picks the
last heartbeat, most recent window, or first matching place. This preserves the
existing multi-Studio ambiguity rule.

## Wire protocol v1

Endpoints:

```text
GET  /health
GET  /v1/studio-bridge/discover
POST /v1/studio-bridge/pair
POST /v1/studio-bridge/register
POST /v1/studio-bridge/report
GET  /v1/studio-bridge/sessions
```

`/health`, `/discover`, and `/pair` are loopback-only bootstrap endpoints.
`/register`, `/report`, and `/sessions` require:

```text
Authorization: Bearer <bridge token>
```

The daemon refuses non-loopback binds. Request bodies default to a 1 MiB limit.
Discovery challenges expire after 30 seconds and can be redeemed once. The
resulting session credential expires after 12 hours and is never rendered in
the plugin UI or written to source. An optional 32–256 byte
`LATTICE_STUDIO_BRIDGE_TOKEN` remains available for legacy clients; it is not
needed for automatic Studio pairing.

Reports are monotonically sequenced. A plugin retries an uncertain report using
the same sequence and body. Lattice returns its cached response without ingesting
events or command results twice. Commands remain leased and are redelivered with
the same command ID until the plugin reports a result. The plugin caches command
results by ID, preventing duplicate execution after a lost response.

## Bounds and failure behavior

Defaults are deliberately finite:

| Resource | Limit |
|---|---:|
| Plugin sessions | 32 |
| Pending commands per session | 128 |
| Commands per report | 32 |
| Events per report | 512 |
| Retained events per session | 4,096 |
| Command results per report | 128 |
| HTTP request body | 1 MiB |
| Command timeout | 15 seconds |
| Session TTL | 60 seconds |

When the event ring is full, the oldest event is discarded and
`dropped_events_total` advances. Command queues reject new work when full. A
missing daemon causes exponential plugin backoff up to 15 seconds. No unbounded
queue or background task is created per heartbeat.

## Trust and mutation boundary

Plugin metadata, event text, console output, paths, and extension payloads are
untrusted project/runtime data. They are never instructions or policy authority.

The initial command set is read-only:

- `get_studio_state`
- `get_selection`
- `read_source`
- `get_children`
- `subscribe`

Source reads have a 512 KiB bridge inline limit. Larger content must eventually
use a chunked/result-store path. Source writes, Instance mutations, execution,
and deletion are deliberately absent until they pass the existing ChangeSet,
policy, precondition, verification, audit, and rollback pipeline.

## Running explicitly

The bridge is disabled unless requested:

```text
lattice-daemon \
  --workspace <project> \
  --studio-bridge-bind 127.0.0.1:13471
```

The plugin pairs automatically after Studio has **Allow HTTP Requests** enabled.
No token is entered in Studio. The bridge is still restricted to loopback and
does not accept non-local binds.

## Remaining acceptance work

Automated tests verify bounds, replay behavior, explicit binding, loopback
enforcement, token redaction, automatic pairing, and HTTP registration. A real
Studio acceptance test still requires the user to install the plugin and enable
HTTP requests. Lattice must report the companion as unavailable until a real
automatically paired registration is observed.
