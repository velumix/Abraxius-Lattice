//! `SQLite` metadata storage and disposable BLAKE3 object cache.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use lattice_luau::{LuauAnalysis, ReferenceKind, SymbolKind};
use lattice_resource::{ContentHash, LatticeId, ResourceKind, ResourceRef};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const INITIAL_SCHEMA: &str = include_str!("migrations/0001_initial.sql");
const TOOL_FABRIC_SCHEMA: &str = include_str!("migrations/0002_tool_fabric.sql");

#[derive(Clone, Debug)]
pub struct CacheLayout {
    root: PathBuf,
}

impl CacheLayout {
    pub fn initialize(project_root: &Path) -> Result<Self, StorageError> {
        let root = project_root.join(".lattice");
        for child in ["objects/b3", "indexes", "snapshots", "logs"] {
            fs::create_dir_all(root.join(child))?;
        }
        Ok(Self { root })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn database_path(&self) -> PathBuf {
        self.root.join("database.sqlite")
    }

    #[must_use]
    pub fn index_path(&self) -> PathBuf {
        self.root.join("indexes/source")
    }

    #[must_use]
    pub fn tool_index_path(&self) -> PathBuf {
        self.root.join("indexes/tools")
    }

    #[must_use]
    pub fn object_store(&self) -> ObjectStore {
        ObjectStore { root: self.root.join("objects") }
    }
}

#[derive(Clone, Debug)]
pub struct ObjectStore {
    root: PathBuf,
}

impl ObjectStore {
    pub fn put(&self, bytes: &[u8]) -> Result<ObjectMetadata, StorageError> {
        let hash = ContentHash::of(bytes);
        let path = self.path_for(hash);
        if path.exists() {
            let existing = fs::read(&path)?;
            if ContentHash::of(&existing) != hash {
                return Err(StorageError::CacheCorrupt(path));
            }
            return Ok(ObjectMetadata { hash, byte_len: bytes.len() as u64, path });
        }

        let parent = path.parent().ok_or_else(|| StorageError::CacheCorrupt(path.clone()))?;
        fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(".{}.{}.tmp", hash.to_hex(), LatticeId::new()));
        let write_result = (|| -> io::Result<()> {
            let mut file = fs::OpenOptions::new().create_new(true).write(true).open(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &path)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ignored = fs::remove_file(&temporary);
        }
        write_result?;
        Ok(ObjectMetadata { hash, byte_len: bytes.len() as u64, path })
    }

    pub fn get(&self, hash: ContentHash) -> Result<Vec<u8>, StorageError> {
        let path = self.path_for(hash);
        let bytes = fs::read(&path)?;
        if ContentHash::of(&bytes) != hash {
            return Err(StorageError::CacheCorrupt(path));
        }
        Ok(bytes)
    }

