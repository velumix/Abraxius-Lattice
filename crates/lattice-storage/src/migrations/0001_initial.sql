PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_versions (
    version INTEGER PRIMARY KEY,
    applied_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY,
    root_path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id), name TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS universes (id TEXT PRIMARY KEY, project_id TEXT REFERENCES projects(id), cloud_universe_id INTEGER);
CREATE TABLE IF NOT EXISTS places (id TEXT PRIMARY KEY, project_id TEXT REFERENCES projects(id), cloud_place_id INTEGER);

CREATE TABLE IF NOT EXISTS entities (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    resource_ref TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    display_path TEXT,
    revision INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS entities_workspace_kind ON entities(workspace_id, kind);
CREATE TABLE IF NOT EXISTS entity_aliases (entity_id TEXT NOT NULL REFERENCES entities(id), alias TEXT NOT NULL, valid_from_revision INTEGER NOT NULL, valid_to_revision INTEGER, PRIMARY KEY(entity_id, alias, valid_from_revision));
CREATE TABLE IF NOT EXISTS resource_refs (resource_ref TEXT PRIMARY KEY, entity_id TEXT NOT NULL REFERENCES entities(id), active INTEGER NOT NULL DEFAULT 1);

CREATE TABLE IF NOT EXISTS content_objects (
    hash TEXT PRIMARY KEY,
    byte_len INTEGER NOT NULL,
    object_path TEXT NOT NULL,
    verified_at_unix_ms INTEGER
);
CREATE TABLE IF NOT EXISTS source_units (
    id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL UNIQUE REFERENCES entities(id),
    language TEXT NOT NULL,
    current_revision INTEGER NOT NULL,
    current_hash TEXT NOT NULL REFERENCES content_objects(hash)
);
CREATE TABLE IF NOT EXISTS source_revisions (
    source_unit_id TEXT NOT NULL REFERENCES source_units(id),
    revision INTEGER NOT NULL,
    content_hash TEXT NOT NULL REFERENCES content_objects(hash),
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(source_unit_id, revision)
);

CREATE TABLE IF NOT EXISTS symbols (id TEXT PRIMARY KEY, source_unit_id TEXT NOT NULL REFERENCES source_units(id), name TEXT NOT NULL, kind TEXT NOT NULL, begin_line INTEGER NOT NULL, begin_column INTEGER NOT NULL, end_line INTEGER NOT NULL, end_column INTEGER NOT NULL, revision INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS symbols_name ON symbols(name);
CREATE TABLE IF NOT EXISTS "references" (id TEXT PRIMARY KEY, source_unit_id TEXT NOT NULL REFERENCES source_units(id), name TEXT NOT NULL, kind TEXT NOT NULL, begin_line INTEGER NOT NULL, begin_column INTEGER NOT NULL, end_line INTEGER NOT NULL, end_column INTEGER NOT NULL, revision INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS references_name ON "references"(name);
CREATE TABLE IF NOT EXISTS edges (id TEXT PRIMARY KEY, source_entity_id TEXT NOT NULL REFERENCES entities(id), target_ref TEXT NOT NULL, kind TEXT NOT NULL, origin TEXT NOT NULL, confidence TEXT NOT NULL, evidence_id TEXT, revision INTEGER NOT NULL);

CREATE TABLE IF NOT EXISTS files (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id), entity_id TEXT NOT NULL REFERENCES entities(id), relative_path TEXT NOT NULL, content_hash TEXT NOT NULL, modified_at_unix_ms INTEGER NOT NULL, UNIQUE(workspace_id, relative_path));
CREATE TABLE IF NOT EXISTS git_state (workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id), branch TEXT, head_commit TEXT, dirty INTEGER NOT NULL DEFAULT 0, observed_at_unix_ms INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS diagnostics (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id), resource_ref TEXT NOT NULL, severity TEXT NOT NULL, origin TEXT NOT NULL, message TEXT NOT NULL, evidence_id TEXT, revision INTEGER NOT NULL, resolved_at_unix_ms INTEGER);
CREATE TABLE IF NOT EXISTS evidence (id TEXT PRIMARY KEY, resource_ref TEXT NOT NULL, kind TEXT NOT NULL, origin TEXT NOT NULL, confidence TEXT NOT NULL, payload_hash TEXT NOT NULL, revision INTEGER NOT NULL, source_span_json TEXT);

CREATE TABLE IF NOT EXISTS changesets (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id), state TEXT NOT NULL, principal TEXT NOT NULL, created_at_unix_ms INTEGER NOT NULL, updated_at_unix_ms INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS change_operations (id TEXT PRIMARY KEY, changeset_id TEXT NOT NULL REFERENCES changesets(id), ordinal INTEGER NOT NULL, kind TEXT NOT NULL, target_ref TEXT NOT NULL, risk TEXT NOT NULL, payload_json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS change_preconditions (operation_id TEXT NOT NULL REFERENCES change_operations(id), expected_revision INTEGER, expected_hash TEXT, PRIMARY KEY(operation_id));
CREATE TABLE IF NOT EXISTS rollback_records (id TEXT PRIMARY KEY, changeset_id TEXT NOT NULL REFERENCES changesets(id), operation_id TEXT NOT NULL REFERENCES change_operations(id), previous_hash TEXT, reversible INTEGER NOT NULL, payload_json TEXT);

CREATE TABLE IF NOT EXISTS test_runs (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id), state TEXT NOT NULL, started_at_unix_ms INTEGER NOT NULL, finished_at_unix_ms INTEGER);
CREATE TABLE IF NOT EXISTS test_results (id TEXT PRIMARY KEY, test_run_id TEXT NOT NULL REFERENCES test_runs(id), resource_ref TEXT NOT NULL, status TEXT NOT NULL, evidence_id TEXT, payload_hash TEXT);
CREATE TABLE IF NOT EXISTS runtime_events (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id), source TEXT NOT NULL, level TEXT NOT NULL, occurred_at_unix_ms INTEGER NOT NULL, payload_hash TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS studio_environments (id TEXT PRIMARY KEY, host_platform TEXT NOT NULL, runtime TEXT NOT NULL, resolver_version INTEGER NOT NULL, deployment_id TEXT, process_id INTEGER, process_start_unix_seconds INTEGER, capabilities_json TEXT NOT NULL, observed_at_unix_ms INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS studio_environment_paths (environment_id TEXT NOT NULL REFERENCES studio_environments(id) ON DELETE CASCADE, role TEXT NOT NULL, namespace TEXT NOT NULL, relative_path TEXT, availability TEXT NOT NULL, origin TEXT NOT NULL, PRIMARY KEY(environment_id, role, namespace));
CREATE TABLE IF NOT EXISTS studio_sessions (id TEXT PRIMARY KEY, workspace_id TEXT REFERENCES workspaces(id), studio_environment_id TEXT REFERENCES studio_environments(id), process_id INTEGER, external_session_id TEXT NOT NULL, place_label TEXT, state TEXT NOT NULL, capabilities_json TEXT NOT NULL, last_heartbeat_unix_ms INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS cloud_sessions (id TEXT PRIMARY KEY, workspace_id TEXT REFERENCES workspaces(id), principal_label TEXT NOT NULL, secret_identifier TEXT NOT NULL, capabilities_json TEXT NOT NULL, observed_at_unix_ms INTEGER NOT NULL);

CREATE TABLE IF NOT EXISTS tool_invocations (id TEXT PRIMARY KEY, principal TEXT NOT NULL, operation TEXT NOT NULL, target_ref TEXT, policy_decision TEXT NOT NULL, started_at_unix_ms INTEGER NOT NULL, finished_at_unix_ms INTEGER, result_code TEXT);
CREATE TABLE IF NOT EXISTS agent_runs (id TEXT PRIMARY KEY, principal TEXT NOT NULL, started_at_unix_ms INTEGER NOT NULL, finished_at_unix_ms INTEGER, status TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS audit_events (id TEXT PRIMARY KEY, timestamp_unix_ms INTEGER NOT NULL, principal TEXT NOT NULL, action TEXT NOT NULL, resource_ref TEXT, policy_decision TEXT NOT NULL, changeset_id TEXT, result_code TEXT NOT NULL, evidence_ids_json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS jobs (id TEXT PRIMARY KEY, workspace_id TEXT REFERENCES workspaces(id), kind TEXT NOT NULL, state TEXT NOT NULL, progress_current INTEGER NOT NULL DEFAULT 0, progress_total INTEGER, created_at_unix_ms INTEGER NOT NULL, updated_at_unix_ms INTEGER NOT NULL, result_ref TEXT, error_code TEXT);
CREATE TABLE IF NOT EXISTS index_metadata (index_name TEXT PRIMARY KEY, schema_version INTEGER NOT NULL, indexed_workspace_revision INTEGER NOT NULL, updated_at_unix_ms INTEGER NOT NULL);

INSERT OR IGNORE INTO schema_versions(version, applied_at_unix_ms) VALUES (1, unixepoch('subsec') * 1000);
