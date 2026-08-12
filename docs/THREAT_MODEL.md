# Lattice threat model

Status: initial design threat model. Project content and every adapter response are untrusted.

## Assets

User source and place data, Git state, unpublished work, Studio sessions, cloud credentials, DataStores, publication targets, ChangeSet authorizations, audit integrity, Lattice identities, and local model/context data.

## Trust boundaries

- MCP client to Lattice MCP adapter
- Lattice protocol adapters to core services
- filesystem/place/Rojo parsers to canonical model
- Studio MCP/Open Cloud responses to adapters
- source/runtime/project content to context compiler and external models
- extensions to host capabilities
- UI to versioned local IPC

## Threats and controls

| Threat | Required control |
|---|---|
| Malicious or compromised MCP client/model | deny-by-default policy; small semantic tools; bounded inputs/results; no direct mutation; audit all attempts |
| Prompt injection in comments/source/runtime logs | typed trust labels; content is data, never instructions; preserve source delimiters; policy independent of model text |
| Hostile project/place/asset | bounded parsing, no symlink traversal, path containment, size/depth limits, native parser fuzz/property tests, never execute during ingestion |
| Arbitrary file access/path traversal | canonical workspace roots, stable IDs, allowlisted adapter roots, reject traversal and symlink escapes |
| Wine guest/host namespace confusion | distinct `WinePath`/`HostPath` types, discovered drive maps, no guest string passed to host APIs, component and prefix validation |
| Malicious Wine drive mapping | canonicalize existing ancestors, reject mapping escape, do not assume `Z:`, retain environment/runtime evidence |
| Flatpak access confusion | separate missing, host permission, and sandbox-denied states; never infer Lattice access from Studio access |
| Secret exfiltration | OS credential store abstraction; database stores identifiers only; redact logs/errors; external context export requires policy |
| Destructive publish/DataStore write/Git push | destructive flag, explicit target, policy plus user authorization, dry-run/preview, audit, verification; never implied by developer role |
| Studio session confusion | enumerate sessions, stable local session IDs, explicit target once ambiguous, revalidate place before mutation |
| PID reuse or helper misclassification | environment fingerprint includes process start time and installation; main executable evidence is distinct from related Wine/WebView processes |
| Secrets in process environments | `sysinfo` environment values are filtered immediately to a fixed Wine/Flatpak correlation allowlist and never included in reports |
| Replayed/forged ChangeSet or reference | state-machine nonce/idempotency, canonical resolver, principal/resource authorization, hash/revision preconditions |
| Concurrent human edit | mandatory re-read and BLAKE3/revision compare; return `REVISION_CONFLICT`; never overwrite |
| Malicious extension | Wasmtime component isolation, declared permissions, bounded resources, no write/publish without explicit grants |
| Cache/SQLite tampering | BLAKE3 verification, SQLite constraints/transactions, migration version checks; cache is disposable, never source authority |
| Denial of service | bounded channels, semaphore job limits, source/result limits, timeouts, cancellation, backpressure, quota-aware cloud polling |
| Native Luau FFI memory bug | exact upstream pin, narrow owned CXX structs, no raw pointer escape, compatibility fixtures, sanitizer builds in native matrix |

## Prompt-injection packet labels

Every future context packet must keep these sections structurally distinct:

```text
SYSTEM METADATA
TRUSTED LATTICE EVIDENCE
PROJECT CONTENT (UNTRUSTED)
RUNTIME OUTPUT (UNTRUSTED)
USER INSTRUCTION
```

An evidence origin reports how a fact was obtained; it does not make arbitrary payload text authoritative.

## Residual risks in bootstrap

There is no mutation surface, cloud adapter, extension loader, or secret store yet, reducing current impact. The local CLI trusts the invoking OS user and filesystem permissions. SQLite metadata is not cryptographically authenticated. Tantivy and Luau process hostile content in-process; fuzzing and sandboxed ingestion are future hardening work.