    #[must_use]
    pub fn path_for(&self, hash: ContentHash) -> PathBuf {
        let hex = hash.to_hex();
        self.root.join("b3").join(&hex[0..2]).join(&hex[2..])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectMetadata {
    pub hash: ContentHash,
    pub byte_len: u64,
    pub path: PathBuf,
}

pub struct Database {
    connection: Connection,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceRecord {
    pub id: LatticeId,
    pub root_path: PathBuf,
    pub name: String,
    pub revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredSource {
    pub entity_id: LatticeId,
    pub source_unit_id: LatticeId,
    pub resource_ref: ResourceRef,
    pub relative_path: String,
    pub name: String,
    pub revision: u64,
    pub content_hash: ContentHash,
    pub changed: bool,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.execute_batch(INITIAL_SCHEMA)?;
        apply_migration(&connection, 2, TOOL_FABRIC_SCHEMA)?;
        Ok(Self { connection })
    }

    pub fn open_workspace(&mut self, root: &Path) -> Result<WorkspaceRecord, StorageError> {
        let root_text = root.to_string_lossy().into_owned();
        if let Some(existing) = self.workspace_by_root(&root_text)? {
            return Ok(existing);
        }

        let id = LatticeId::new();
        let name =
            root.file_name().and_then(|value| value.to_str()).unwrap_or("workspace").to_owned();
        let now = unix_ms()?;
        self.connection.execute(
            "INSERT INTO workspaces(id, root_path, name, created_at_unix_ms, updated_at_unix_ms) VALUES (?1, ?2, ?3, ?4, ?4)",
            params![id.to_string(), root_text, name, now],
        )?;
        Ok(WorkspaceRecord { id, root_path: root.to_path_buf(), name, revision: 0 })
    }

    pub fn workspace(&self, id: LatticeId) -> Result<Option<WorkspaceRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, root_path, name, revision FROM workspaces WHERE id = ?1",
                [id.to_string()],
                row_to_workspace,
            )
            .optional()
            .map_err(StorageError::from)
    }

    fn workspace_by_root(&self, root: &str) -> Result<Option<WorkspaceRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, root_path, name, revision FROM workspaces WHERE root_path = ?1",
                [root],
                row_to_workspace,
            )
            .optional()
            .map_err(StorageError::from)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_source(
        &mut self,
        workspace: &WorkspaceRecord,
        relative_path: &str,
        name: &str,
        object: &ObjectMetadata,
        analysis: &LuauAnalysis,
    ) -> Result<StoredSource, StorageError> {
        let tx = self.connection.transaction()?;
        let existing: Option<(String, String, String, i64, String)> = tx
            .query_row(
                "SELECT f.entity_id, su.id, e.resource_ref, su.current_revision, su.current_hash FROM files f JOIN source_units su ON su.entity_id = f.entity_id JOIN entities e ON e.id = f.entity_id WHERE f.workspace_id = ?1 AND f.relative_path = ?2",
                params![workspace.id.to_string(), relative_path],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()?;

        let hash_text = object.hash.to_string();
        let object_path = object.path.to_string_lossy().into_owned();
        tx.execute(
            "INSERT OR IGNORE INTO content_objects(hash, byte_len, object_path) VALUES (?1, ?2, ?3)",
            params![hash_text, to_i64(object.byte_len)?, object_path],
        )?;

        let (entity_id, source_unit_id, resource_ref, previous_revision, previous_hash) =
            if let Some((entity, unit, reference, revision, hash)) = existing {
                (
                    parse_lattice_id(&entity)?,
                    parse_lattice_id(&unit)?,
                    reference.parse()?,
                    u64::try_from(revision).map_err(|_| StorageError::IntegerRange)?,
                    hash,
                )
            } else {
                let entity_id = LatticeId::new();
                let source_unit_id = LatticeId::new();
                let resource_ref =
                    ResourceRef::workspace(workspace.id, ResourceKind::Script, entity_id);
                tx.execute(
                "INSERT INTO entities(id, workspace_id, resource_ref, kind, name, display_path, revision) VALUES (?1, ?2, ?3, 'script', ?4, ?5, 0)",
                params![entity_id.to_string(), workspace.id.to_string(), resource_ref.to_string(), name, relative_path],
            )?;
                tx.execute(
                    "INSERT INTO resource_refs(resource_ref, entity_id) VALUES (?1, ?2)",
                    params![resource_ref.to_string(), entity_id.to_string()],
                )?;
                tx.execute(
                "INSERT INTO source_units(id, entity_id, language, current_revision, current_hash) VALUES (?1, ?2, 'luau', 0, ?3)",
                params![source_unit_id.to_string(), entity_id.to_string(), hash_text],
            )?;
                tx.execute(
                "INSERT INTO files(id, workspace_id, entity_id, relative_path, content_hash, modified_at_unix_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![LatticeId::new().to_string(), workspace.id.to_string(), entity_id.to_string(), relative_path, hash_text, unix_ms()?],
            )?;
                (entity_id, source_unit_id, resource_ref, 0, String::new())
            };

        let changed = previous_hash != hash_text;
        let revision =
            if changed { previous_revision.saturating_add(1) } else { previous_revision };
        if changed {
            tx.execute(
                "UPDATE source_units SET current_revision = ?1, current_hash = ?2 WHERE id = ?3",
                params![to_i64(revision)?, hash_text, source_unit_id.to_string()],
            )?;
            tx.execute(
                "UPDATE entities SET revision = ?1, name = ?2, display_path = ?3 WHERE id = ?4",
                params![to_i64(revision)?, name, relative_path, entity_id.to_string()],
            )?;
            tx.execute(
                "UPDATE files SET content_hash = ?1, modified_at_unix_ms = ?2 WHERE workspace_id = ?3 AND relative_path = ?4",
                params![hash_text, unix_ms()?, workspace.id.to_string(), relative_path],
            )?;
            tx.execute(
                "INSERT INTO source_revisions(source_unit_id, revision, content_hash, created_at_unix_ms) VALUES (?1, ?2, ?3, ?4)",
                params![source_unit_id.to_string(), to_i64(revision)?, hash_text, unix_ms()?],
            )?;
            tx.execute(
                "DELETE FROM symbols WHERE source_unit_id = ?1",
                [source_unit_id.to_string()],
            )?;
            tx.execute(
                "DELETE FROM \"references\" WHERE source_unit_id = ?1",
                [source_unit_id.to_string()],
            )?;
            tx.execute("DELETE FROM edges WHERE source_entity_id = ?1", [entity_id.to_string()])?;

            for symbol in &analysis.symbols {
                tx.execute(
                    "INSERT INTO symbols(id, source_unit_id, name, kind, begin_line, begin_column, end_line, end_column, revision) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![LatticeId::new().to_string(), source_unit_id.to_string(), symbol.name, symbol_kind(symbol.kind), symbol.span.begin.line, symbol.span.begin.column, symbol.span.end.line, symbol.span.end.column, to_i64(revision)?],
                )?;
            }
            for reference in &analysis.references {
                tx.execute(
                    "INSERT INTO \"references\"(id, source_unit_id, name, kind, begin_line, begin_column, end_line, end_column, revision) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![LatticeId::new().to_string(), source_unit_id.to_string(), reference.name, reference_kind(reference.kind), reference.span.begin.line, reference.span.begin.column, reference.span.end.line, reference.span.end.column, to_i64(revision)?],
                )?;
            }
            for require in &analysis.requires {
                tx.execute(
                    "INSERT INTO edges(id, source_entity_id, target_ref, kind, origin, confidence, revision) VALUES (?1, ?2, ?3, 'REQUIRES', 'StaticAst', 'Certain', ?4)",
                    params![LatticeId::new().to_string(), entity_id.to_string(), require.specifier, to_i64(revision)?],
                )?;
            }
            tx.execute(
                "UPDATE workspaces SET revision = revision + 1, updated_at_unix_ms = ?1 WHERE id = ?2",
                params![unix_ms()?, workspace.id.to_string()],
            )?;
        }
        tx.commit()?;

        Ok(StoredSource {
            entity_id,
            source_unit_id,
            resource_ref,
            relative_path: relative_path.to_owned(),
            name: name.to_owned(),
            revision,
            content_hash: object.hash,
            changed,
        })
    }

    pub fn source_count(&self, workspace_id: LatticeId) -> Result<u64, StorageError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM files WHERE workspace_id = ?1",
            [workspace_id.to_string()],
            |row| row.get(0),
        )?;
        u64::try_from(count).map_err(|_| StorageError::IntegerRange)
    }

