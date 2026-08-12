# Lattice database schema

SQLite is authoritative local metadata for v1. Embedded migrations `0001_initial.sql` and `0002_tool_fabric.sql` are applied transactionally and recorded in `schema_versions`. Startup enables foreign keys, WAL, transactions, and a five-second busy timeout.

## Main groups

| Group | Tables |
|---|---|
| Identity/project | `workspaces`, `projects`, `universes`, `places`, `entities`, `entity_aliases`, `resource_refs` |
| Source/content | `source_units`, `source_revisions`, `content_objects`, `files` |
| Intelligence | `symbols`, `references`, `edges`, `diagnostics`, `evidence`, `index_metadata` |
| Git | `git_state` |
| Change safety | `changesets`, `change_operations`, `change_preconditions`, `rollback_records` |
| Execution | `test_runs`, `test_results`, `runtime_events`, `jobs` |
| Adapters | `studio_environments`, `studio_environment_paths`, `studio_sessions`, `cloud_sessions` |
| Audit/agents | `tool_invocations`, `agent_runs`, `audit_events` |
| Tool fabric | `providers`, `provider_connections`, `provider_health`, `provider_catalog_revisions`, `tools`, `tool_schema_revisions`, `capabilities`, `tool_capability_bindings`, `provider_resources`, `tool_results` |
| Migration | `schema_versions` |

Large immutable content is never stored in normal rows. `content_objects` records a `b3:<hex>` identity, byte size, and relative/native object location; bytes live under `.lattice/objects/b3/aa/bb...`. Reads verify BLAKE3 and report cache corruption.

Each source revision is append-only. `source_units.current_hash` is a convenience pointer to the latest immutable object. A mutation must compare its expected hash/revision within the apply transaction before changing external state.

The cache is disposable. No user source lives only in SQLite or the object store.

Studio environment paths are scoped by environment, semantic role, and namespace. Absolute user-specific paths are not canonical identities; persisted path records prefer a role-relative representation. Studio sessions reference `studio_environment_id` and retain the correlated process ID as temporary evidence rather than identity.
