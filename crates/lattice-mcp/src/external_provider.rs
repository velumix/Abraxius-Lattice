use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::Stdio,
    sync::RwLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use lattice_connections::{
    ProviderCallResult, ProviderConnection, ProviderError, ProviderFuture, ToolProvider,
};
use lattice_resource::LatticeId;
use lattice_tools::{
    AuthenticationState, ImportedTool, OperationSemantics, ProviderConnectionState,
    ProviderDescriptor, ProviderHealth, ProviderHealthStatus, ProviderId, ProviderKind,
    ProviderMetadata, ProviderTransportKind, ProviderTrust, ReportedSemantics, ToolId, ToolTrust,
};
use rmcp::{
    ClientServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation},
    service::{RoleClient, RunningService},
    transport::async_rw::AsyncRwTransport,
};

use crate::protocol::{McpNegotiation, automatic_lifecycle};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    sync::Mutex,
    task::JoinHandle,
};

const STDERR_CAPTURE_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ExternalMcpProviderConfig {
    pub stable_key: String,
    pub name: String,
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub working_directory: Option<PathBuf>,
    pub trust: ProviderTrust,
    pub shutdown_timeout: Duration,
}

struct ExternalState {
    service: RunningService<RoleClient, ClientInfo>,
    child: Child,
    stderr_task: JoinHandle<()>,
    negotiation: McpNegotiation,
}

pub struct ExternalMcpProvider {
    config: ExternalMcpProviderConfig,
    descriptor: RwLock<ProviderDescriptor>,
    state: Mutex<Option<ExternalState>>,
}

impl ExternalMcpProvider {
    pub fn new(config: ExternalMcpProviderConfig) -> Result<Self, ProviderError> {
        if config.stable_key.trim().is_empty() || config.name.trim().is_empty() {
            return Err(ProviderError::Connection(
                "provider stable key and name must not be empty".into(),
            ));
        }
        if !config.executable.is_absolute() {
            return Err(ProviderError::Connection(
                "stdio provider executable must be an absolute path".into(),
            ));
        }
        if !config.executable.is_file() {
            return Err(ProviderError::Connection(format!(
                "stdio provider executable does not exist: {}",
                config.executable.display()
            )));
        }
        if config
            .environment
            .keys()
            .any(|name| name.is_empty() || name.contains('=') || name.contains('\0'))
        {
            return Err(ProviderError::Connection(
                "provider environment contains an invalid variable name".into(),
            ));
        }
        let id = ProviderId::from_stable_key(config.stable_key.as_bytes());
        let descriptor = ProviderDescriptor {
            id,
            kind: ProviderKind::McpStdio,
            name: config.name.clone(),
            version: None,
            trust: config.trust,
            transport: ProviderTransportKind::Stdio,
            health: ProviderHealth {
                status: ProviderHealthStatus::Unavailable,
                reason: "configured; lazy connection has not been opened".into(),
                last_successful_operation_unix_ms: None,
                last_failure: None,
                rtt_micros: None,
                consecutive_failures: 0,
                connected_at_unix_ms: None,
                catalog_revision: None,
                tool_count: 0,
                resource_count: 0,
                authentication: AuthenticationState::Unknown,
                connection_state: ProviderConnectionState::Configured,
            },
            metadata: ProviderMetadata {
                source: "user-configured:mcp-stdio".into(),
                studio_environment_id: None,
                studio_session_id: None,
                configured_priority: None,
                protocol: None,
            },
        };
        Ok(Self { config, descriptor: RwLock::new(descriptor), state: Mutex::new(None) })
    }

    pub async fn protocol_status(&self) -> Option<McpNegotiation> {
        self.state.lock().await.as_ref().map(|active| active.negotiation.clone())
    }

    fn provider_id(&self) -> ProviderId {
        self.descriptor.read().unwrap_or_else(std::sync::PoisonError::into_inner).id
    }

    async fn discover_tools(&self) -> Result<Vec<ImportedTool>, ProviderError> {
        let state = self.state.lock().await;
        let state = state
            .as_ref()
            .ok_or_else(|| ProviderError::Connection("provider is disconnected".into()))?;
        let tools = state
            .service
            .list_all_tools()
            .await
            .map_err(|error| ProviderError::Protocol(error.to_string()))?;
        tools
            .into_iter()
            .map(|tool| {
                let reported = tool.annotations.as_ref();
                Ok(ImportedTool {
                    native_name: tool.name.into_owned(),
                    title: tool.title,
                    provider_description: tool.description.map(std::borrow::Cow::into_owned),
                    input_schema: serde_json::Value::Object((*tool.input_schema).clone()),
                    output_schema: tool
                        .output_schema
                        .map(|schema| serde_json::Value::Object((*schema).clone())),
                    capabilities: Vec::new(),
                    verified_semantics: OperationSemantics::unknown(),
                    reported_semantics: ReportedSemantics {
                        read_only_hint: reported.and_then(|value| value.read_only_hint),
                        destructive_hint: reported.and_then(|value| value.destructive_hint),
                        idempotent_hint: reported.and_then(|value| value.idempotent_hint),
                        open_world_hint: reported.and_then(|value| value.open_world_hint),
                    },
                    trust: match self.config.trust {
                        ProviderTrust::BuiltIn => ToolTrust::BuiltIn,
                        ProviderTrust::Verified => ToolTrust::Verified,
                        ProviderTrust::UserConfigured => ToolTrust::UserConfigured,
                        ProviderTrust::Untrusted => ToolTrust::Untrusted,
                    },
                })
            })
            .collect()
    }
}

