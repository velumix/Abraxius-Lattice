# ADR-003: SQLite metadata store

## Context

Lattice needs transactional local metadata without a daemon dependency.

## Decision

Use SQLite through exact-pinned rusqlite, with bundled SQLite, WAL, foreign keys, busy timeout, migrations, and transactions. Large immutable bytes live in a BLAKE3 object store.

## Alternatives

PostgreSQL, Redis, Elasticsearch, flat JSON, and storing large blobs in relational rows were rejected for v1.

## Consequences

One disposable `.lattice` directory is sufficient. Schema evolution, corruption checks, and concurrency discipline remain required.

