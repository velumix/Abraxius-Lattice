use std::{
    collections::BTreeSet,
    error::Error as StdError,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use lattice_resource::LatticeId;
use lattice_studio::{
    StudioMcpConnectionSnapshot, StudioMcpConnectionState, StudioMcpToolDescriptor,
    StudioMcpToolResult, StudioMcpTransportKind,
};
use rmcp::{
    ClientHandler, ClientServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation},
    service::{RoleClient, RunningService},
    transport::IntoTransport,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::watch;

use crate::LatticeProtocolClient;
use crate::protocol::{
    McpNegotiation, McpNegotiationPath, McpSessionModel, McpStartupMode, lifecycle_for,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioMcpSessionBinding {
    pub studio_session_id: LatticeId,
    pub environment_id: lattice_platform::StudioEnvironmentId,
    pub process_id: u32,
}

/// Cloneable cancellation signal for one in-flight southbound request.
#[derive(Clone)]
pub struct StudioMcpCancellation {
    sender: watch::Sender<bool>,
}

impl Default for StudioMcpCancellation {
    fn default() -> Self {
        let (sender, _) = watch::channel(false);
        Self { sender }
    }
}

impl StudioMcpCancellation {
    pub fn cancel(&self) {
        self.sender.send_replace(true);
    }

    fn receiver(&self) -> watch::Receiver<bool> {
        self.sender.subscribe()
    }
}

#[derive(Clone, Debug)]
struct LatticeStudioClientHandler {
    info: ClientInfo,
}

impl ClientHandler for LatticeStudioClientHandler {
    fn get_info(&self) -> ClientInfo {
        self.info.clone()
    }
}

/// One initialized, session-bound southbound connection. The transport is
/// supplied by a verified endpoint resolver; this type never launches Studio.
pub struct StudioMcpClient {
    service: RunningService<RoleClient, LatticeStudioClientHandler>,
    snapshot: StudioMcpConnectionSnapshot,
    request_timeout: Duration,
}

impl StudioMcpClient {
    /// Initializes an MCP client over an already-open transport, negotiates
    /// capabilities, and retrieves the complete tool catalog.
    pub async fn connect<T, E, A>(
        transport: T,
        binding: StudioMcpSessionBinding,
        transport_kind: StudioMcpTransportKind,
        endpoint_identity: impl Into<String>,
        request_timeout: Duration,
    ) -> Result<Self, StudioMcpClientError>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: StdError + Send + Sync + 'static,
    {
        Self::connect_with_startup(
            transport,
            binding,
            transport_kind,
            endpoint_identity,
            request_timeout,
            McpStartupMode::Automatic,
        )
        .await
    }

    pub async fn connect_with_startup<T, E, A>(
        transport: T,
        binding: StudioMcpSessionBinding,
        transport_kind: StudioMcpTransportKind,
        endpoint_identity: impl Into<String>,
        request_timeout: Duration,
        startup: McpStartupMode,
    ) -> Result<Self, StudioMcpClientError>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: StdError + Send + Sync + 'static,
    {
        let handler = LatticeStudioClientHandler {
            info: ClientInfo::new(
                ClientCapabilities::default(),
                Implementation::new("abraxius-lattice-studio-client", env!("CARGO_PKG_VERSION")),
            ),
        };
        let service = tokio::time::timeout(
            request_timeout,
            handler.serve_with_lifecycle(transport, lifecycle_for(&startup)),
        )
        .await
        .map_err(|_| StudioMcpClientError::InitializeTimeout(request_timeout))?
        .map_err(|error| StudioMcpClientError::Initialize(error.to_string()))?;
        let peer_info =
            service.peer_info().ok_or(StudioMcpClientError::MissingInitializationResult)?;
        let fallback_reason = match startup {
            McpStartupMode::Automatic => None,
            McpStartupMode::VerifiedLegacyFallback { reason } => Some(reason),
        };
        let negotiation = McpNegotiation::from_negotiated_version(&peer_info.protocol_version)
            .with_fallback_reason(fallback_reason);
        let tools = tokio::time::timeout(request_timeout, service.list_all_tools())
            .await
            .map_err(|_| StudioMcpClientError::ToolDiscoveryTimeout(request_timeout))?
            .map_err(StudioMcpClientError::Service)?;
        let tool_descriptors = tools
            .into_iter()
            .map(|tool| {
                let annotations = tool.annotations.as_ref();
                StudioMcpToolDescriptor {
                    name: tool.name.into_owned(),
                    description: tool.description.map(std::borrow::Cow::into_owned),
                    input_schema: serde_json::Value::Object((*tool.input_schema).clone()),
                    read_only_hint: annotations.and_then(|value| value.read_only_hint),
                    destructive_hint: annotations.and_then(|value| value.destructive_hint),
                }
            })
            .collect::<Vec<_>>();
        let catalog_bytes =
            serde_json::to_vec(&tool_descriptors).map_err(StudioMcpClientError::Serialize)?;
        let capabilities = capability_names(&peer_info.capabilities)?;
        let server_info = peer_info.server_info.as_ref();
        let snapshot = StudioMcpConnectionSnapshot {
            id: LatticeId::new(),
            studio_session_id: binding.studio_session_id,
            environment_id: binding.environment_id,
            process_id: binding.process_id,
            state: StudioMcpConnectionState::Connected,
            transport: transport_kind,
            endpoint_identity: endpoint_identity.into(),
            protocol_version: peer_info.protocol_version.to_string(),
            protocol_negotiation: match negotiation.path {
                McpNegotiationPath::ServerDiscover => "server/discover",
                McpNegotiationPath::InitializeFallback => "initialize fallback",
            }
            .into(),
            protocol_session_model: match negotiation.features.session_model {
                McpSessionModel::StatelessPerRequest => "stateless per request",
                McpSessionModel::LegacyConnectionSession => "legacy connection session",
            }
            .into(),
            protocol_fallback_reason: negotiation.fallback_reason,
            server_name: server_info.map_or_else(|| "unknown".into(), |info| info.name.clone()),
            server_version: server_info
                .map_or_else(|| "unknown".into(), |info| info.version.clone()),
            capabilities,
            tools: tool_descriptors,
            tool_catalog_revision: format!("b3:{}", blake3::hash(&catalog_bytes).to_hex()),
            connected_at_unix_ms: unix_time_ms(),
            last_successful_request_unix_ms: None,
            last_rtt_micros: None,
            failure_count: 0,
            last_error: None,
        };
        Ok(Self { service, snapshot, request_timeout })
    }

    #[must_use]
    pub fn snapshot(&self) -> &StudioMcpConnectionSnapshot {
        &self.snapshot
    }

    /// Calls one advertised Studio tool with a bounded timeout. Callers remain
    /// responsible for policy and for selecting read-only tools when required.
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<StudioMcpToolResult, StudioMcpClientError> {
        self.call_tool_cancellable(name, arguments, None).await
    }

    /// Equivalent to [`Self::call_tool`] with an explicit cancellation signal.
    pub async fn call_tool_cancellable(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
        cancellation: Option<&StudioMcpCancellation>,
    ) -> Result<StudioMcpToolResult, StudioMcpClientError> {
        if !self.snapshot.state.permits_requests() || self.service.is_closed() {
            return Err(StudioMcpClientError::Disconnected);
        }
        if !self.snapshot.tools.iter().any(|tool| tool.name == name) {
            return Err(StudioMcpClientError::ToolUnavailable(name.to_owned()));
        }
        let serde_json::Value::Object(arguments) = arguments else {
            return Err(StudioMcpClientError::ArgumentsMustBeObject);
        };
        let started = Instant::now();
        let request = tokio::time::timeout(
            self.request_timeout,
            self.service
                .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(arguments)),
        );
        let response = if let Some(cancellation) = cancellation {
            let mut receiver = cancellation.receiver();
            if *receiver.borrow() {
                return Err(StudioMcpClientError::RequestCancelled);
            }
            tokio::select! {
                result = request => result,
                changed = receiver.changed() => {
                    if changed.is_ok() && *receiver.borrow() {
                        return Err(StudioMcpClientError::RequestCancelled);
                    }
                    return Err(StudioMcpClientError::RequestCancelled);
                }
            }
        } else {
            request.await
        };
        let elapsed = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        match response {
            Ok(Ok(result)) => {
                let value =
                    serde_json::to_value(result).map_err(StudioMcpClientError::Serialize)?;
                self.snapshot.last_successful_request_unix_ms = Some(unix_time_ms());
                self.snapshot.last_rtt_micros = Some(elapsed);
                self.snapshot.last_error = None;
                Ok(StudioMcpToolResult { value, rtt_micros: elapsed })
            }
            Ok(Err(error)) => {
                self.record_failure(error.to_string());
                Err(StudioMcpClientError::Service(error))
            }
            Err(_) => {
                self.snapshot.state = StudioMcpConnectionState::Degraded;
                self.record_failure(format!("request timed out after {:?}", self.request_timeout));
                Err(StudioMcpClientError::RequestTimeout(self.request_timeout))
            }
        }
    }

    /// Bounded graceful disconnect. This never terminates the bound Studio
    /// process; it only closes the Lattice-owned transport.
    pub async fn disconnect(&mut self, timeout: Duration) -> Result<(), StudioMcpClientError> {
        let result =
            self.service.close_with_timeout(timeout).await.map_err(StudioMcpClientError::Join)?;
        if result.is_none() {
            self.snapshot.state = StudioMcpConnectionState::Failed;
            return Err(StudioMcpClientError::DisconnectTimeout(timeout));
        }
        self.snapshot.state = StudioMcpConnectionState::Disconnected;
        Ok(())
    }

    fn record_failure(&mut self, message: String) {
        self.snapshot.failure_count = self.snapshot.failure_count.saturating_add(1);
        self.snapshot.last_error = Some(message);
    }
}

impl LatticeProtocolClient for StudioMcpClient {
    fn connection(&self) -> &StudioMcpConnectionSnapshot {
        &self.snapshot
    }
}

fn capability_names(
    capabilities: &rmcp::model::ServerCapabilities,
) -> Result<Vec<String>, StudioMcpClientError> {
    let value = serde_json::to_value(capabilities).map_err(StudioMcpClientError::Serialize)?;
    let mut names = BTreeSet::new();
    if let serde_json::Value::Object(object) = value {
        for (name, value) in object {
            if !value.is_null() {
                names.insert(name);
            }
        }
    }
    Ok(names.into_iter().collect())
}

fn unix_time_ms() -> i64 {
    let millis =
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| duration.as_millis());
    i64::try_from(millis).unwrap_or(i64::MAX)
}

