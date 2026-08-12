+++
title = "Security"
weight = 11
+++

Lattice treats provider data, tool descriptions, schemas, resources, and
remote annotations as untrusted input. Verified semantics and local policy
remain the authorization authority.

Important boundaries:

- credentials are provider-scoped secret references, never generated docs;
- stdio providers use explicit executables and argument arrays, not shells;
- output and schemas are bounded and large values go to immutable result storage;
- mutation retries require verified idempotence or pre-dispatch failure;
- Studio and Wine paths are translated only through platform capabilities;
- audit records retain caller, provider, tool, schema revision, policy result,
  and OperationId without copying source or secrets by default.

Read the source [threat model](https://github.com/abraxius/lattice/blob/main/docs/THREAT_MODEL.md)
and [security audit](https://github.com/abraxius/lattice/blob/main/docs/SECURITY_AUDIT.md).
