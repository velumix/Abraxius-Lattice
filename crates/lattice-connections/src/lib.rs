//! Lazy provider lifecycle, bounded execution, policy, and immutable results.

use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use lattice_resource::{ContentHash, LatticeId};
use lattice_tools::{
    ImportedTool, OperationId, ProviderConnectionState, ProviderDescriptor, ProviderHealthStatus,
    ProviderId, ProviderRegistry, SchemaRevision, ToolAvailability, ToolCatalog, ToolId, ToolRef,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock, Semaphore, watch};

pub type ProviderFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ProviderError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderConnection {
    pub id: LatticeId,
    pub provider_id: ProviderId,
    pub endpoint_identity: String,
    pub connected_at_unix_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProviderCallResult {
    pub structured: serde_json::Value,
    pub content_type: String,
    pub cancellation_supported: bool,
}

pub trait ToolProvider: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;
    fn connect(&self) -> ProviderFuture<'_, ProviderConnection>;
    fn disconnect(&self) -> ProviderFuture<'_, ()>;
    fn list_tools(&self) -> ProviderFuture<'_, Vec<ImportedTool>>;
    fn call(
        &self,
        tool_id: ToolId,
        input: serde_json::Value,
    ) -> ProviderFuture<'_, ProviderCallResult>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PolicyRequest {
    pub caller: String,
    pub provider_id: ProviderId,
    pub tool_ref: ToolRef,
    pub schema_revision: SchemaRevision,
    pub arguments_hash: ContentHash,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub reason: String,
}

pub trait PolicyEnforcer: Send + Sync {
    fn authorize(&self, request: &PolicyRequest) -> PolicyDecision;
}

pub struct DenyAllPolicy;

