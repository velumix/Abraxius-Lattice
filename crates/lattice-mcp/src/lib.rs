//! MCP 2026 adapter. RMCP types are contained entirely in this crate.

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use lattice_core::{CoreError, Lattice};
use lattice_daemon_ipc::DaemonStream;
use rmcp::{
    Json, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod external_provider;
mod protocol;
mod studio_client;
mod studio_launcher;

pub use external_provider::{ExternalMcpProvider, ExternalMcpProviderConfig};
pub use lattice_daemon_ipc::{DaemonEndpointInfo, DaemonIpcError, DaemonListener};
pub use protocol::{
    McpCatalogChangeModel, McpNegotiation, McpNegotiationPath, McpProtocolFeatures,
    McpProtocolProfile, McpSessionModel, McpStartupMode,
};
pub use studio_client::{
    StudioMcpCancellation, StudioMcpClient, StudioMcpClientError, StudioMcpSessionBinding,
};
pub use studio_launcher::{
    LaunchedStudioMcpClient, ResolvedStudioMcpProcessLauncher, StudioMcpLaunchError,
    StudioMcpProcessLauncher,
};

const DEFAULT_SEARCH_LIMIT: usize = 10;
const MAX_INLINE_SEARCH_RESULTS: usize = 50;

/// Protocol-neutral service contract implemented by the Lattice core façade.
pub trait LatticeOperations: Send + Sync {
    fn search(&self, request: SearchRequest) -> Result<SearchResponse, ProtocolError>;
    fn workspace_status(&self) -> Result<WorkspaceStatusResponse, ProtocolError>;
    fn provider_list(&self) -> Result<ProviderListResponse, ProtocolError>;
    fn tool_search(&self, request: ToolSearchRequest) -> Result<ToolSearchResponse, ProtocolError>;
    fn tool_inspect(
        &self,
        request: ToolInspectRequest,
    ) -> Result<ToolInspectResponse, ProtocolError>;
}

/// Northbound protocol server boundary. Transports implement this, not core.
pub trait LatticeProtocolServer: Send + Sync {
    fn operations(&self) -> &dyn LatticeOperations;
}

/// Southbound protocol client boundary for adapters such as Studio. Concrete
/// RMCP request/response types stay private to this crate.
pub trait LatticeProtocolClient: Send + Sync {
    fn connection(&self) -> &lattice_studio::StudioMcpConnectionSnapshot;
}

#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchRequest {
    #[schemars(description = "Exact name, symbol, path, or source terms to find")]
    pub query: String,
    #[schemars(description = "Maximum inline hits; values are bounded to 1..50")]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct SearchResponse {
    pub query: String,
    pub hits: Vec<SearchHitResponse>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct SearchHitResponse {
    pub resource_ref: String,
    pub display_path: String,
    pub name: String,
    pub score_milli: i64,
    pub content_hash: String,
    pub evidence_id: String,
    pub evidence_origin: String,
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct WorkspaceStatusResponse {
    pub workspace_id: String,
    pub name: String,
    pub root: String,
    pub revision: u64,
    pub source_count: u64,
    pub graph_nodes: usize,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct CapabilitiesResponse {
    pub protocol: String,
    pub operations: Vec<String>,
    pub result_resource_threshold_bytes: u64,
    pub mutation_enabled: bool,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ProviderListResponse {
    pub providers: Vec<ProviderSummaryResponse>,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ProviderSummaryResponse {
    pub provider_id: String,
    pub name: String,
    pub kind: String,
    pub connection_state: String,
    pub health: String,
    pub reason: String,
    pub tool_count: u64,
}

#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolSearchRequest {
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolSearchResponse {
    pub query: String,
    pub tools: Vec<ToolSummaryResponse>,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolSummaryResponse {
    pub tool_ref: String,
    pub native_name: String,
    pub title: Option<String>,
    pub provider_id: String,
    pub capabilities: Vec<String>,
    pub availability: String,
    pub trust: String,
}

#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolInspectRequest {
    pub tool_ref: String,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolInspectResponse {
    pub tool_ref: String,
    pub provider_id: String,
    pub native_name: String,
    pub title: Option<String>,
    pub provider_description: Option<String>,
    pub input_schema_revision: String,
    pub input_schema: serde_json::Value,
    pub output_schema_revision: Option<String>,
    pub output_schema: Option<serde_json::Value>,
    pub capabilities: Vec<String>,
    pub verified_semantics: serde_json::Value,
    pub reported_semantics: serde_json::Value,
    pub availability: String,
    pub trust: String,
}

/// Stable documentation metadata for the compact northbound broker surface.
/// The schemas are generated from the same request/response types used by the
/// RMCP handlers, so reference docs cannot drift from their wire contracts.
#[derive(Clone, Debug, Serialize)]
pub struct NorthboundToolDocumentation {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
}

/// Return the northbound tool contract without starting a transport.
pub fn northbound_tool_documentation() -> Result<Vec<NorthboundToolDocumentation>, serde_json::Error>
{
    Ok(vec![
        NorthboundToolDocumentation {
            name: "lattice.search".to_owned(),
            description: "Search the canonical Roblox workspace and return compact stable rbx:// references and evidence IDs.".to_owned(),
            input_schema: serde_json::to_value(rmcp::schemars::schema_for!(SearchRequest))?,
            output_schema: serde_json::to_value(rmcp::schemars::schema_for!(SearchResponse))?,
        },
        NorthboundToolDocumentation {
            name: "lattice.workspace.status".to_owned(),
            description: "Return the active canonical workspace identity, revision, and indexed source count.".to_owned(),
            input_schema: serde_json::json!({"type":"object","additionalProperties":false}),
            output_schema: serde_json::to_value(rmcp::schemars::schema_for!(WorkspaceStatusResponse))?,
        },
        NorthboundToolDocumentation {
            name: "lattice.provider.list".to_owned(),
            description: "List configured providers with compact health and connection state; does not connect them.".to_owned(),
            input_schema: serde_json::json!({"type":"object","additionalProperties":false}),
            output_schema: serde_json::to_value(rmcp::schemars::schema_for!(ProviderListResponse))?,
        },
        NorthboundToolDocumentation {
            name: "lattice.tool.search".to_owned(),
            description: "Search the unified tool catalog and return compact references without loading full schemas.".to_owned(),
            input_schema: serde_json::to_value(rmcp::schemars::schema_for!(ToolSearchRequest))?,
            output_schema: serde_json::to_value(rmcp::schemars::schema_for!(ToolSearchResponse))?,
        },
        NorthboundToolDocumentation {
            name: "lattice.tool.inspect".to_owned(),
            description: "Inspect one exact lattice:// provider tool reference and load its schema and safety metadata.".to_owned(),
            input_schema: serde_json::to_value(rmcp::schemars::schema_for!(ToolInspectRequest))?,
            output_schema: serde_json::to_value(rmcp::schemars::schema_for!(ToolInspectResponse))?,
        },
        NorthboundToolDocumentation {
            name: "lattice.capabilities".to_owned(),
            description: "Discover the semantic Lattice operations enabled by this server.".to_owned(),
            input_schema: serde_json::json!({"type":"object","additionalProperties":false}),
            output_schema: serde_json::to_value(rmcp::schemars::schema_for!(CapabilitiesResponse))?,
        },
    ])
}

pub struct LocalOperations {
    lattice: Mutex<Lattice>,
    tool_fabric: lattice_tools::BuiltinToolFabric,
}

impl LocalOperations {
    pub fn open(root: &Path) -> Result<Self, ProtocolError> {
        let mut lattice = Lattice::open(root)?;
        lattice.ingest()?;
        let platform = lattice_platform::PlatformResolver::current()
            .inspect()
            .map_err(|error| ProtocolError::Platform(error.to_string()))?;
        let tool_fabric =
            lattice_tools::BuiltinToolFabric::from_environments(&platform.environments)
                .map_err(ProtocolError::ToolFabric)?;
        Ok(Self { lattice: Mutex::new(lattice), tool_fabric })
    }
}

impl LatticeOperations for LocalOperations {
    fn search(&self, request: SearchRequest) -> Result<SearchResponse, ProtocolError> {
        if request.query.trim().is_empty() {
            return Err(ProtocolError::InvalidInput("query must not be empty".to_owned()));
        }
        let requested_limit = request.limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
        let limit = requested_limit.clamp(1, MAX_INLINE_SEARCH_RESULTS);
        let lattice = self.lattice.lock().map_err(|_| ProtocolError::StatePoisoned)?;
        let hits = lattice
            .search(&request.query, limit)?
            .into_iter()
            .map(|hit| SearchHitResponse {
                resource_ref: hit.resource_ref.to_string(),
                display_path: hit.display_path,
                name: hit.name,
                score_milli: hit.score_milli,
                content_hash: hit.content_hash.to_string(),
                evidence_id: hit.evidence.id.to_string(),
                evidence_origin: "TextMatch".to_owned(),
                revision: hit.evidence.revision,
            })
            .collect();
        Ok(SearchResponse { query: request.query, hits, truncated: requested_limit > limit })
    }

    fn workspace_status(&self) -> Result<WorkspaceStatusResponse, ProtocolError> {
        let lattice = self.lattice.lock().map_err(|_| ProtocolError::StatePoisoned)?;
        let status = lattice.status()?;
        Ok(WorkspaceStatusResponse {
            workspace_id: status.workspace_id.to_string(),
            name: status.name,
            root: status.root.to_string_lossy().into_owned(),
            revision: status.revision,
            source_count: status.source_count,
            graph_nodes: status.graph_nodes,
        })
    }

    fn provider_list(&self) -> Result<ProviderListResponse, ProtocolError> {
        let providers = self
            .tool_fabric
            .providers
            .list()
            .into_iter()
            .map(|provider| ProviderSummaryResponse {
                provider_id: provider.id.to_string(),
                name: provider.name.clone(),
                kind: format!("{:?}", provider.kind),
                connection_state: format!("{:?}", provider.health.connection_state),
                health: format!("{:?}", provider.health.status),
                reason: provider.health.reason.clone(),
                tool_count: provider.health.tool_count,
            })
            .collect();
        Ok(ProviderListResponse { providers })
    }

    fn tool_search(&self, request: ToolSearchRequest) -> Result<ToolSearchResponse, ProtocolError> {
        if request.query.trim().is_empty() {
            return Err(ProtocolError::InvalidInput("query must not be empty".into()));
        }
        let tools = self
            .tool_fabric
            .catalog
            .search(&request.query, request.limit.unwrap_or(10))
            .into_iter()
            .map(|tool| ToolSummaryResponse {
                tool_ref: tool.reference().to_string(),
                native_name: tool.native_name.clone(),
                title: tool.title.clone(),
                provider_id: tool.provider_id.to_string(),
                capabilities: tool
                    .capabilities
                    .iter()
                    .map(|binding| binding.capability.to_string())
                    .collect(),
                availability: format!("{:?}", tool.availability),
                trust: format!("{:?}", tool.trust),
            })
            .collect();
        Ok(ToolSearchResponse { query: request.query, tools })
    }

    fn tool_inspect(
        &self,
        request: ToolInspectRequest,
    ) -> Result<ToolInspectResponse, ProtocolError> {
        let reference = request
            .tool_ref
            .parse::<lattice_tools::ToolRef>()
            .map_err(ProtocolError::ToolFabric)?;
        let tool = self
            .tool_fabric
            .catalog
            .get(&reference)
            .ok_or_else(|| ProtocolError::ToolNotFound(request.tool_ref.clone()))?;
        let input = self
            .tool_fabric
            .catalog
            .schema(tool.input_schema)
            .ok_or_else(|| ProtocolError::ToolNotFound(request.tool_ref.clone()))?;
        let output =
            tool.output_schema.and_then(|revision| self.tool_fabric.catalog.schema(revision));
        Ok(ToolInspectResponse {
            tool_ref: reference.to_string(),
            provider_id: tool.provider_id.to_string(),
            native_name: tool.native_name.clone(),
            title: tool.title.clone(),
            provider_description: tool.provider_description.clone(),
            input_schema_revision: tool.input_schema.to_string(),
            input_schema: input.normalized_schema.clone(),
            output_schema_revision: tool.output_schema.map(|revision| revision.to_string()),
            output_schema: output.map(|schema| schema.normalized_schema.clone()),
            capabilities: tool
                .capabilities
                .iter()
                .map(|binding| binding.capability.to_string())
                .collect(),
            verified_semantics: serde_json::to_value(&tool.verified_semantics)?,
            reported_semantics: serde_json::to_value(&tool.reported_semantics)?,
            availability: format!("{:?}", tool.availability),
            trust: format!("{:?}", tool.trust),
        })
    }
}

#[derive(Clone)]
pub struct LatticeMcpServer {
    operations: Arc<dyn LatticeOperations>,
    tool_router: ToolRouter<Self>,
}

impl LatticeMcpServer {
    #[must_use]
    pub fn new(operations: Arc<dyn LatticeOperations>) -> Self {
        Self { operations, tool_router: Self::tool_router() }
    }
}

impl LatticeProtocolServer for LatticeMcpServer {
    fn operations(&self) -> &dyn LatticeOperations {
        self.operations.as_ref()
    }
}

#[tool_router]
impl LatticeMcpServer {
    #[tool(
        name = "lattice.search",
        description = "Search the canonical Roblox workspace. Returns compact stable rbx:// references and evidence IDs."
    )]
    async fn search(
        &self,
        Parameters(request): Parameters<SearchRequest>,
    ) -> Result<Json<SearchResponse>, String> {
        let operations = Arc::clone(&self.operations);
        tokio::task::spawn_blocking(move || operations.search(request))
            .await
            .map_err(|error| format!("INTERNAL: search worker failed: {error}"))?
            .map(Json)
            .map_err(|error| error.to_string())
    }

    #[tool(
        name = "lattice.workspace.status",
        description = "Return the active canonical workspace identity, revision, and indexed source count."
    )]
    async fn workspace_status(&self) -> Result<Json<WorkspaceStatusResponse>, String> {
        let operations = Arc::clone(&self.operations);
        tokio::task::spawn_blocking(move || operations.workspace_status())
            .await
            .map_err(|error| format!("INTERNAL: status worker failed: {error}"))?
            .map(Json)
            .map_err(|error| error.to_string())
    }

    #[tool(
        name = "lattice.provider.list",
        description = "List configured providers with compact health and connection state; does not connect them."
    )]
    async fn provider_list(&self) -> Result<Json<ProviderListResponse>, String> {
        let operations = Arc::clone(&self.operations);
        tokio::task::spawn_blocking(move || operations.provider_list())
            .await
            .map_err(|error| format!("INTERNAL: provider worker failed: {error}"))?
            .map(Json)
            .map_err(|error| error.to_string())
    }

    #[tool(
        name = "lattice.tool.search",
        description = "Search the unified tool catalog and return compact references without loading full schemas."
    )]
    async fn tool_search(
        &self,
        Parameters(request): Parameters<ToolSearchRequest>,
    ) -> Result<Json<ToolSearchResponse>, String> {
        let operations = Arc::clone(&self.operations);
        tokio::task::spawn_blocking(move || operations.tool_search(request))
            .await
            .map_err(|error| format!("INTERNAL: tool search worker failed: {error}"))?
            .map(Json)
            .map_err(|error| error.to_string())
    }

    #[tool(
        name = "lattice.tool.inspect",
        description = "Inspect one exact lattice:// provider tool reference and load its schema and safety metadata."
    )]
    async fn tool_inspect(
        &self,
        Parameters(request): Parameters<ToolInspectRequest>,
    ) -> Result<Json<ToolInspectResponse>, String> {
        let operations = Arc::clone(&self.operations);
        tokio::task::spawn_blocking(move || operations.tool_inspect(request))
            .await
            .map_err(|error| format!("INTERNAL: tool inspect worker failed: {error}"))?
            .map(Json)
            .map_err(|error| error.to_string())
    }

    #[tool(
        name = "lattice.capabilities",
        description = "Discover the semantic Lattice operations enabled by this server."
    )]
    #[allow(clippy::unused_self)] // RMCP tool handlers are instance methods by contract.
    fn capabilities(&self) -> Json<CapabilitiesResponse> {
        Json(CapabilitiesResponse {
            protocol: "MCP 2026-07-28 via rmcp 3.1.2".to_owned(),
            operations: vec![
                "lattice.workspace.status".to_owned(),
                "lattice.search".to_owned(),
                "lattice.capabilities".to_owned(),
                "lattice.provider.list".to_owned(),
                "lattice.tool.search".to_owned(),
                "lattice.tool.inspect".to_owned(),
            ],
            result_resource_threshold_bytes: 64 * 1024,
            mutation_enabled: false,
        })
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for LatticeMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("lattice", env!("CARGO_PKG_VERSION"))
                    .with_title("Abraxius Lattice")
                    .with_description("The Native Intelligence Layer for Roblox"),
            )
            .with_instructions(
                "Lattice provides structured Roblox development infrastructure: search, Studio operations, tools, resources, diagnostics, traces, Git, and related capabilities. Use bounded results and canonical rbx:// references; query capability availability instead of guessing.",
            )
    }
}

