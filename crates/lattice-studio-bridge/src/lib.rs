//! Native loopback transport for the optional Roblox Studio companion plugin.
//!
//! This is deliberately not MCP. Studio discovers and locally pairs with the
//! bridge, then initiates authenticated HTTP reports to Lattice while Lattice
//! returns bounded commands for that exact plugin session. The bridge enriches
//! Studio MCP with durable session identity and event telemetry; it never
//! replaces the official Studio MCP provider.

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fmt,
    future::Future,
    net::SocketAddr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use lattice_platform::{HostPlatform, StudioEnvironmentId, StudioRuntime};
use lattice_resource::LatticeId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::{net::TcpListener, sync::Mutex};

pub const BRIDGE_PROTOCOL_VERSION: u32 = 1;
const AUTHORIZATION: &str = "authorization";
const BEARER_PREFIX: &str = "Bearer ";
const PAIRING_CHALLENGE_TTL_MS: i64 = 30_000;
const PAIRING_SESSION_TTL_MS: i64 = 12 * 60 * 60 * 1_000;
const MAX_PENDING_PAIRING_CHALLENGES: usize = 32;
const MAX_PAIRING_SESSIONS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StudioBridgeSessionId(pub LatticeId);

impl StudioBridgeSessionId {
    #[must_use]
    pub fn new() -> Self {
        Self(LatticeId::new())
    }
}

impl Default for StudioBridgeSessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for StudioBridgeSessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StudioBridgeCommandId(pub LatticeId);

impl StudioBridgeCommandId {
    #[must_use]
    pub fn new() -> Self {
        Self(LatticeId::new())
    }
}