#[derive(Debug, Error)]
pub enum StudioMcpClientError {
    #[error("STUDIO_MCP_INITIALIZE_TIMEOUT: initialization exceeded {0:?}")]
    InitializeTimeout(Duration),
    #[error("STUDIO_MCP_INITIALIZE_FAILED: {0}")]
    Initialize(String),
    #[error("STUDIO_MCP_INVALID_HANDSHAKE: initialization returned no peer information")]
    MissingInitializationResult,
    #[error("STUDIO_MCP_TOOL_DISCOVERY_TIMEOUT: tool discovery exceeded {0:?}")]
    ToolDiscoveryTimeout(Duration),
    #[error("STUDIO_MCP_REQUEST_TIMEOUT: request exceeded {0:?}")]
    RequestTimeout(Duration),
    #[error("STUDIO_MCP_REQUEST_CANCELLED: request was cancelled")]
    RequestCancelled,
    #[error("STUDIO_MCP_DISCONNECT_TIMEOUT: disconnect exceeded {0:?}")]
    DisconnectTimeout(Duration),
    #[error("STUDIO_NOT_CONNECTED: the Studio MCP transport is closed")]
    Disconnected,
    #[error("STUDIO_CAPABILITY_UNAVAILABLE: tool {0} was not advertised")]
    ToolUnavailable(String),
    #[error("INVALID_INPUT: Studio MCP tool arguments must be a JSON object")]
    ArgumentsMustBeObject,
    #[error("STUDIO_MCP_SERVICE_ERROR: {0}")]
    Service(#[source] rmcp::service::ServiceError),
    #[error("STUDIO_MCP_SERIALIZATION_FAILED: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("STUDIO_MCP_TRANSPORT_TASK_FAILED: {0}")]
    Join(#[source] tokio::task::JoinError),
}

#[cfg(test)]
mod tests {
    use std::{error::Error, sync::Arc};

    use lattice_studio::StudioMcpConnectionState;
    use rmcp::ServiceExt;

    use super::*;
    use crate::{
        LatticeMcpServer, LatticeOperations, ProtocolError, ProviderListResponse, SearchRequest,
        SearchResponse, ToolInspectRequest, ToolInspectResponse, ToolSearchRequest,
        ToolSearchResponse, WorkspaceStatusResponse,
    };

    struct TestOperations;

    impl LatticeOperations for TestOperations {
        fn search(&self, request: SearchRequest) -> Result<SearchResponse, ProtocolError> {
            Ok(SearchResponse { query: request.query, hits: Vec::new(), truncated: false })
        }

        fn workspace_status(&self) -> Result<WorkspaceStatusResponse, ProtocolError> {
            Ok(WorkspaceStatusResponse {
                workspace_id: "workspace-test".into(),
                name: "test".into(),
                root: "/test".into(),
                revision: 1,
                source_count: 0,
                graph_nodes: 0,
            })
        }

        fn provider_list(&self) -> Result<ProviderListResponse, ProtocolError> {
            Ok(ProviderListResponse { providers: Vec::new() })
        }

        fn tool_search(
            &self,
            request: ToolSearchRequest,
        ) -> Result<ToolSearchResponse, ProtocolError> {
            Ok(ToolSearchResponse { query: request.query, tools: Vec::new() })
        }

        fn tool_inspect(
            &self,
            request: ToolInspectRequest,
        ) -> Result<ToolInspectResponse, ProtocolError> {
            Err(ProtocolError::ToolNotFound(request.tool_ref))
        }
    }

    async fn connected_pair()
    -> Result<(StudioMcpClient, tokio::task::JoinHandle<Result<(), String>>), Box<dyn Error>> {
        let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
        let server_task = tokio::spawn(async move {
            let service = LatticeMcpServer::new(Arc::new(TestOperations))
                .serve(server_transport)
                .await
                .map_err(|error| error.to_string())?;
            service.waiting().await.map_err(|error| error.to_string())?;
            Ok(())
        });
        let binding = StudioMcpSessionBinding {
            studio_session_id: LatticeId::new(),
            environment_id: lattice_platform::StudioEnvironmentId::from_fingerprint(b"test"),
            process_id: 77,
        };
        let client = StudioMcpClient::connect(
            client_transport,
            binding,
            StudioMcpTransportKind::Stdio,
            "test-duplex",
            Duration::from_secs(2),
        )
        .await?;
        Ok((client, server_task))
    }

    #[tokio::test]
    async fn initializes_discovers_calls_and_disconnects() -> Result<(), Box<dyn Error>> {
        let (mut client, server_task) = connected_pair().await?;
        assert_eq!(client.snapshot().state, StudioMcpConnectionState::Connected);
        assert_eq!(client.snapshot().process_id, 77);
        assert_eq!(client.snapshot().tools.len(), 6);
        assert!(client.snapshot().capabilities.iter().any(|value| value == "tools"));
        assert!(client.snapshot().tools.iter().any(|tool| tool.name == "lattice.capabilities"));
        assert!(client.snapshot().tools.iter().any(|tool| tool.name == "lattice.tool.search"));

        let result = client.call_tool("lattice.capabilities", serde_json::json!({})).await?;
        assert!(result.value.to_string().contains("lattice.search"));
        assert!(client.snapshot().last_successful_request_unix_ms.is_some());

        client.disconnect(Duration::from_secs(2)).await?;
        assert_eq!(client.snapshot().state, StudioMcpConnectionState::Disconnected);
        server_task.await.map_err(|error| Box::new(error) as Box<dyn Error>)??;
        Ok(())
    }

    #[tokio::test]
    async fn a_new_transport_reconnects_with_a_new_connection_identity()
    -> Result<(), Box<dyn Error>> {
        let (mut first, first_server) = connected_pair().await?;
        let first_id = first.snapshot().id;
        first.disconnect(Duration::from_secs(2)).await?;
        first_server.await.map_err(|error| Box::new(error) as Box<dyn Error>)??;

        let (mut second, second_server) = connected_pair().await?;
        assert_ne!(first_id, second.snapshot().id);
        assert_eq!(second.snapshot().state, StudioMcpConnectionState::Connected);
        second.disconnect(Duration::from_secs(2)).await?;
        second_server.await.map_err(|error| Box::new(error) as Box<dyn Error>)??;
        Ok(())
    }

    #[tokio::test]
    async fn pre_cancelled_request_never_reaches_the_server() -> Result<(), Box<dyn Error>> {
        let (mut client, server_task) = connected_pair().await?;
        let cancellation = StudioMcpCancellation::default();
        cancellation.cancel();
        let result = client
            .call_tool_cancellable(
                "lattice.capabilities",
                serde_json::json!({}),
                Some(&cancellation),
            )
            .await;
        assert!(matches!(result, Err(StudioMcpClientError::RequestCancelled)));
        assert!(client.snapshot().last_successful_request_unix_ms.is_none());
        client.disconnect(Duration::from_secs(2)).await?;
        server_task.await.map_err(|error| Box::new(error) as Box<dyn Error>)??;
        Ok(())
    }
}