pub async fn serve_stdio(root: &Path) -> Result<(), McpServerError> {
    let operations = LocalOperations::open(root)?;
    let service = LatticeMcpServer::new(Arc::new(operations))
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|error| McpServerError::Initialize(error.to_string()))?;
    service.waiting().await?;
    Ok(())
}

/// Serves the authoritative daemon-owned MCP surface over the local IPC
/// listener. The daemon creates one `LocalOperations` instance and each
/// connection gets only a transport/server wrapper around that shared state.
pub async fn serve_daemon_ipc(
    listener: DaemonListener,
    operations: Arc<dyn LatticeOperations>,
) -> Result<(), McpServerError> {
    loop {
        let stream = listener.accept().await.map_err(McpServerError::DaemonIpc)?;
        let operations = Arc::clone(&operations);
        tokio::spawn(async move {
            if let Err(error) = serve_daemon_connection(stream, operations).await {
                tracing::debug!(%error, "local MCP client disconnected");
            }
        });
    }
}

/// Runs the Codex-facing thin stdio bridge. It forwards MCP bytes only; it
/// never opens a workspace, indexes files, or creates another Lattice core.
pub async fn serve_stdio_bridge() -> Result<(), McpServerError> {
    let stream = lattice_daemon_ipc::connect().await.map_err(McpServerError::DaemonIpc)?;
    let (mut daemon_read, mut daemon_write) = tokio::io::split(stream);
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut input = tokio::spawn(async move {
        let result = tokio::io::copy(&mut stdin, &mut daemon_write).await;
        let _ = tokio::io::AsyncWriteExt::shutdown(&mut daemon_write).await;
        result
    });
    let mut output = tokio::spawn(async move {
        let result = tokio::io::copy(&mut daemon_read, &mut stdout).await;
        let _ = tokio::io::AsyncWriteExt::flush(&mut stdout).await;
        result
    });
    tokio::select! {
        result = &mut input => {
            result
                .map_err(McpServerError::Join)?
                .map_err(|error| McpServerError::Io(error.to_string()))?;
            let _ = output.await;
        }
        result = &mut output => {
            result
                .map_err(McpServerError::Join)?
                .map_err(|error| McpServerError::Io(error.to_string()))?;
            input.abort();
        }
    }
    Ok(())
}