    pub fn graph_node_count(&self, workspace_id: LatticeId) -> Result<u64, StorageError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM entities WHERE workspace_id = ?1",
            [workspace_id.to_string()],
            |row| row.get(0),
        )?;
        u64::try_from(count).map_err(|_| StorageError::IntegerRange)
    }

    pub fn current_workspace_revision(&self, workspace_id: LatticeId) -> Result<u64, StorageError> {
        let revision: i64 = self.connection.query_row(
            "SELECT revision FROM workspaces WHERE id = ?1",
            [workspace_id.to_string()],
            |row| row.get(0),
        )?;
        u64::try_from(revision).map_err(|_| StorageError::IntegerRange)
    }

    pub fn schema_version(&self) -> Result<u64, StorageError> {
        let version: i64 = self.connection.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_versions",
            [],
            |row| row.get(0),
        )?;
        u64::try_from(version).map_err(|_| StorageError::IntegerRange)
    }
}

fn apply_migration(
    connection: &Connection,
    version: i64,
    migration: &str,
) -> Result<(), StorageError> {
    let applied: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM schema_versions WHERE version = ?1)",
        [version],
        |row| row.get(0),
    )?;
    if !applied {
        connection.execute_batch("BEGIN IMMEDIATE")?;
        if let Err(error) = connection.execute_batch(migration) {
            let _rollback_result = connection.execute_batch("ROLLBACK");
            return Err(StorageError::Sqlite(error));
        }
        connection.execute_batch("COMMIT")?;
    }
    Ok(())
}