impl Default for StudioBridgeCommandId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudioDataModel {
    Edit,
    Play,
    Client,
    Server,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudioBridgeSessionState {
    Unbound,
    Bound,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeRegistration {
    pub protocol_version: u32,
    pub external_session_id: String,
    pub plugin_version: String,
    pub data_model: StudioDataModel,
    pub place_id: Option<u64>,
    pub universe_id: Option<u64>,
    pub is_running: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeRegistrationResponse {
    pub protocol_version: u32,
    pub session_id: StudioBridgeSessionId,
    pub report_interval_ms: u64,
    pub max_events_per_report: usize,
    pub max_commands_per_report: usize,
    pub backend: StudioBridgeBackendState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StudioBridgeEventPayload {
    Heartbeat,
    ModeChanged {
        data_model: StudioDataModel,
        is_running: bool,
    },
    Console {
        context: StudioDataModel,
        severity: String,
        message: String,
        stack: Option<String>,
        source: Option<String>,
        line: Option<u32>,
    },
    SelectionChanged {
        instance_paths: Vec<String>,
    },
    ActiveScriptChanged {
        instance_path: Option<String>,
    },
    ScriptChanged {
        instance_path: String,
        reported_source_hash: Option<String>,
    },
    InstanceDelta {
        operation: String,
        instance_path: String,
        class_name: Option<String>,
        parent_path: Option<String>,
    },
    ChangeHistory {
        operation: String,
        label: Option<String>,
    },
    Metrics {
        memory_mb: Option<f64>,
        physics_fps: Option<f64>,
        instance_count: Option<u64>,
        script_count: Option<u64>,
    },
    /// Forward-compatible provider data. It remains untrusted telemetry and is
    /// never interpreted as Lattice authority or an instruction.
    Extension {
        name: String,
        payload: Value,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeEvent {
    pub event_id: String,
    pub sequence: u64,
    pub observed_unix_ms: i64,
    pub payload: StudioBridgeEventPayload,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StudioBridgeCommandPayload {
    GetStudioState,
    GetSelection,
    ReadSource { path_components: Vec<String> },
    GetChildren { path_components: Vec<String> },
    Subscribe { event_kinds: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeCommand {
    pub id: StudioBridgeCommandId,
    pub payload: StudioBridgeCommandPayload,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeCommandResult {
    pub command_id: StudioBridgeCommandId,
    pub succeeded: bool,
    pub value: Option<Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeReport {
    pub protocol_version: u32,
    pub session_id: StudioBridgeSessionId,
    pub report_sequence: u64,
    pub data_model: StudioDataModel,
    pub place_id: Option<u64>,
    pub universe_id: Option<u64>,
    pub is_running: bool,
    #[serde(default)]
    pub events: Vec<StudioBridgeEvent>,
    #[serde(default)]
    pub command_results: Vec<StudioBridgeCommandResult>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeReportResponse {
    pub accepted_sequence: u64,
    pub commands: Vec<StudioBridgeCommand>,
    pub dropped_events_total: u64,
    pub backend: StudioBridgeBackendState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeEnvironmentSummary {
    pub host_platform: HostPlatform,
    pub runtime: StudioRuntime,
    pub environment_id: StudioEnvironmentId,
    pub process_id: Option<u32>,
    pub deployment: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeServiceHealth {
    pub state: String,
    pub detail: Option<String>,
    pub latency_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeRecorderState {
    pub state: String,
    pub mode: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub events: Option<u64>,
    pub anomalies: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeClientSummary {
    pub name: String,
    pub authority: Option<String>,
}

/// Structured daemon-owned state rendered by the plugin cockpit. `None` and
/// `unavailable` are intentional truth states; the UI must not synthesize them.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeBackendState {
    pub provider: String,
    pub environment: Option<StudioBridgeEnvironmentSummary>,
    /// Canonical display name returned by the real Studio MCP session.
    #[serde(default)]
    pub place_name: Option<String>,
    pub mcp: StudioBridgeServiceHealth,
    pub index_state: Option<String>,
    pub recorder: StudioBridgeRecorderState,
    pub connected_clients: Vec<StudioBridgeClientSummary>,
    pub capabilities: Vec<String>,
    pub authorization: Vec<String>,
    pub errors: Vec<String>,
}

impl Default for StudioBridgeBackendState {
    fn default() -> Self {
        Self {
            provider: "Lattice Studio Bridge".to_owned(),
            environment: None,
            place_name: None,
            mcp: StudioBridgeServiceHealth {
                state: "unavailable".to_owned(),
                detail: Some("Studio MCP state has not been bound to the companion".to_owned()),
                latency_ms: None,
            },
            index_state: None,
            recorder: StudioBridgeRecorderState {
                state: "unavailable".to_owned(),
                mode: None,
                elapsed_ms: None,
                events: None,
                anomalies: None,
            },
            connected_clients: Vec::new(),
            capabilities: Vec::new(),
            authorization: Vec::new(),
            errors: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeBinding {
    pub studio_session_id: LatticeId,
    pub environment_id: StudioEnvironmentId,
    pub process_id: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioBridgeSessionSnapshot {
    pub id: StudioBridgeSessionId,
    pub external_session_id: String,
    pub plugin_version: String,
    pub data_model: StudioDataModel,
    pub place_id: Option<u64>,
    pub universe_id: Option<u64>,
    pub is_running: bool,
    pub state: StudioBridgeSessionState,
    pub binding: Option<StudioBridgeBinding>,
    pub created_unix_ms: i64,
    pub last_seen_unix_ms: i64,
    pub last_report_sequence: Option<u64>,
    pub queued_commands: usize,
    pub in_flight_commands: usize,
    pub retained_events: usize,
    pub dropped_events_total: u64,
}

#[derive(Clone, Debug)]
pub struct StudioBridgeLimits {
    pub max_sessions: usize,
    pub max_pending_commands_per_session: usize,
    pub max_events_per_session: usize,
    pub max_events_per_report: usize,
    pub max_command_results_per_report: usize,
    pub max_commands_per_report: usize,
    pub max_body_bytes: usize,
    pub report_interval: Duration,
    pub command_timeout: Duration,
    pub session_ttl: Duration,
}

impl Default for StudioBridgeLimits {
    fn default() -> Self {
        Self {
            max_sessions: 32,
            max_pending_commands_per_session: 128,
            max_events_per_session: 4_096,
            max_events_per_report: 512,
            max_command_results_per_report: 128,
            max_commands_per_report: 32,
            max_body_bytes: 1024 * 1024,
            report_interval: Duration::from_millis(500),
            command_timeout: Duration::from_secs(15),
            session_ttl: Duration::from_mins(1),
        }
    }
}

/// Secret used only to authenticate the local Studio bridge. Debug output is
/// intentionally redacted. Production construction should source the value
/// through Lattice's secret-store boundary.
#[derive(Clone)]
pub struct StudioBridgeAuthToken(Vec<u8>);

impl StudioBridgeAuthToken {
    /// Constructs a bridge token.
    ///
    /// # Errors
    ///
    /// Rejects tokens shorter than 32 bytes or larger than 256 bytes.
    pub fn new(value: impl Into<Vec<u8>>) -> Result<Self, StudioBridgeError> {
        let value = value.into();
        if !(32..=256).contains(&value.len()) {
            return Err(StudioBridgeError::InvalidAuthenticationConfiguration);
        }
        Ok(Self(value))
    }

    fn matches(&self, candidate: &[u8]) -> bool {
        let expected = blake3::hash(&self.0);
        let candidate = blake3::hash(candidate);
        expected
            .as_bytes()
            .iter()
            .zip(candidate.as_bytes())
            .fold(0_u8, |difference, (left, right)| difference | (left ^ right))
            == 0
    }
}

impl fmt::Debug for StudioBridgeAuthToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StudioBridgeAuthToken([REDACTED])")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudioBridgeDiscoveryResponse {
    pub service: String,
    pub protocol_version: u32,
    pub authentication: String,
    pub challenge: String,
    pub challenge_expires_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudioBridgePairingRequest {
    pub challenge: String,
    pub client_kind: String,
    pub client_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudioBridgePairingResponse {
    pub service: String,
    pub protocol_version: u32,
    pub session_token: String,
    pub session_expires_unix_ms: i64,
}

#[derive(Default)]
struct PairingState {
    challenges: HashMap<String, i64>,
    sessions: HashMap<String, i64>,
}

impl PairingState {
    fn issue_challenge(&mut self, now: i64) -> (String, i64) {
        self.expire(now);
        while self.challenges.len() >= MAX_PENDING_PAIRING_CHALLENGES {
            if let Some(oldest) = self
                .challenges
                .iter()
                .min_by_key(|(_, expires)| *expires)
                .map(|(value, _)| value.clone())
            {
                self.challenges.remove(&oldest);
            } else {
                break;
            }
        }
        let challenge = opaque_nonce("challenge");
        let expires = now.saturating_add(PAIRING_CHALLENGE_TTL_MS);
        self.challenges.insert(challenge.clone(), expires);
        (challenge, expires)
    }

    fn redeem(&mut self, challenge: &str, now: i64) -> Option<(String, i64)> {
        self.expire(now);
        self.challenges.remove(challenge)?;
        while self.sessions.len() >= MAX_PAIRING_SESSIONS {
            if let Some(oldest) = self
                .sessions
                .iter()
                .min_by_key(|(_, expires)| *expires)
                .map(|(value, _)| value.clone())
            {
                self.sessions.remove(&oldest);
            } else {
                break;
            }
        }
        let token = opaque_nonce("session");
        let expires = now.saturating_add(PAIRING_SESSION_TTL_MS);
        self.sessions.insert(token.clone(), expires);
        Some((token, expires))
    }

    fn matches(&mut self, candidate: &str, now: i64) -> bool {
        self.expire(now);
        self.sessions.get(candidate).is_some_and(|expires| *expires > now)
    }

    fn expire(&mut self, now: i64) {
        self.challenges.retain(|_, expires| *expires > now);
        self.sessions.retain(|_, expires| *expires > now);
    }
}

fn opaque_nonce(prefix: &str) -> String {
    // Uuid v7 obtains random bits from the platform RNG. Hashing the value
    // gives the wire credential a uniform, opaque representation without
    // persisting secrets or requiring a user-managed key.
    let seed = format!("{prefix}:{}", lattice_resource::LatticeId::new());
    format!("{prefix}_{}", blake3::hash(seed.as_bytes()).to_hex())
}

struct SessionRecord {
    snapshot: StudioBridgeSessionSnapshot,
    queued_commands: VecDeque<StudioBridgeCommand>,
    in_flight_commands: BTreeMap<StudioBridgeCommandId, StudioBridgeCommand>,
    responders:
        HashMap<StudioBridgeCommandId, tokio::sync::oneshot::Sender<StudioBridgeCommandResult>>,
    events: VecDeque<StudioBridgeEvent>,
    last_response: Option<StudioBridgeReportResponse>,
}

#[derive(Default)]
struct BrokerState {
    sessions: HashMap<StudioBridgeSessionId, SessionRecord>,
    external_sessions: HashMap<String, StudioBridgeSessionId>,
    backend: StudioBridgeBackendState,
}

pub struct StudioBridgeBroker {
    limits: StudioBridgeLimits,
    state: Mutex<BrokerState>,
}

impl StudioBridgeBroker {
    #[must_use]
    pub fn new(limits: StudioBridgeLimits) -> Self {
        Self { limits, state: Mutex::new(BrokerState::default()) }
    }

    pub async fn set_backend_state(&self, backend: StudioBridgeBackendState) {
        self.state.lock().await.backend = backend;
    }

    /// Registers or refreshes one plugin-owned Studio session.
    pub async fn register(
        &self,
        registration: StudioBridgeRegistration,
    ) -> Result<StudioBridgeRegistrationResponse, StudioBridgeError> {
        validate_protocol(registration.protocol_version)?;
        validate_small_identifier(&registration.external_session_id)?;
        validate_small_identifier(&registration.plugin_version)?;
        let now = unix_time_ms();
        let mut state = self.state.lock().await;
        expire_sessions(&mut state, &self.limits, now);

        if let Some(session_id) = state.external_sessions.get(&registration.external_session_id) {
            let session_id = *session_id;
            let record = state
                .sessions
                .get_mut(&session_id)
                .ok_or(StudioBridgeError::SessionNotFound(session_id))?;
            record.snapshot.plugin_version = registration.plugin_version;
            record.snapshot.data_model = registration.data_model;
            record.snapshot.place_id = registration.place_id;
            record.snapshot.universe_id = registration.universe_id;
            record.snapshot.is_running = registration.is_running;
            record.snapshot.last_seen_unix_ms = now;
            record.snapshot.state = if record.snapshot.binding.is_some() {
                StudioBridgeSessionState::Bound
            } else {
                StudioBridgeSessionState::Unbound
            };
            return Ok(self.registration_response(session_id, state.backend.clone()));
        }

        if state.sessions.len() >= self.limits.max_sessions {
            return Err(StudioBridgeError::SessionCapacityExceeded);
        }
        let session_id = StudioBridgeSessionId::new();
        let snapshot = StudioBridgeSessionSnapshot {
            id: session_id,
            external_session_id: registration.external_session_id.clone(),
            plugin_version: registration.plugin_version,
            data_model: registration.data_model,
            place_id: registration.place_id,
            universe_id: registration.universe_id,
            is_running: registration.is_running,
            state: StudioBridgeSessionState::Unbound,
            binding: None,
            created_unix_ms: now,
            last_seen_unix_ms: now,
            last_report_sequence: None,
            queued_commands: 0,
            in_flight_commands: 0,
            retained_events: 0,
            dropped_events_total: 0,
        };
        state.external_sessions.insert(registration.external_session_id, session_id);
        state.sessions.insert(
            session_id,
            SessionRecord {
                snapshot,
                queued_commands: VecDeque::new(),
                in_flight_commands: BTreeMap::new(),
                responders: HashMap::new(),
                events: VecDeque::new(),
                last_response: None,
            },
        );
        Ok(self.registration_response(session_id, state.backend.clone()))
    }

    fn registration_response(
        &self,
        session_id: StudioBridgeSessionId,
        backend: StudioBridgeBackendState,
    ) -> StudioBridgeRegistrationResponse {
        let interval = u64::try_from(self.limits.report_interval.as_millis()).unwrap_or(u64::MAX);
        StudioBridgeRegistrationResponse {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            session_id,
            report_interval_ms: interval,
            max_events_per_report: self.limits.max_events_per_report,
            max_commands_per_report: self.limits.max_commands_per_report,
            backend,
        }
    }

    /// Associates a plugin session with Lattice's already-resolved Studio
    /// session/environment. The bridge never guesses this association.
    pub async fn bind_session(
        &self,
        session_id: StudioBridgeSessionId,
        binding: StudioBridgeBinding,
    ) -> Result<(), StudioBridgeError> {
        let mut state = self.state.lock().await;
        let record = state
            .sessions
            .get_mut(&session_id)
            .ok_or(StudioBridgeError::SessionNotFound(session_id))?;
        if let Some(existing) = &record.snapshot.binding
            && existing != &binding
        {
            return Err(StudioBridgeError::BindingConflict);
        }
        record.snapshot.binding = Some(binding);
        record.snapshot.state = StudioBridgeSessionState::Bound;
        Ok(())
    }

    /// Accepts one ordered plugin report and returns commands for only that
    /// exact session. Dispatched commands remain leased until a result arrives,
    /// allowing safe response retransmission after network loss.
    pub async fn report(
        &self,
        report: StudioBridgeReport,
    ) -> Result<StudioBridgeReportResponse, StudioBridgeError> {
        validate_protocol(report.protocol_version)?;
        if report.events.len() > self.limits.max_events_per_report
            || report.command_results.len() > self.limits.max_command_results_per_report
        {
            return Err(StudioBridgeError::ReportLimitExceeded);
        }
        let now = unix_time_ms();
        let mut state = self.state.lock().await;
        let backend = state.backend.clone();
        let record = state
            .sessions
            .get_mut(&report.session_id)
            .ok_or(StudioBridgeError::SessionNotFound(report.session_id))?;

        if let Some(previous) = record.snapshot.last_report_sequence {
            if report.report_sequence < previous {
                return Err(StudioBridgeError::OutOfOrderReport {
                    previous,
                    received: report.report_sequence,
                });
            }
            if report.report_sequence == previous {
                return record
                    .last_response
                    .clone()
                    .ok_or(StudioBridgeError::DuplicateReportUnavailable);
            }
        }

        record.snapshot.data_model = report.data_model;
        record.snapshot.place_id = report.place_id;
        record.snapshot.universe_id = report.universe_id;
        record.snapshot.is_running = report.is_running;
        record.snapshot.last_seen_unix_ms = now;
        record.snapshot.last_report_sequence = Some(report.report_sequence);
        record.snapshot.state = if record.snapshot.binding.is_some() {
            StudioBridgeSessionState::Bound
        } else {
            StudioBridgeSessionState::Unbound
        };

        for result in report.command_results {
            record.in_flight_commands.remove(&result.command_id);
            if let Some(responder) = record.responders.remove(&result.command_id) {
                let _send_result = responder.send(result);
            }
        }

        for event in report.events {
            if self.limits.max_events_per_session == 0 {
                record.snapshot.dropped_events_total =
                    record.snapshot.dropped_events_total.saturating_add(1);
                continue;
            }
            if record.events.len() >= self.limits.max_events_per_session {
                record.events.pop_front();
                record.snapshot.dropped_events_total =
                    record.snapshot.dropped_events_total.saturating_add(1);
            }
            record.events.push_back(event);
        }

        let available =
            self.limits.max_commands_per_report.saturating_sub(record.in_flight_commands.len());
        for _ in 0..available {
            let Some(command) = record.queued_commands.pop_front() else {
                break;
            };
            record.in_flight_commands.insert(command.id, command);
        }
        let commands = record
            .in_flight_commands
            .values()
            .take(self.limits.max_commands_per_report)
            .cloned()
            .collect();
        record.snapshot.queued_commands = record.queued_commands.len();
        record.snapshot.in_flight_commands = record.in_flight_commands.len();
        record.snapshot.retained_events = record.events.len();
        let response = StudioBridgeReportResponse {
            accepted_sequence: report.report_sequence,
            commands,
            dropped_events_total: record.snapshot.dropped_events_total,
            backend,
        };
        record.last_response = Some(response.clone());
        Ok(response)
    }

    /// Executes a command against one explicit, bound bridge session.
    ///
    /// # Errors
    ///
    /// Returns a typed unavailable, capacity, timeout, or plugin error. There is
    /// no implicit "most recent Studio" fallback.
    pub async fn call(
        &self,
        session_id: StudioBridgeSessionId,
        payload: StudioBridgeCommandPayload,
    ) -> Result<StudioBridgeCommandResult, StudioBridgeError> {
        let command_id = StudioBridgeCommandId::new();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        {
            let mut state = self.state.lock().await;
            let record = state
                .sessions
                .get_mut(&session_id)
                .ok_or(StudioBridgeError::SessionNotFound(session_id))?;
            if record.snapshot.binding.is_none() {
                return Err(StudioBridgeError::SessionUnbound(session_id));
            }
            let pending = record.queued_commands.len() + record.in_flight_commands.len();
            if pending >= self.limits.max_pending_commands_per_session {
                return Err(StudioBridgeError::CommandCapacityExceeded(session_id));
            }
            record.queued_commands.push_back(StudioBridgeCommand { id: command_id, payload });
            record.responders.insert(command_id, sender);
            record.snapshot.queued_commands = record.queued_commands.len();
        }

        match tokio::time::timeout(self.limits.command_timeout, receiver).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_closed)) => {
                self.remove_command(session_id, command_id).await;
                Err(StudioBridgeError::CommandChannelClosed(command_id))
            }
            Err(_elapsed) => {
                self.remove_command(session_id, command_id).await;
                Err(StudioBridgeError::CommandTimedOut(command_id))
            }
        }
    }

    async fn remove_command(
        &self,
        session_id: StudioBridgeSessionId,
        command_id: StudioBridgeCommandId,
    ) {
        let mut state = self.state.lock().await;
        if let Some(record) = state.sessions.get_mut(&session_id) {
            record.queued_commands.retain(|command| command.id != command_id);
            record.in_flight_commands.remove(&command_id);
            record.responders.remove(&command_id);
            record.snapshot.queued_commands = record.queued_commands.len();
            record.snapshot.in_flight_commands = record.in_flight_commands.len();
        }
    }

    #[must_use]
    pub async fn sessions(&self) -> Vec<StudioBridgeSessionSnapshot> {
        let now = unix_time_ms();
        let mut state = self.state.lock().await;
        mark_stale_sessions(&mut state, &self.limits, now);
        let mut sessions =
            state.sessions.values().map(|record| record.snapshot.clone()).collect::<Vec<_>>();
        sessions.sort_by_key(|session| session.created_unix_ms);
        sessions
    }

    /// Drains at most `limit` retained telemetry events from one session.
    pub async fn drain_events(
        &self,
        session_id: StudioBridgeSessionId,
        limit: usize,
    ) -> Result<Vec<StudioBridgeEvent>, StudioBridgeError> {
        let mut state = self.state.lock().await;
        let record = state
            .sessions
            .get_mut(&session_id)
            .ok_or(StudioBridgeError::SessionNotFound(session_id))?;
        let count = limit.min(record.events.len());
        let events = record.events.drain(..count).collect();
        record.snapshot.retained_events = record.events.len();
        Ok(events)
    }
}

fn expire_sessions(state: &mut BrokerState, limits: &StudioBridgeLimits, now: i64) {
    let ttl = duration_millis_i64(limits.session_ttl);
    let expired = state
        .sessions
        .iter()
        .filter_map(|(id, record)| {
            (now.saturating_sub(record.snapshot.last_seen_unix_ms) > ttl).then_some(*id)
        })
        .collect::<Vec<_>>();
    for id in expired {
        if let Some(record) = state.sessions.remove(&id) {
            state.external_sessions.remove(&record.snapshot.external_session_id);
        }
    }
}

fn mark_stale_sessions(state: &mut BrokerState, limits: &StudioBridgeLimits, now: i64) {
    let ttl = duration_millis_i64(limits.session_ttl);
    for record in state.sessions.values_mut() {
        if now.saturating_sub(record.snapshot.last_seen_unix_ms) > ttl {
            record.snapshot.state = StudioBridgeSessionState::Stale;
        }
    }
}

fn duration_millis_i64(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn validate_protocol(version: u32) -> Result<(), StudioBridgeError> {
    if version == BRIDGE_PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(StudioBridgeError::ProtocolMismatch {
            expected: BRIDGE_PROTOCOL_VERSION,
            received: version,
        })
    }
}

fn validate_small_identifier(value: &str) -> Result<(), StudioBridgeError> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        Err(StudioBridgeError::InvalidRegistration)
    } else {
        Ok(())
    }
}

fn unix_time_ms() -> i64 {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    duration_millis_i64(elapsed)
}

#[derive(Clone)]
struct HttpState {
    broker: std::sync::Arc<StudioBridgeBroker>,
    manual_token: Option<StudioBridgeAuthToken>,
    pairing: std::sync::Arc<Mutex<PairingState>>,
}

#[derive(Clone, Debug)]
pub struct StudioBridgeServerConfig {
    pub bind: SocketAddr,
    /// Optional legacy token. New Studio companions use local discovery and
    /// one-time pairing, so no user-managed token is required.
    pub token: Option<StudioBridgeAuthToken>,
}

pub struct BoundStudioBridgeServer {
    listener: TcpListener,
    router: Router,
    local_address: SocketAddr,
}

impl BoundStudioBridgeServer {
    #[must_use]
    pub const fn local_address(&self) -> SocketAddr {
        self.local_address
    }

    /// Serves until shutdown resolves, then drains existing requests.
    pub async fn serve_until<F>(self, shutdown: F) -> Result<(), StudioBridgeError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        axum::serve(self.listener, self.router)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(StudioBridgeError::Serve)
    }
}

/// Binds the authenticated companion service. Non-loopback addresses are
/// rejected before any socket is opened.
pub async fn bind_studio_bridge(
    config: StudioBridgeServerConfig,
    broker: std::sync::Arc<StudioBridgeBroker>,
) -> Result<BoundStudioBridgeServer, StudioBridgeError> {
    if !config.bind.ip().is_loopback() {
        return Err(StudioBridgeError::NonLoopbackBind(config.bind));
    }
    let listener = TcpListener::bind(config.bind).await.map_err(StudioBridgeError::Bind)?;
    let local_address = listener.local_addr().map_err(StudioBridgeError::Bind)?;
    let max_body_bytes = broker.limits.max_body_bytes;
    let state = HttpState {
        broker,
        manual_token: config.token,
        pairing: std::sync::Arc::new(Mutex::new(PairingState::default())),
    };
    let router = Router::new()
        .route("/health", get(health))
        .route("/v1/studio-bridge/discover", get(discover_http))
        .route("/v1/studio-bridge/pair", post(pair_http))
        .route("/v1/studio-bridge/register", post(register_http))
        .route("/v1/studio-bridge/report", post(report_http))
        .route("/v1/studio-bridge/sessions", get(sessions_http))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .with_state(state);
    Ok(BoundStudioBridgeServer { listener, router, local_address })
}

#[derive(Serialize)]
struct HealthResponse {
    service: &'static str,
    protocol_version: u32,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "abraxius-lattice-studio-bridge",
        protocol_version: BRIDGE_PROTOCOL_VERSION,
    })
}

async fn discover_http(State(state): State<HttpState>) -> Json<StudioBridgeDiscoveryResponse> {
    let now = unix_time_ms();
    let (challenge, expires) = state.pairing.lock().await.issue_challenge(now);
    Json(StudioBridgeDiscoveryResponse {
        service: "abraxius-lattice-studio-bridge".to_owned(),
        protocol_version: BRIDGE_PROTOCOL_VERSION,
        authentication: "loopback_auto_pair".to_owned(),
        challenge,
        challenge_expires_unix_ms: expires,
    })
}

async fn pair_http(
    State(state): State<HttpState>,
    payload: Result<Json<StudioBridgePairingRequest>, JsonRejection>,
) -> Result<Json<StudioBridgePairingResponse>, StudioBridgeHttpError> {
    let Json(payload) = payload.map_err(StudioBridgeHttpError::InvalidJson)?;
    if !matches!(payload.client_kind.as_str(), "roblox-studio-plugin" | "lattice-desktop")
        || payload.challenge.is_empty()
        || payload.challenge.len() > 256
        || payload.challenge.chars().any(char::is_control)
    {
        return Err(StudioBridgeHttpError::PairingRejected);
    }
    if let Some(client_name) = payload.client_name.as_deref() {
        validate_small_identifier(client_name).map_err(StudioBridgeHttpError::Bridge)?;
    }
    let now = unix_time_ms();
    let Some((session_token, session_expires_unix_ms)) =
        state.pairing.lock().await.redeem(&payload.challenge, now)
    else {
        return Err(StudioBridgeHttpError::PairingRejected);
    };
    Ok(Json(StudioBridgePairingResponse {
        service: "abraxius-lattice-studio-bridge".to_owned(),
        protocol_version: BRIDGE_PROTOCOL_VERSION,
        session_token,
        session_expires_unix_ms,
    }))
}

async fn register_http(
    State(state): State<HttpState>,
    headers: HeaderMap,
    payload: Result<Json<StudioBridgeRegistration>, JsonRejection>,
) -> Result<Json<StudioBridgeRegistrationResponse>, StudioBridgeHttpError> {
    authenticate(&headers, &state).await?;
    let Json(payload) = payload.map_err(StudioBridgeHttpError::InvalidJson)?;
    state.broker.register(payload).await.map(Json).map_err(Into::into)
}

async fn report_http(
    State(state): State<HttpState>,
    headers: HeaderMap,
    payload: Result<Json<StudioBridgeReport>, JsonRejection>,
) -> Result<Json<StudioBridgeReportResponse>, StudioBridgeHttpError> {
    authenticate(&headers, &state).await?;
    let Json(payload) = payload.map_err(StudioBridgeHttpError::InvalidJson)?;
    state.broker.report(payload).await.map(Json).map_err(Into::into)
}

async fn sessions_http(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<Vec<StudioBridgeSessionSnapshot>>, StudioBridgeHttpError> {
    authenticate(&headers, &state).await?;
    Ok(Json(state.broker.sessions().await))
}

async fn authenticate(headers: &HeaderMap, state: &HttpState) -> Result<(), StudioBridgeHttpError> {
    let Some(supplied) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix(BEARER_PREFIX))
    else {
        return Err(StudioBridgeHttpError::Unauthorized);
    };
    if state.manual_token.as_ref().is_some_and(|expected| expected.matches(supplied.as_bytes()))
        || state.pairing.lock().await.matches(supplied, unix_time_ms())
    {
        Ok(())
    } else {
        Err(StudioBridgeHttpError::Unauthorized)
    }
}

#[derive(Debug, Error)]
pub enum StudioBridgeError {
    #[error("STUDIO_BRIDGE_AUTH_INVALID: token must contain 32 to 256 bytes")]
    InvalidAuthenticationConfiguration,
    #[error("STUDIO_BRIDGE_PROTOCOL_MISMATCH: expected {expected}, received {received}")]
    ProtocolMismatch { expected: u32, received: u32 },
    #[error("STUDIO_BRIDGE_REGISTRATION_INVALID: identifiers are empty, oversized, or invalid")]
    InvalidRegistration,
    #[error("STUDIO_BRIDGE_SESSION_LIMIT: the bounded session registry is full")]
    SessionCapacityExceeded,
    #[error("STUDIO_BRIDGE_SESSION_NOT_FOUND: {0}")]
    SessionNotFound(StudioBridgeSessionId),
    #[error("STUDIO_BRIDGE_SESSION_UNBOUND: {0} has no verified Studio environment binding")]
    SessionUnbound(StudioBridgeSessionId),
    #[error("STUDIO_BRIDGE_BINDING_CONFLICT: the session is already bound to another Studio")]
    BindingConflict,
    #[error("STUDIO_BRIDGE_REPORT_LIMIT: report exceeds negotiated bounded limits")]
    ReportLimitExceeded,
    #[error("STUDIO_BRIDGE_REPORT_OUT_OF_ORDER: previous {previous}, received {received}")]
    OutOfOrderReport { previous: u64, received: u64 },
    #[error("STUDIO_BRIDGE_DUPLICATE_UNAVAILABLE: duplicate report response is unavailable")]
    DuplicateReportUnavailable,
    #[error("STUDIO_BRIDGE_COMMAND_LIMIT: session {0} command queue is full")]
    CommandCapacityExceeded(StudioBridgeSessionId),
    #[error("STUDIO_BRIDGE_COMMAND_TIMEOUT: command {0:?} timed out")]
    CommandTimedOut(StudioBridgeCommandId),
    #[error("STUDIO_BRIDGE_COMMAND_CLOSED: command {0:?} response channel closed")]
    CommandChannelClosed(StudioBridgeCommandId),
    #[error("STUDIO_BRIDGE_BIND_DENIED: {0} is not a loopback address")]
    NonLoopbackBind(SocketAddr),
    #[error("STUDIO_BRIDGE_BIND_FAILED: {0}")]
    Bind(std::io::Error),
    #[error("STUDIO_BRIDGE_SERVE_FAILED: {0}")]
    Serve(std::io::Error),
}

#[derive(Debug)]
enum StudioBridgeHttpError {
    Unauthorized,
    PairingRejected,
    InvalidJson(JsonRejection),
    Bridge(StudioBridgeError),
}

impl From<StudioBridgeError> for StudioBridgeHttpError {
    fn from(value: StudioBridgeError) -> Self {
        Self::Bridge(value)
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    code: &'static str,
    message: String,
    recoverable: bool,
}

impl IntoResponse for StudioBridgeHttpError {
    fn into_response(self) -> Response {
        let (status, code, message, recoverable) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "STUDIO_BRIDGE_UNAUTHORIZED",
                "missing or invalid bridge authorization".to_owned(),
                true,
            ),
            Self::PairingRejected => (
                StatusCode::UNAUTHORIZED,
                "STUDIO_BRIDGE_PAIRING_REJECTED",
                "pairing challenge is missing, expired, or already used".to_owned(),
                true,
            ),
            Self::InvalidJson(error) => {
                (StatusCode::BAD_REQUEST, "STUDIO_BRIDGE_REQUEST_INVALID", error.body_text(), true)
            }
            Self::Bridge(error) => {
                let status = match error {
                    StudioBridgeError::SessionNotFound(_) => StatusCode::NOT_FOUND,
                    StudioBridgeError::SessionCapacityExceeded
                    | StudioBridgeError::CommandCapacityExceeded(_) => {
                        StatusCode::TOO_MANY_REQUESTS
                    }
                    StudioBridgeError::BindingConflict
                    | StudioBridgeError::OutOfOrderReport { .. } => StatusCode::CONFLICT,
                    _ => StatusCode::BAD_REQUEST,
                };
                (status, "STUDIO_BRIDGE_REQUEST_FAILED", error.to_string(), true)
            }
        };
        (status, Json(ErrorResponse { code, message, recoverable })).into_response()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn registration(external: &str) -> StudioBridgeRegistration {
        StudioBridgeRegistration {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            external_session_id: external.to_owned(),
            plugin_version: "0.1.0".to_owned(),
            data_model: StudioDataModel::Edit,
            place_id: Some(42),
            universe_id: Some(7),
            is_running: false,
        }
    }

    fn report(session_id: StudioBridgeSessionId, sequence: u64) -> StudioBridgeReport {
        StudioBridgeReport {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            session_id,
            report_sequence: sequence,
            data_model: StudioDataModel::Edit,
            place_id: Some(42),
            universe_id: Some(7),
            is_running: false,
            events: Vec::new(),
            command_results: Vec::new(),
        }
    }

    #[tokio::test]
    async fn registration_is_stable_and_session_binding_is_explicit()
    -> Result<(), StudioBridgeError> {
        let broker = StudioBridgeBroker::new(StudioBridgeLimits::default());
        let first = broker.register(registration("plugin-window-a")).await?;
        let second = broker.register(registration("plugin-window-a")).await?;
        assert_eq!(first.session_id, second.session_id);
        assert!(matches!(
            broker.call(first.session_id, StudioBridgeCommandPayload::GetStudioState).await,
            Err(StudioBridgeError::SessionUnbound(_))
        ));
        broker
            .bind_session(
                first.session_id,
                StudioBridgeBinding {
                    studio_session_id: LatticeId::new(),
                    environment_id: StudioEnvironmentId::from_fingerprint(b"studio-a"),
                    process_id: 99,
                },
            )
            .await?;
        assert_eq!(broker.sessions().await[0].state, StudioBridgeSessionState::Bound);
        Ok(())
    }

    #[tokio::test]
    async fn report_replay_never_issues_a_new_command() -> Result<(), StudioBridgeError> {
        let limits = StudioBridgeLimits {
            command_timeout: Duration::from_secs(2),
            ..StudioBridgeLimits::default()
        };
        let broker = Arc::new(StudioBridgeBroker::new(limits));
        let registered = broker.register(registration("plugin-window-a")).await?;
        broker
            .bind_session(
                registered.session_id,
                StudioBridgeBinding {
                    studio_session_id: LatticeId::new(),
                    environment_id: StudioEnvironmentId::from_fingerprint(b"studio-a"),
                    process_id: 99,
                },
            )
            .await?;

        let calling_broker = Arc::clone(&broker);
        let session_id = registered.session_id;
        let call = tokio::spawn(async move {
            calling_broker.call(session_id, StudioBridgeCommandPayload::GetStudioState).await
        });
        tokio::task::yield_now().await;
        let first = broker.report(report(session_id, 1)).await?;
        let replay = broker.report(report(session_id, 1)).await?;
        assert_eq!(first, replay);
        assert_eq!(first.commands.len(), 1);

        let command_id = first.commands[0].id;
        let mut completion = report(session_id, 2);
        completion.command_results.push(StudioBridgeCommandResult {
            command_id,
            succeeded: true,
            value: Some(serde_json::json!({"state": "edit"})),
            error_code: None,
            error_message: None,
        });
        broker.report(completion).await?;
        let call_result =
            call.await.map_err(|_| StudioBridgeError::CommandChannelClosed(command_id))??;
        assert!(call_result.succeeded);
        Ok(())
    }

    #[tokio::test]
    async fn commands_never_cross_studio_sessions() -> Result<(), StudioBridgeError> {
        let limits = StudioBridgeLimits {
            command_timeout: Duration::from_secs(2),
            ..StudioBridgeLimits::default()
        };
        let broker = Arc::new(StudioBridgeBroker::new(limits));
        let first = broker.register(registration("plugin-window-a")).await?;
        let second = broker.register(registration("plugin-window-b")).await?;
        for (session_id, fingerprint, process_id) in [
            (first.session_id, b"studio-a".as_slice(), 99),
            (second.session_id, b"studio-b".as_slice(), 100),
        ] {
            broker
                .bind_session(
                    session_id,
                    StudioBridgeBinding {
                        studio_session_id: LatticeId::new(),
                        environment_id: StudioEnvironmentId::from_fingerprint(fingerprint),
                        process_id,
                    },
                )
                .await?;
        }

        let calling_broker = Arc::clone(&broker);
        let call = tokio::spawn(async move {
            calling_broker.call(first.session_id, StudioBridgeCommandPayload::GetSelection).await
        });
        tokio::task::yield_now().await;
        assert!(broker.report(report(second.session_id, 1)).await?.commands.is_empty());
        let first_report = broker.report(report(first.session_id, 1)).await?;
        assert_eq!(first_report.commands.len(), 1);

        let command_id = first_report.commands[0].id;
        let mut completion = report(first.session_id, 2);
        completion.command_results.push(StudioBridgeCommandResult {
            command_id,
            succeeded: true,
            value: Some(serde_json::json!({"instances": []})),
            error_code: None,
            error_message: None,
        });
        broker.report(completion).await?;
        let result =
            call.await.map_err(|_| StudioBridgeError::CommandChannelClosed(command_id))??;
        assert!(result.succeeded);
        Ok(())
    }

    #[tokio::test]
    async fn event_retention_is_bounded_and_counted() -> Result<(), StudioBridgeError> {
        let limits = StudioBridgeLimits {
            max_events_per_session: 2,
            max_events_per_report: 3,
            ..StudioBridgeLimits::default()
        };
        let broker = StudioBridgeBroker::new(limits);
        let registered = broker.register(registration("plugin-window-a")).await?;
        let mut incoming = report(registered.session_id, 1);
        incoming.events = (1..=3)
            .map(|sequence| StudioBridgeEvent {
                event_id: format!("event-{sequence}"),
                sequence,
                observed_unix_ms: 0,
                payload: StudioBridgeEventPayload::Heartbeat,
            })
            .collect();
        let response = broker.report(incoming).await?;
        assert_eq!(response.dropped_events_total, 1);
        let events = broker.drain_events(registered.session_id, 10).await?;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sequence, 2);
        Ok(())
    }

    #[tokio::test]
    async fn server_rejects_non_loopback_binding() -> Result<(), StudioBridgeError> {
        let token = StudioBridgeAuthToken::new(vec![b'x'; 32])?;
        let result = bind_studio_bridge(
            StudioBridgeServerConfig {
                bind: "0.0.0.0:0"
                    .parse()
                    .map_err(|_| StudioBridgeError::InvalidAuthenticationConfiguration)?,
                token: Some(token),
            },
            Arc::new(StudioBridgeBroker::new(StudioBridgeLimits::default())),
        )
        .await;
        assert!(matches!(result, Err(StudioBridgeError::NonLoopbackBind(_))));
        Ok(())
    }

    #[test]
    fn authentication_tokens_are_redacted_and_compared() -> Result<(), StudioBridgeError> {
        let token = StudioBridgeAuthToken::new(b"01234567890123456789012345678901".to_vec())?;
        assert!(token.matches(b"01234567890123456789012345678901"));
        assert!(!token.matches(b"11234567890123456789012345678901"));
        assert_eq!(format!("{token:?}"), "StudioBridgeAuthToken([REDACTED])");
        Ok(())
    }

    async fn raw_http_request(
        address: SocketAddr,
        path: &str,
        authorization: Option<&str>,
        body: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut stream = tokio::net::TcpStream::connect(address).await?;
        let authorization = authorization
            .map(|value| format!("Authorization: Bearer {value}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\n{authorization}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await?;
        let mut response = String::new();
        stream.read_to_string(&mut response).await?;
        Ok(response)
    }

    async fn raw_http_get(
        address: SocketAddr,
        path: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut stream = tokio::net::TcpStream::connect(address).await?;
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await?;
        let mut response = String::new();
        stream.read_to_string(&mut response).await?;
        Ok(response)
    }

    fn response_body(response: &str) -> Result<&str, Box<dyn std::error::Error>> {
        response
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .ok_or_else(|| "HTTP response did not contain a body separator".into())
    }

    #[tokio::test]
    async fn loopback_http_requires_authentication_and_registers()
    -> Result<(), Box<dyn std::error::Error>> {
        let token_value = "01234567890123456789012345678901";
        let token = StudioBridgeAuthToken::new(token_value.as_bytes().to_vec())?;
        let broker = Arc::new(StudioBridgeBroker::new(StudioBridgeLimits::default()));
        let server = bind_studio_bridge(
            StudioBridgeServerConfig { bind: "127.0.0.1:0".parse()?, token: Some(token) },
            broker,
        )
        .await?;
        let address = server.local_address();
        let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.serve_until(async move {
            let _shutdown_result = shutdown_receiver.await;
        }));
        let body = serde_json::to_string(&registration("wire-session"))?;

        let rejected = raw_http_request(address, "/v1/studio-bridge/register", None, &body).await?;
        assert!(rejected.starts_with("HTTP/1.1 401 Unauthorized"));
        assert!(rejected.contains("STUDIO_BRIDGE_UNAUTHORIZED"));

        let accepted =
            raw_http_request(address, "/v1/studio-bridge/register", Some(token_value), &body)
                .await?;
        assert!(accepted.starts_with("HTTP/1.1 200 OK"));
        assert!(accepted.contains("\"protocol_version\":1"));
        assert!(accepted.contains("\"session_id\""));

        let _send_result = shutdown_sender.send(());
        task.await??;
        Ok(())
    }

    #[tokio::test]
    async fn loopback_http_auto_pairs_without_a_user_token()
    -> Result<(), Box<dyn std::error::Error>> {
        let broker = Arc::new(StudioBridgeBroker::new(StudioBridgeLimits::default()));
        let server = bind_studio_bridge(
            StudioBridgeServerConfig { bind: "127.0.0.1:0".parse()?, token: None },
            broker,
        )
        .await?;
        let address = server.local_address();
        let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.serve_until(async move {
            let _shutdown_result = shutdown_receiver.await;
        }));

        let discovery_response = raw_http_get(address, "/v1/studio-bridge/discover").await?;
        assert!(discovery_response.starts_with("HTTP/1.1 200 OK"));
        let discovery: StudioBridgeDiscoveryResponse =
            serde_json::from_str(response_body(&discovery_response)?)?;
        assert_eq!(discovery.authentication, "loopback_auto_pair");

        let pair_body = serde_json::to_string(&StudioBridgePairingRequest {
            challenge: discovery.challenge,
            client_kind: "roblox-studio-plugin".to_owned(),
            client_name: Some("LatticeCompanion".to_owned()),
        })?;
        let pair_response =
            raw_http_request(address, "/v1/studio-bridge/pair", None, &pair_body).await?;
        assert!(pair_response.starts_with("HTTP/1.1 200 OK"));
        let pairing: StudioBridgePairingResponse =
            serde_json::from_str(response_body(&pair_response)?)?;
        assert!(!pairing.session_token.is_empty());

        let registration_body = serde_json::to_string(&registration("auto-paired"))?;
        let registered = raw_http_request(
            address,
            "/v1/studio-bridge/register",
            Some(&pairing.session_token),
            &registration_body,
        )
        .await?;
        assert!(registered.starts_with("HTTP/1.1 200 OK"));

        let _send_result = shutdown_sender.send(());
        task.await??;
        Ok(())
    }
}
