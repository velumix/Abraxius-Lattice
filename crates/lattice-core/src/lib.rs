//! Authoritative, protocol-neutral Lattice services.

use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use lattice_graph::{GraphNode, ProjectGraph};
use lattice_luau::LuauAnalysis;
use lattice_model::{Confidence, Evidence, EvidenceOrigin, SearchHit};
use lattice_resource::{ContentHash, LatticeId};
use lattice_search::{IndexDocument, SourceIndex};
use lattice_storage::{CacheLayout, Database, StorageError, WorkspaceRecord};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{Semaphore, broadcast};

const EVENT_CAPACITY: usize = 512;
const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;

pub struct Lattice {
    layout: CacheLayout,
    database: Database,
    index: SourceIndex,
    workspace: WorkspaceRecord,
    graph: ProjectGraph,
    events: EventBus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceStatus {
    pub workspace_id: LatticeId,
    pub name: String,
    pub root: PathBuf,
    pub revision: u64,
    pub source_count: u64,
    pub graph_nodes: usize,
    pub cache_root: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IngestReport {
    pub workspace_id: LatticeId,
    pub discovered_sources: u64,
    pub changed_sources: u64,
    pub parse_diagnostics: u64,
    pub workspace_revision: u64,
}

impl Lattice {
    pub fn open(root: &Path) -> Result<Self, CoreError> {
        if !root.is_dir() {
            return Err(CoreError::WorkspaceNotFound(root.to_path_buf()));
        }
        let root = root.canonicalize()?;
        let layout = CacheLayout::initialize(&root)?;
        let mut database = Database::open(&layout.database_path())?;
        let workspace = database.open_workspace(&root)?;
        let index = SourceIndex::open(&layout.index_path())?;
        Ok(Self {
            layout,
            database,
            index,
            workspace,
            graph: ProjectGraph::default(),
            events: EventBus::new(),
        })
    }

    pub fn ingest(&mut self) -> Result<IngestReport, CoreError> {
        let mut files = Vec::new();
        collect_luau_files(&self.workspace.root_path, &self.workspace.root_path, &mut files)?;
        files.sort();
        let mut changed_sources = 0_u64;
        let mut parse_diagnostics = 0_u64;
        let rebuild_index = self.index.is_empty();

        for path in &files {
            let bytes = std::fs::read(path)?;
            let source = std::str::from_utf8(&bytes)
                .map_err(|error| CoreError::InvalidSourceEncoding(path.clone(), error))?;
            let relative = normalized_relative_path(&self.workspace.root_path, path)?;
            let name = path.file_stem().and_then(|value| value.to_str()).unwrap_or("script");
            let analysis = lattice_luau::analyze(source);
            parse_diagnostics = parse_diagnostics.saturating_add(analysis.diagnostics.len() as u64);
            let object = self.layout.object_store().put(&bytes)?;
            let stored = self.database.record_source(
                &self.workspace,
                &relative,
                name,
                &object,
                &analysis,
            )?;
            if stored.changed {
                changed_sources = changed_sources.saturating_add(1);
                self.events.publish(LatticeEvent::SourceChanged {
                    resource_ref: stored.resource_ref.to_string(),
                    revision: stored.revision,
                    content_hash: stored.content_hash,
                });
            }

            let symbols = symbol_text(&analysis);
            if stored.changed || rebuild_index {
                self.index.upsert(&IndexDocument {
                    resource_ref: &stored.resource_ref,
                    path: &stored.relative_path,
                    name: &stored.name,
                    source,
                    symbols: &symbols,
                    content_hash: stored.content_hash,
                })?;
            }
            self.graph
                .upsert_node(GraphNode { resource_ref: stored.resource_ref, name: stored.name });
        }

        let revision = self.database.current_workspace_revision(self.workspace.id)?;
        self.workspace.revision = revision;
        self.events.publish(LatticeEvent::IndexUpdated {
            workspace_id: self.workspace.id,
            revision,
            changed_sources,
        });
        Ok(IngestReport {
            workspace_id: self.workspace.id,
            discovered_sources: files.len() as u64,
            changed_sources,
            parse_diagnostics,
            workspace_revision: revision,
        })
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, CoreError> {
        let revision = self.workspace.revision;
        self.index
            .search(query, limit)?
            .into_iter()
            .map(|hit| {
                let evidence = Evidence {
                    id: LatticeId::new(),
                    kind: "full_text_match".to_owned(),
                    resource_ref: hit.resource_ref.clone(),
                    source_span: None,
                    revision,
                    origin: EvidenceOrigin::TextMatch,
                    confidence: Confidence::Certain,
                    payload_hash: hit.content_hash,
                };
                Ok(SearchHit {
                    resource_ref: hit.resource_ref,
                    display_path: hit.path,
                    name: hit.name,
                    score_milli: hit.score_milli,
                    content_hash: hit.content_hash,
                    evidence,
                })
            })
            .collect()
    }

    pub fn status(&self) -> Result<WorkspaceStatus, CoreError> {
        Ok(WorkspaceStatus {
            workspace_id: self.workspace.id,
            name: self.workspace.name.clone(),
            root: self.workspace.root_path.clone(),
            revision: self.database.current_workspace_revision(self.workspace.id)?,
            source_count: self.database.source_count(self.workspace.id)?,
            graph_nodes: usize::try_from(self.database.graph_node_count(self.workspace.id)?)
                .map_err(|_| CoreError::CountRange)?,
            cache_root: self.layout.root().to_path_buf(),
        })
    }

    #[must_use]
    pub fn events(&self) -> EventBus {
        self.events.clone()
    }
}

fn collect_luau_files(
    root: &Path,
    directory: &Path,
    output: &mut Vec<PathBuf>,
) -> Result<(), CoreError> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = entry.file_name();
            if matches!(name.to_str(), Some(".git" | ".lattice" | "target" | "bin" | "obj")) {
                continue;
            }
            collect_luau_files(root, &path, output)?;
        } else if file_type.is_file() && is_luau_source(&path) {
            let byte_len = entry.metadata()?.len();
            if byte_len > MAX_SOURCE_BYTES {
                tracing::warn!(path = %path.display(), byte_len, "skipping source over bounded ingestion size");
                continue;
            }
            if !path.starts_with(root) {
                return Err(CoreError::PathEscapesWorkspace(path));
            }
            output.push(path);
        }
    }
    Ok(())
}