fn row_to_workspace(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceRecord> {
    let id: String = row.get(0)?;
    let revision: i64 = row.get(3)?;
    Ok(WorkspaceRecord {
        id: id.parse().map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        root_path: PathBuf::from(row.get::<_, String>(1)?),
        name: row.get(2)?,
        revision: u64::try_from(revision).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
    })
}

fn parse_lattice_id(value: &str) -> Result<LatticeId, StorageError> {
    value.parse().map_err(StorageError::Uuid)
}

const fn symbol_kind(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Local => "local",
        SymbolKind::Function => "function",
        SymbolKind::TypeAlias => "type_alias",
    }
}

const fn reference_kind(kind: ReferenceKind) -> &'static str {
    match kind {
        ReferenceKind::Global => "global",
        ReferenceKind::Member => "member",
        ReferenceKind::Call => "call",
    }
}

fn unix_ms() -> Result<i64, StorageError> {
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).map_err(StorageError::Clock)?;
    i64::try_from(duration.as_millis()).map_err(|_| StorageError::IntegerRange)
}

fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::IntegerRange)
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("cache object failed content verification: {0}")]
    CacheCorrupt(PathBuf),
    #[error("system clock is before the Unix epoch: {0}")]
    Clock(std::time::SystemTimeError),
    #[error("integer is outside the supported SQLite range")]
    IntegerRange,
    #[error("invalid stored Lattice identifier: {0}")]
    Uuid(uuid::Error),
    #[error("invalid stored resource reference: {0}")]
    ResourceRef(#[from] lattice_resource::ResourceRefError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_store_detects_and_reads_content() -> Result<(), Box<dyn std::error::Error>> {
        let temporary = tempfile::tempdir()?;
        let layout = CacheLayout::initialize(temporary.path())?;
        let store = layout.object_store();
        let metadata = store.put(b"return 42")?;
        assert_eq!(store.get(metadata.hash)?, b"return 42");
        assert!(metadata.path.exists());
        Ok(())
    }

    #[test]
    fn unchanged_source_does_not_advance_revision() -> Result<(), Box<dyn std::error::Error>> {
        let temporary = tempfile::tempdir()?;
        let layout = CacheLayout::initialize(temporary.path())?;
        let mut database = Database::open(&layout.database_path())?;
        let workspace = database.open_workspace(temporary.path())?;
        let object = layout.object_store().put(b"local value = 1")?;
        let analysis = lattice_luau::analyze("local value = 1");
        let first =
            database.record_source(&workspace, "value.luau", "value", &object, &analysis)?;
        let second =
            database.record_source(&workspace, "value.luau", "value", &object, &analysis)?;
        assert!(first.changed);
        assert!(!second.changed);
        assert_eq!(first.revision, second.revision);
        assert_eq!(first.resource_ref, second.resource_ref);
        Ok(())
    }

    #[test]
    fn tool_fabric_migration_is_applied_idempotently() -> Result<(), Box<dyn std::error::Error>> {
        let temporary = tempfile::tempdir()?;
        let path = temporary.path().join("lattice.sqlite");
        let first = Database::open(&path)?;
        assert_eq!(first.schema_version()?, 2);
        drop(first);
        let second = Database::open(&path)?;
        assert_eq!(second.schema_version()?, 2);
        Ok(())
    }
}
