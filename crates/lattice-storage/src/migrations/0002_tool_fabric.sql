CREATE TABLE providers (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    version TEXT,
    trust TEXT NOT NULL,
    transport TEXT NOT NULL,
    metadata_json TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE provider_connections (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    state TEXT NOT NULL,
    endpoint_identity TEXT,
    connected_at_unix_ms INTEGER,
    disconnected_at_unix_ms INTEGER,
    last_rtt_micros INTEGER,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    last_error_code TEXT
);
CREATE INDEX provider_connections_provider ON provider_connections(provider_id);

CREATE TABLE provider_health (
    provider_id TEXT PRIMARY KEY REFERENCES providers(id),
    status TEXT NOT NULL,
    reason TEXT NOT NULL,
    authentication_state TEXT NOT NULL,
    catalog_revision TEXT,
    tool_count INTEGER NOT NULL,
    resource_count INTEGER NOT NULL,
    observed_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE provider_catalog_revisions (
    provider_id TEXT NOT NULL REFERENCES providers(id),
    revision TEXT NOT NULL,
    observed_at_unix_ms INTEGER NOT NULL,
    active INTEGER NOT NULL,
    PRIMARY KEY(provider_id, revision)
);

CREATE TABLE tool_schema_revisions (
    revision TEXT PRIMARY KEY,
    provider_schema_hash TEXT NOT NULL,
    normalized_schema_hash TEXT NOT NULL,
    validation_state TEXT NOT NULL,
    byte_len INTEGER NOT NULL,
    created_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE tools (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    native_name TEXT NOT NULL,
    title TEXT,
    provider_description TEXT,
    active_input_schema_revision TEXT NOT NULL REFERENCES tool_schema_revisions(revision),
    active_output_schema_revision TEXT REFERENCES tool_schema_revisions(revision),
    verified_semantics_json TEXT NOT NULL,
    reported_semantics_json TEXT NOT NULL,
    trust TEXT NOT NULL,
    availability TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    UNIQUE(provider_id, native_name)
);
CREATE INDEX tools_provider_availability ON tools(provider_id, availability);

CREATE TABLE capabilities (
    id TEXT PRIMARY KEY,
    meaning TEXT NOT NULL,
    input_contract TEXT NOT NULL,
    output_contract TEXT NOT NULL,
    side_effects_json TEXT NOT NULL
);

CREATE TABLE tool_capability_bindings (
    tool_id TEXT NOT NULL REFERENCES tools(id),
    capability_id TEXT NOT NULL REFERENCES capabilities(id),
    target_json TEXT NOT NULL,
    origin TEXT NOT NULL,
    PRIMARY KEY(tool_id, capability_id, target_json)
);

CREATE TABLE provider_resources (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    original_uri TEXT NOT NULL,
    mime_type TEXT,
    metadata_json TEXT NOT NULL,
    availability TEXT NOT NULL,
    UNIQUE(provider_id, original_uri)
);

CREATE TABLE tool_results (
    id TEXT PRIMARY KEY,
    operation_id TEXT NOT NULL,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    tool_id TEXT NOT NULL REFERENCES tools(id),
    schema_revision TEXT NOT NULL REFERENCES tool_schema_revisions(revision),
    arguments_hash TEXT NOT NULL,
    content_hash TEXT,
    byte_len INTEGER NOT NULL,
    content_type TEXT,
    created_at_unix_ms INTEGER NOT NULL
);
CREATE INDEX tool_results_operation ON tool_results(operation_id);

INSERT INTO schema_versions(version, applied_at_unix_ms)
VALUES (2, unixepoch('subsec') * 1000);