fn is_luau_source(path: &Path) -> bool {
    path.extension().and_then(|value| value.to_str()).is_some_and(|extension| {
        extension.eq_ignore_ascii_case("luau") || extension.eq_ignore_ascii_case("lua")
    })
}

fn normalized_relative_path(root: &Path, path: &Path) -> Result<String, CoreError> {
    let relative =
        path.strip_prefix(root).map_err(|_| CoreError::PathEscapesWorkspace(path.to_path_buf()))?;
    Ok(relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn symbol_text(analysis: &LuauAnalysis) -> String {
    analysis.symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>().join(" ")
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LatticeEvent {
    WorkspaceOpened { workspace_id: LatticeId },
    SourceChanged { resource_ref: String, revision: u64, content_hash: ContentHash },
    IndexUpdated { workspace_id: LatticeId, revision: u64, changed_sources: u64 },
    JobChanged { job_id: LatticeId, state: JobState },
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<LatticeEvent>,
}

impl EventBus {
    #[must_use]
    pub fn new() -> Self {
        let (sender, _receiver) = broadcast::channel(EVENT_CAPACITY);
        Self { sender }
    }

    pub fn publish(&self, event: LatticeEvent) {
        let _receiver_count = self.sender.send(event);
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<LatticeEvent> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub struct JobSystem {
    permits: Arc<Semaphore>,
    events: EventBus,
}

impl JobSystem {
    #[must_use]
    pub fn new(max_concurrency: usize, events: EventBus) -> Self {
        Self { permits: Arc::new(Semaphore::new(max_concurrency.max(1))), events }
    }

    pub fn spawn<F>(&self, task: F) -> (LatticeId, CancellationToken)
    where
        F: Future<Output = Result<(), CoreError>> + Send + 'static,
    {
        let id = LatticeId::new();
        let token = CancellationToken::default();
        let task_token = token.clone();
        let permits = Arc::clone(&self.permits);
        let events = self.events.clone();
        events.publish(LatticeEvent::JobChanged { job_id: id, state: JobState::Queued });
        tokio::spawn(async move {
            let Ok(_permit) = permits.acquire_owned().await else {
                events.publish(LatticeEvent::JobChanged { job_id: id, state: JobState::Failed });
                return;
            };
            if task_token.is_cancelled() {
                events.publish(LatticeEvent::JobChanged { job_id: id, state: JobState::Cancelled });
                return;
            }
            events.publish(LatticeEvent::JobChanged { job_id: id, state: JobState::Running });
            let state = match task.await {
                Ok(()) if task_token.is_cancelled() => JobState::Cancelled,
                Ok(()) => JobState::Succeeded,
                Err(error) => {
                    tracing::error!(job_id = %id, error = %error, "background job failed");
                    JobState::Failed
                }
            };
            events.publish(LatticeEvent::JobChanged { job_id: id, state });
        });
        (id, token)
    }
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("WORKSPACE_NOT_FOUND: {0}")]
    WorkspaceNotFound(PathBuf),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("search error: {0}")]
    Search(#[from] lattice_search::SearchError),
    #[error("SOURCE_PARSE_FAILED: {0} is not UTF-8: {1}")]
    InvalidSourceEncoding(PathBuf, std::str::Utf8Error),
    #[error("path escapes the workspace root: {0}")]
    PathEscapesWorkspace(PathBuf),
    #[error("stored count exceeds this platform's addressable range")]
    CountRange,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_vertical_slice_indexes_real_luau() -> Result<(), Box<dyn std::error::Error>> {
        let temporary = tempfile::tempdir()?;
        std::fs::write(
            temporary.path().join("Inventory.luau"),
            "local Inventory = {}\nfunction Inventory.grant(player) return player end\nreturn Inventory\n",
        )?;
        let mut lattice = Lattice::open(temporary.path())?;
        let first = lattice.ingest()?;
        assert_eq!(first.discovered_sources, 1);
        assert_eq!(first.changed_sources, 1);
        assert_eq!(lattice.search("grant", 10)?.len(), 1);
        let second = lattice.ingest()?;
        assert_eq!(second.changed_sources, 0);
        std::fs::write(
            temporary.path().join("Inventory.luau"),
            "local Inventory = {}\nfunction Inventory.award(player) return player end\nreturn Inventory\n",
        )?;
        let third = lattice.ingest()?;
        assert_eq!(third.changed_sources, 1);
        assert!(lattice.search("grant", 10)?.is_empty());
        assert_eq!(lattice.search("award", 10)?.len(), 1);
        Ok(())
    }
}