pub async fn serve_daemon_connection<S>(
    stream: S,
    operations: Arc<dyn LatticeOperations>,
) -> Result<(), McpServerError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    use rmcp::ServiceExt;
    let service = LatticeMcpServer::new(operations)
        .serve(stream)
        .await
        .map_err(|error| McpServerError::Initialize(error.to_string()))?;
    service.waiting().await.map_err(McpServerError::Join)?;
    Ok(())
}

pub async fn connect_daemon() -> Result<DaemonStream, DaemonIpcError> {
    lattice_daemon_ipc::connect().await
}

pub fn inspect_daemon() -> Result<Option<DaemonEndpointInfo>, DaemonIpcError> {
    lattice_daemon_ipc::inspect()
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("INVALID_INPUT: {0}")]
    InvalidInput(String),
    #[error("INTERNAL: shared Lattice state is poisoned")]
    StatePoisoned,
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("PLATFORM_RESOLUTION_FAILED: {0}")]
    Platform(String),
    #[error(transparent)]
    ToolFabric(#[from] lattice_tools::ToolFabricError),
    #[error("TOOL_NOT_FOUND: {0}")]
    ToolNotFound(String),
    #[error("TOOL_SCHEMA_INVALID: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Error)]
pub enum McpServerError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("MCP service error: {0}")]
    Service(#[from] rmcp::service::ServiceError),
    #[error("MCP initialization error: {0}")]
    Initialize(String),
    #[error("MCP transport task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("MCP transport I/O failed: {0}")]
    Io(String),
    #[error(transparent)]
    DaemonIpc(#[from] DaemonIpcError),
}