impl ToolProvider for ExternalMcpProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        self.descriptor.read().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    fn connect(&self) -> ProviderFuture<'_, ProviderConnection> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if state.is_some() {
                return Err(ProviderError::Connection("provider is already connected".into()));
            }
            let mut command = Command::new(&self.config.executable);
            command
                .args(&self.config.arguments)
                .env_clear()
                .envs(&self.config.environment)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            if let Some(directory) = &self.config.working_directory {
                command.current_dir(directory);
            }
            let mut child =
                command.spawn().map_err(|error| ProviderError::Connection(error.to_string()))?;
            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| ProviderError::Connection("provider stdin unavailable".into()))?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| ProviderError::Connection("provider stdout unavailable".into()))?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| ProviderError::Connection("provider stderr unavailable".into()))?;
            let stderr_task = tokio::spawn(drain_stderr(stderr));
            let transport = AsyncRwTransport::new_client(stdout, stdin);
            let client = ClientInfo::new(
                ClientCapabilities::default(),
                Implementation::new("abraxius-lattice-provider-client", env!("CARGO_PKG_VERSION")),
            );
            let service = client
                .serve_with_lifecycle(transport, automatic_lifecycle())
                .await
                .map_err(|error| ProviderError::Protocol(error.to_string()))?;
            let peer_info = service.peer_info().ok_or_else(|| {
                ProviderError::Protocol("provider startup returned no negotiated peer state".into())
            })?;
            let negotiation = McpNegotiation::from_negotiated_version(&peer_info.protocol_version);
            let provider_id = self.provider_id();
            {
                let mut descriptor =
                    self.descriptor.write().unwrap_or_else(std::sync::PoisonError::into_inner);
                descriptor.metadata.protocol = Some(negotiation.provider_metadata());
                descriptor.version =
                    peer_info.server_info.as_ref().map(|server| server.version.clone());
            }
            let connection = ProviderConnection {
                id: LatticeId::new(),
                provider_id,
                endpoint_identity: format!("stdio:{provider_id}"),
                connected_at_unix_ms: unix_time_ms(),
            };
            *state = Some(ExternalState { service, child, stderr_task, negotiation });
            Ok(connection)
        })
    }

    fn disconnect(&self) -> ProviderFuture<'_, ()> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let Some(mut active) = state.take() else {
                return Ok(());
            };
            let _quit = active
                .service
                .close_with_timeout(self.config.shutdown_timeout)
                .await
                .map_err(|error| ProviderError::Protocol(error.to_string()))?;
            if tokio::time::timeout(self.config.shutdown_timeout, active.child.wait())
                .await
                .is_err()
            {
                active
                    .child
                    .kill()
                    .await
                    .map_err(|error| ProviderError::Connection(error.to_string()))?;
            }
            active.stderr_task.abort();
            Ok(())
        })
    }

    fn list_tools(&self) -> ProviderFuture<'_, Vec<ImportedTool>> {
        Box::pin(self.discover_tools())
    }

    fn call(
        &self,
        tool_id: ToolId,
        input: serde_json::Value,
    ) -> ProviderFuture<'_, ProviderCallResult> {
        Box::pin(async move {
            let tools = self.discover_tools().await?;
            let native_name = tools
                .iter()
                .find(|tool| {
                    ToolId::for_native_name(self.provider_id(), &tool.native_name) == tool_id
                })
                .map(|tool| tool.native_name.clone())
                .ok_or(ProviderError::ToolNotFound)?;
            let serde_json::Value::Object(arguments) = input else {
                return Err(ProviderError::Protocol("tool arguments must be an object".into()));
            };
            let state = self.state.lock().await;
            let active = state
                .as_ref()
                .ok_or_else(|| ProviderError::Connection("provider is disconnected".into()))?;
            let result = active
                .service
                .call_tool(CallToolRequestParams::new(native_name).with_arguments(arguments))
                .await
                .map_err(|error| ProviderError::Protocol(error.to_string()))?;
            Ok(ProviderCallResult {
                structured: serde_json::to_value(result)
                    .map_err(|error| ProviderError::Protocol(error.to_string()))?,
                content_type: "application/json".into(),
                cancellation_supported: true,
            })
        })
    }
}

async fn drain_stderr(mut stderr: tokio::process::ChildStderr) {
    let mut buffer = [0_u8; 8 * 1024];
    let mut retained = 0_usize;
    loop {
        let Ok(read) = stderr.read(&mut buffer).await else {
            return;
        };
        if read == 0 {
            return;
        }
        retained = retained.saturating_add(read).min(STDERR_CAPTURE_LIMIT);
    }
}

fn unix_time_ms() -> i64 {
    let millis =
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| duration.as_millis());
    i64::try_from(millis).unwrap_or(i64::MAX)
}