impl PolicyEnforcer for DenyAllPolicy {
    fn authorize(&self, _request: &PolicyRequest) -> PolicyDecision {
        PolicyDecision {
            allowed: false,
            reason: "deny-by-default: no execution policy is configured".into(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResultRef(String);

impl ResultRef {
    #[must_use]
    pub fn new() -> Self {
        Self(format!("lattice://result/{}", LatticeId::new()))
    }
}

impl Default for ResultRef {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ResultRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredResult {
    pub reference: ResultRef,
    pub content_hash: ContentHash,
    pub content_type: String,
    pub bytes: Arc<[u8]>,
}

#[derive(Default)]
pub struct MemoryResultStore {
    values: RwLock<BTreeMap<ResultRef, StoredResult>>,
}

impl MemoryResultStore {
    pub async fn put(&self, bytes: Vec<u8>, content_type: String) -> StoredResult {
        let stored = StoredResult {
            reference: ResultRef::new(),
            content_hash: ContentHash::of(&bytes),
            content_type,
            bytes: Arc::from(bytes),
        };
        self.values.write().await.insert(stored.reference.clone(), stored.clone());
        stored
    }

    pub async fn read_range(
        &self,
        reference: &ResultRef,
        offset: usize,
        length: usize,
    ) -> Result<Vec<u8>, BrokerError> {
        let values = self.values.read().await;
        let value = values.get(reference).ok_or(BrokerError::ResultNotFound)?;
        let start = offset.min(value.bytes.len());
        let end = start.saturating_add(length).min(value.bytes.len());
        Ok(value.bytes[start..end].to_vec())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationStatus {
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolInvocationResult {
    pub operation_id: OperationId,
    pub tool_ref: ToolRef,
    pub provider_id: ProviderId,
    pub schema_revision: SchemaRevision,
    pub arguments_hash: ContentHash,
    pub started_at_unix_ms: i64,
    pub finished_at_unix_ms: i64,
    pub status: InvocationStatus,
    pub inline: Option<serde_json::Value>,
    pub result_ref: Option<ResultRef>,
    pub byte_len: u64,
    pub content_type: Option<String>,
    pub error: Option<String>,
    pub cancellation_supported: Option<bool>,
}

#[derive(Clone)]
pub struct OperationCancellation {
    sender: watch::Sender<bool>,
}

impl Default for OperationCancellation {
    fn default() -> Self {
        let (sender, _) = watch::channel(false);
        Self { sender }
    }
}

impl OperationCancellation {
    pub fn cancel(&self) {
        self.sender.send_replace(true);
    }
}

struct ProviderSlot {
    provider: Arc<dyn ToolProvider>,
    connection: Mutex<Option<ProviderConnection>>,
    concurrency: Arc<Semaphore>,
}

pub struct ConnectionBroker {
    providers: RwLock<ProviderRegistry>,
    catalog: RwLock<ToolCatalog>,
    slots: RwLock<BTreeMap<ProviderId, Arc<ProviderSlot>>>,
    global_concurrency: Arc<Semaphore>,
    policy: Arc<dyn PolicyEnforcer>,
    results: Arc<MemoryResultStore>,
    inline_limit: usize,
    result_limit: usize,
    default_timeout: Duration,
}

impl ConnectionBroker {
    #[must_use]
    pub fn new(
        global_concurrency: usize,
        policy: Arc<dyn PolicyEnforcer>,
        results: Arc<MemoryResultStore>,
    ) -> Self {
        Self {
            providers: RwLock::new(ProviderRegistry::default()),
            catalog: RwLock::new(ToolCatalog::default()),
            slots: RwLock::new(BTreeMap::new()),
            global_concurrency: Arc::new(Semaphore::new(global_concurrency.max(1))),
            policy,
            results,
            inline_limit: 64 * 1024,
            result_limit: 64 * 1024 * 1024,
            default_timeout: Duration::from_secs(30),
        }
    }

    pub async fn register(
        &self,
        provider: Arc<dyn ToolProvider>,
        concurrency: usize,
    ) -> Result<ProviderId, BrokerError> {
        let descriptor = provider.descriptor();
        let id = descriptor.id;
        self.providers.write().await.register(descriptor)?;
        self.slots.write().await.insert(
            id,
            Arc::new(ProviderSlot {
                provider,
                connection: Mutex::new(None),
                concurrency: Arc::new(Semaphore::new(concurrency.max(1))),
            }),
        );
        Ok(id)
    }

    pub async fn connect(
        &self,
        provider_id: ProviderId,
    ) -> Result<ProviderConnection, BrokerError> {
        let slot = self.slot(provider_id).await?;
        if let Some(existing) = slot.connection.lock().await.clone() {
            return Ok(existing);
        }
        self.set_connection_state(provider_id, ProviderConnectionState::Connecting, None).await?;
        let connection = tokio::time::timeout(self.default_timeout, slot.provider.connect())
            .await
            .map_err(|_| BrokerError::OperationTimeout)??;
        if connection.provider_id != provider_id {
            return Err(BrokerError::ProviderIdentityMismatch);
        }
        *slot.connection.lock().await = Some(connection.clone());
        self.providers.write().await.update_descriptor(slot.provider.descriptor())?;
        self.set_connection_state(
            provider_id,
            ProviderConnectionState::Connected,
            Some(ProviderHealthStatus::Healthy),
        )
        .await?;
        self.refresh_catalog(provider_id).await?;
        Ok(connection)
    }

    pub async fn disconnect(&self, provider_id: ProviderId) -> Result<(), BrokerError> {
        let slot = self.slot(provider_id).await?;
        tokio::time::timeout(self.default_timeout, slot.provider.disconnect())
            .await
            .map_err(|_| BrokerError::OperationTimeout)??;
        *slot.connection.lock().await = None;
        self.set_connection_state(
            provider_id,
            ProviderConnectionState::Disconnected,
            Some(ProviderHealthStatus::Unavailable),
        )
        .await
    }

    pub async fn refresh_catalog(&self, provider_id: ProviderId) -> Result<(), BrokerError> {
        let slot = self.slot(provider_id).await?;
        if slot.connection.lock().await.is_none() {
            return Err(BrokerError::ProviderUnavailable(provider_id));
        }
        let tools = tokio::time::timeout(self.default_timeout, slot.provider.list_tools())
            .await
            .map_err(|_| BrokerError::OperationTimeout)??;
        let refresh = self.catalog.write().await.refresh_provider(provider_id, tools)?;
        let mut providers = self.providers.write().await;
        let mut health = providers
            .get(provider_id)
            .ok_or(BrokerError::ProviderUnavailable(provider_id))?
            .health
            .clone();
        health.catalog_revision = Some(refresh.catalog_revision);
        health.tool_count = u64::try_from(
            refresh.added.len() + refresh.unchanged.len() + refresh.schema_changed.len(),
        )
        .unwrap_or(u64::MAX);
        providers.update_health(provider_id, health)?;
        Ok(())
    }

    pub async fn call(
        &self,
        caller: &str,
        tool_ref: ToolRef,
        arguments: serde_json::Value,
        timeout: Option<Duration>,
        cancellation: Option<&OperationCancellation>,
    ) -> Result<ToolInvocationResult, BrokerError> {
        let started_at_unix_ms = unix_time_ms();
        let operation_id = OperationId::new();
        let encoded_arguments = serde_json::to_vec(&arguments)?;
        let arguments_hash = ContentHash::of(&encoded_arguments);
        let (schema_revision, availability) = {
            let catalog = self.catalog.read().await;
            let tool = catalog.get(&tool_ref).ok_or(BrokerError::ToolNotFound)?;
            let schema = catalog.schema(tool.input_schema).ok_or(BrokerError::ToolNotFound)?;
            lattice_tools::validate_tool_input(&schema.normalized_schema, &arguments)?;
            (tool.input_schema, tool.availability)
        };
        if availability != ToolAvailability::Available {
            return Err(BrokerError::ToolUnavailable);
        }
        let decision = self.policy.authorize(&PolicyRequest {
            caller: caller.into(),
            provider_id: tool_ref.provider_id,
            tool_ref: tool_ref.clone(),
            schema_revision,
            arguments_hash,
        });
        if !decision.allowed {
            return Err(BrokerError::PolicyDenied(decision.reason));
        }
        let slot = self.slot(tool_ref.provider_id).await?;
        if slot.connection.lock().await.is_none() {
            return Err(BrokerError::ProviderUnavailable(tool_ref.provider_id));
        }
        let _global =
            self.global_concurrency.acquire().await.map_err(|_| BrokerError::BrokerClosed)?;
        let _provider = slot.concurrency.acquire().await.map_err(|_| BrokerError::BrokerClosed)?;
        let duration = timeout.unwrap_or(self.default_timeout).min(self.default_timeout);
        let request =
            tokio::time::timeout(duration, slot.provider.call(tool_ref.tool_id, arguments));
        let response = if let Some(cancellation) = cancellation {
            let mut receiver = cancellation.sender.subscribe();
            if *receiver.borrow() {
                return Ok(terminal_result(
                    operation_id,
                    tool_ref,
                    schema_revision,
                    arguments_hash,
                    started_at_unix_ms,
                    InvocationStatus::Cancelled,
                    "operation cancelled before dispatch",
                ));
            }
            tokio::select! {
                value = request => value,
                _ = receiver.changed() => {
                    return Ok(terminal_result(
                        operation_id,
                        tool_ref,
                        schema_revision,
                        arguments_hash,
                        started_at_unix_ms,
                        InvocationStatus::Cancelled,
                        "cancellation requested; provider future was dropped",
                    ));
                }
            }
        } else {
            request.await
        };
        let provider_result = match response {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                return Ok(terminal_result(
                    operation_id,
                    tool_ref,
                    schema_revision,
                    arguments_hash,
                    started_at_unix_ms,
                    InvocationStatus::Failed,
                    &error.to_string(),
                ));
            }
            Err(_) => {
                return Ok(terminal_result(
                    operation_id,
                    tool_ref,
                    schema_revision,
                    arguments_hash,
                    started_at_unix_ms,
                    InvocationStatus::TimedOut,
                    "provider operation timed out; underlying cancellation is not guaranteed",
                ));
            }
        };
        let encoded = serde_json::to_vec(&provider_result.structured)?;
        if encoded.len() > self.result_limit {
            return Err(BrokerError::ResultTooLarge(encoded.len()));
        }
        let byte_len = u64::try_from(encoded.len()).unwrap_or(u64::MAX);
        let (inline, result_ref) = if encoded.len() <= self.inline_limit {
            (Some(provider_result.structured), None)
        } else {
            let stored = self.results.put(encoded, provider_result.content_type.clone()).await;
            (None, Some(stored.reference))
        };
        Ok(ToolInvocationResult {
            operation_id,
            tool_ref: tool_ref.clone(),
            provider_id: tool_ref.provider_id,
            schema_revision,
            arguments_hash,
            started_at_unix_ms,
            finished_at_unix_ms: unix_time_ms(),
            status: InvocationStatus::Succeeded,
            inline,
            result_ref,
            byte_len,
            content_type: Some(provider_result.content_type),
            error: None,
            cancellation_supported: Some(provider_result.cancellation_supported),
        })
    }

    pub async fn providers(&self) -> Vec<ProviderDescriptor> {
        self.providers.read().await.list().into_iter().cloned().collect()
    }

    pub async fn search_tools(
        &self,
        query: &str,
        limit: usize,
    ) -> Vec<lattice_tools::ToolDescriptor> {
        self.catalog.read().await.search(query, limit).into_iter().cloned().collect()
    }

    async fn slot(&self, id: ProviderId) -> Result<Arc<ProviderSlot>, BrokerError> {
        self.slots.read().await.get(&id).cloned().ok_or(BrokerError::ProviderUnavailable(id))
    }

    async fn set_connection_state(
        &self,
        id: ProviderId,
        state: ProviderConnectionState,
        status: Option<ProviderHealthStatus>,
    ) -> Result<(), BrokerError> {
        let mut providers = self.providers.write().await;
        let mut health =
            providers.get(id).ok_or(BrokerError::ProviderUnavailable(id))?.health.clone();
        health.connection_state = state;
        if let Some(status) = status {
            health.status = status;
        }
        providers.update_health(id, health)?;
        Ok(())
    }
}

fn terminal_result(
    operation_id: OperationId,
    tool_ref: ToolRef,
    schema_revision: SchemaRevision,
    arguments_hash: ContentHash,
    started_at_unix_ms: i64,
    status: InvocationStatus,
    error: &str,
) -> ToolInvocationResult {
    ToolInvocationResult {
        operation_id,
        provider_id: tool_ref.provider_id,
        tool_ref,
        schema_revision,
        arguments_hash,
        started_at_unix_ms,
        finished_at_unix_ms: unix_time_ms(),
        status,
        inline: None,
        result_ref: None,
        byte_len: 0,
        content_type: None,
        error: Some(error.into()),
        cancellation_supported: None,
    }
}

fn unix_time_ms() -> i64 {
    let millis =
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| duration.as_millis());
    i64::try_from(millis).unwrap_or(i64::MAX)
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("PROVIDER_CONNECTION_FAILED: {0}")]
    Connection(String),
    #[error("PROVIDER_PROTOCOL_ERROR: {0}")]
    Protocol(String),
    #[error("TOOL_NOT_FOUND: provider does not recognize the tool")]
    ToolNotFound,
    #[error("OPERATION_CANCELLED: provider operation was cancelled")]
    Cancelled,
}

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("PROVIDER_UNAVAILABLE: {0}")]
    ProviderUnavailable(ProviderId),
    #[error("PROVIDER_CONNECTION_FAILED: connection identity does not match provider")]
    ProviderIdentityMismatch,
    #[error("TOOL_NOT_FOUND: tool is not in the active catalog")]
    ToolNotFound,
    #[error("TOOL_UNAVAILABLE: tool is stale, removed, or quarantined")]
    ToolUnavailable,
    #[error("POLICY_DENIED: {0}")]
    PolicyDenied(String),
    #[error("OPERATION_TIMEOUT: provider operation exceeded its deadline")]
    OperationTimeout,
    #[error("RESULT_TOO_LARGE: provider result is {0} bytes")]
    ResultTooLarge(usize),
    #[error("RESULT_NOT_FOUND: immutable result does not exist")]
    ResultNotFound,
    #[error("PROVIDER_UNAVAILABLE: broker concurrency queue is closed")]
    BrokerClosed,
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Fabric(#[from] lattice_tools::ToolFabricError),
    #[error("PROVIDER_PROTOCOL_ERROR: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use lattice_tools::{
        AuthenticationState, OperationSemantics, ProviderHealth, ProviderKind, ProviderMetadata,
        ProviderTransportKind, ProviderTrust, ReportedSemantics, ToolTrust,
    };

    use super::*;

    struct AllowPolicy;

    impl PolicyEnforcer for AllowPolicy {
        fn authorize(&self, _request: &PolicyRequest) -> PolicyDecision {
            PolicyDecision { allowed: true, reason: "test".into() }
        }
    }

    struct FixtureProvider {
        descriptor: ProviderDescriptor,
        connects: AtomicUsize,
        output_size: usize,
    }

    impl FixtureProvider {
        fn new(output_size: usize) -> Self {
            let id = ProviderId::from_stable_key(b"fixture-provider");
            Self {
                descriptor: ProviderDescriptor {
                    id,
                    kind: ProviderKind::NativeAdapter,
                    name: "Fixture".into(),
                    version: Some("1".into()),
                    trust: ProviderTrust::Verified,
                    transport: ProviderTransportKind::NativeAdapter,
                    health: ProviderHealth {
                        status: ProviderHealthStatus::Unavailable,
                        reason: "configured".into(),
                        last_successful_operation_unix_ms: None,
                        last_failure: None,
                        rtt_micros: None,
                        consecutive_failures: 0,
                        connected_at_unix_ms: None,
                        catalog_revision: None,
                        tool_count: 0,
                        resource_count: 0,
                        authentication: AuthenticationState::NotRequired,
                        connection_state: ProviderConnectionState::Configured,
                    },
                    metadata: ProviderMetadata {
                        source: "test".into(),
                        studio_environment_id: None,
                        studio_session_id: None,
                        configured_priority: None,
                        protocol: None,
                    },
                },
                connects: AtomicUsize::new(0),
                output_size,
            }
        }
    }

    impl ToolProvider for FixtureProvider {
        fn descriptor(&self) -> ProviderDescriptor {
            self.descriptor.clone()
        }

        fn connect(&self) -> ProviderFuture<'_, ProviderConnection> {
            Box::pin(async move {
                self.connects.fetch_add(1, Ordering::SeqCst);
                Ok(ProviderConnection {
                    id: LatticeId::new(),
                    provider_id: self.descriptor.id,
                    endpoint_identity: "fixture".into(),
                    connected_at_unix_ms: unix_time_ms(),
                })
            })
        }

        fn disconnect(&self) -> ProviderFuture<'_, ()> {
            Box::pin(async { Ok(()) })
        }

        fn list_tools(&self) -> ProviderFuture<'_, Vec<ImportedTool>> {
            Box::pin(async {
                Ok(vec![ImportedTool {
                    native_name: "fixture.echo".into(),
                    title: Some("Fixture Echo".into()),
                    provider_description: Some("provider data".into()),
                    input_schema: serde_json::json!({"type":"object"}),
                    output_schema: None,
                    capabilities: Vec::new(),
                    verified_semantics: OperationSemantics::unknown(),
                    reported_semantics: ReportedSemantics {
                        read_only_hint: Some(true),
                        destructive_hint: Some(false),
                        idempotent_hint: Some(true),
                        open_world_hint: Some(false),
                    },
                    trust: ToolTrust::Verified,
                }])
            })
        }

        fn call(
            &self,
            _tool_id: ToolId,
            _input: serde_json::Value,
        ) -> ProviderFuture<'_, ProviderCallResult> {
            Box::pin(async move {
                Ok(ProviderCallResult {
                    structured: serde_json::json!({"payload":"x".repeat(self.output_size)}),
                    content_type: "application/json".into(),
                    cancellation_supported: false,
                })
            })
        }
    }

    #[tokio::test]
    async fn registration_is_lazy_and_connect_refreshes_catalog()
    -> Result<(), Box<dyn std::error::Error>> {
        let provider = Arc::new(FixtureProvider::new(10));
        let id = provider.descriptor.id;
        let broker =
            ConnectionBroker::new(4, Arc::new(AllowPolicy), Arc::new(MemoryResultStore::default()));
        broker.register(provider.clone(), 1).await?;
        assert_eq!(provider.connects.load(Ordering::SeqCst), 0);
        broker.connect(id).await?;
        assert_eq!(provider.connects.load(Ordering::SeqCst), 1);
        assert_eq!(broker.search_tools("echo", 10).await.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn large_results_are_immutable_references() -> Result<(), Box<dyn std::error::Error>> {
        let provider = Arc::new(FixtureProvider::new(70_000));
        let id = provider.descriptor.id;
        let results = Arc::new(MemoryResultStore::default());
        let broker = ConnectionBroker::new(4, Arc::new(AllowPolicy), results.clone());
        broker.register(provider, 1).await?;
        broker.connect(id).await?;
        let tool = broker.search_tools("echo", 1).await.remove(0).reference();
        let invocation = broker.call("test", tool, serde_json::json!({}), None, None).await?;
        assert!(invocation.inline.is_none());
        let reference = invocation.result_ref.ok_or(BrokerError::ResultNotFound)?;
        let first = results.read_range(&reference, 0, 32).await?;
        let second = results.read_range(&reference, 0, 32).await?;
        assert_eq!(first, second);
        Ok(())
    }

    #[tokio::test]
    async fn deny_by_default_blocks_dispatch() -> Result<(), Box<dyn std::error::Error>> {
        let provider = Arc::new(FixtureProvider::new(10));
        let id = provider.descriptor.id;
        let broker = ConnectionBroker::new(
            4,
            Arc::new(DenyAllPolicy),
            Arc::new(MemoryResultStore::default()),
        );
        broker.register(provider, 1).await?;
        broker.connect(id).await?;
        let tool = broker.search_tools("echo", 1).await.remove(0).reference();
        let result = broker.call("test", tool, serde_json::json!({}), None, None).await;
        assert!(matches!(result, Err(BrokerError::PolicyDenied(_))));
        Ok(())
    }
}
