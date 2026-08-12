//! Protocol-neutral Studio adapter boundary. RMCP types must not cross this crate.

use std::collections::BTreeSet;

use lattice_platform::{
    HostPlatform, PathAvailability, StudioEnvironment, StudioEnvironmentId, StudioPathRole,
};
use lattice_resource::LatticeId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RobloxCapability {
    ReadSource,
    WriteSource,
    InspectDataModel,
    ExecuteLuau,
    Playtest,
    CaptureViewport,
    ReadRuntimeLogs,
    InsertAsset,
    Publish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudioState {
    Edit,
    Client,
    Server,
    Unknown,
}

/// Lifecycle of one southbound MCP connection. A discovered executable is not
/// an available transport and therefore never implies `Available`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudioMcpConnectionState {
    Unavailable,
    Discovering,
    Available,
    Connecting,
    Initializing,
    Connected,
    Degraded,
    Reconnecting,
    Disconnected,
    Failed,
}

impl StudioMcpConnectionState {
    #[must_use]
    pub const fn permits_requests(self) -> bool {
        matches!(self, Self::Connected | Self::Degraded)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudioMcpTransportKind {
    Stdio,
    StreamableHttp,
    UnixSocket,
    NamedPipe,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudioMcpEndpointAvailability {
    /// A supported endpoint is independently reachable without starting Studio.
    Attachable,
    /// A documented native launcher exists, but it has not been executed.
    LauncherAvailable,
    /// `StudioMCP` exists on disk but no transport is observable.
    BinaryArtifactOnly,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioMcpEndpointDiagnostic {
    pub environment_id: StudioEnvironmentId,
    pub process_id: Option<u32>,
    pub server_process_observed: bool,
    pub availability: StudioMcpEndpointAvailability,
    pub connection_state: StudioMcpConnectionState,
    pub transport: Option<StudioMcpTransportKind>,
    pub code: String,
    pub message: String,
}

/// Reports only transport evidence that is observable without launching a
/// process. In particular, Linux/Vinegar `StudioMCP.exe` artifacts are never
/// promoted to attachable endpoints.
#[must_use]
pub fn diagnose_mcp_endpoint(environment: &StudioEnvironment) -> StudioMcpEndpointDiagnostic {
    let process_id = environment.studio_process.as_ref().map(|process| process.pid);
    let server_process_observed = environment.related_processes.iter().any(|process| {
        process.executable.as_ref().is_some_and(|executable| {
            executable
                .as_path()
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("StudioMCP.exe"))
        })
    });
    if environment.mcp_launcher.is_some() {
        let experimental =
            environment.mcp_launcher.as_ref().is_some_and(|launcher| launcher.experimental);
        return StudioMcpEndpointDiagnostic {
            environment_id: environment.id,
            process_id,
            server_process_observed,
            availability: StudioMcpEndpointAvailability::LauncherAvailable,
            connection_state: StudioMcpConnectionState::Unavailable,
            transport: Some(StudioMcpTransportKind::Stdio),
            code: "STUDIO_MCP_LAUNCHER_AVAILABLE".into(),
            message: if experimental {
                "an experimental platform-resolved launcher is present; connection requires an explicit launch request"
                    .into()
            } else {
                "a documented native launcher is present; it has not been executed".into()
            },
        };
    }
    let artifact_exists = environment
        .path(StudioPathRole::McpServer)
        .is_some_and(|path| path.availability == PathAvailability::Available);
    if artifact_exists {
        let linux_detail = if environment.host_platform == HostPlatform::Linux {
            " Linux/Vinegar has no documented launch or attach command, so Lattice will not execute it."
        } else {
            ""
        };
        return StudioMcpEndpointDiagnostic {
            environment_id: environment.id,
            process_id,
            server_process_observed,
            availability: StudioMcpEndpointAvailability::BinaryArtifactOnly,
            connection_state: StudioMcpConnectionState::Unavailable,
            transport: None,
            code: "STUDIO_MCP_ENDPOINT_UNAVAILABLE".into(),
            message: format!(
                "StudioMCP is present on disk, but no independently reachable transport was discovered.{linux_detail}"
            ),
        };
    }
    StudioMcpEndpointDiagnostic {
        environment_id: environment.id,
        process_id,
        server_process_observed,
        availability: StudioMcpEndpointAvailability::Unavailable,
        connection_state: StudioMcpConnectionState::Unavailable,
        transport: None,
        code: "STUDIO_MCP_UNAVAILABLE".into(),
        message: "no Studio MCP launcher, binary artifact, or attachable endpoint was discovered"
            .into(),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StudioMcpToolDescriptor {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StudioMcpConnectionSnapshot {
    pub id: LatticeId,
    pub studio_session_id: LatticeId,
    pub environment_id: StudioEnvironmentId,
    pub process_id: u32,
    pub state: StudioMcpConnectionState,
    pub transport: StudioMcpTransportKind,
    pub endpoint_identity: String,
    pub protocol_version: String,
    pub protocol_negotiation: String,
    pub protocol_session_model: String,
    pub protocol_fallback_reason: Option<String>,
    pub server_name: String,
    pub server_version: String,
    pub capabilities: Vec<String>,
    pub tools: Vec<StudioMcpToolDescriptor>,
    pub tool_catalog_revision: String,
    pub connected_at_unix_ms: i64,
    pub last_successful_request_unix_ms: Option<i64>,
    pub last_rtt_micros: Option<u64>,
    pub failure_count: u64,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StudioMcpToolResult {
    pub value: serde_json::Value,
    pub rtt_micros: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StudioSession {
    pub id: LatticeId,
    pub external_id: String,
    pub place_label: Option<String>,
    pub state: StudioState,
    pub environment_id: Option<StudioEnvironmentId>,
    pub process_id: Option<u32>,
    pub capabilities: BTreeSet<RobloxCapability>,
    /// Whether the Studio session/process association is live. Southbound MCP
    /// transport readiness is represented by `mcp_connection.state`.
    pub connected: bool,
    pub mcp_connection: Option<StudioMcpConnectionSnapshot>,
    pub last_heartbeat_unix_ms: Option<i64>,
}

#[derive(Default)]
pub struct StudioManager {
    sessions: Vec<StudioSession>,
}

impl StudioManager {
    #[must_use]
    pub fn sessions(&self) -> &[StudioSession] {
        &self.sessions
    }

    pub fn replace_sessions(&mut self, sessions: Vec<StudioSession>) {
        self.sessions = sessions;
    }

    /// Associates one MCP session with one resolved process environment.
    ///
    /// # Errors
    ///
    /// Returns [`StudioError::SessionNotFound`] if the session is unknown.
    pub fn associate_environment(
        &mut self,
        session_id: LatticeId,
        environment_id: StudioEnvironmentId,
        process_id: Option<u32>,
    ) -> Result<(), StudioError> {
        let session = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
            .ok_or(StudioError::SessionNotFound(session_id))?;
        session.environment_id = Some(environment_id);
        session.process_id = process_id;
        Ok(())
    }

    /// Binds a successfully initialized MCP connection to exactly one Studio
    /// session and verifies its environment/process identity.
    pub fn bind_mcp_connection(
        &mut self,
        snapshot: StudioMcpConnectionSnapshot,
    ) -> Result<(), StudioError> {
        let session = self
            .sessions
            .iter_mut()
            .find(|session| session.id == snapshot.studio_session_id)
            .ok_or(StudioError::SessionNotFound(snapshot.studio_session_id))?;
        if session.environment_id != Some(snapshot.environment_id)
            || session.process_id != Some(snapshot.process_id)
        {
            return Err(StudioError::ConnectionBindingMismatch);
        }
        if !snapshot.state.permits_requests() {
            return Err(StudioError::ConnectionNotReady(snapshot.state));
        }
        session.mcp_connection = Some(snapshot);
        Ok(())
    }

    /// Resolves one connected Studio session and rejects ambiguity.
    ///
    /// # Errors
    ///
    /// Returns [`StudioError::NotConnected`] when none are live,
    /// [`StudioError::AmbiguousSession`] when a target is required, or
    /// [`StudioError::SessionNotFound`] for an unavailable explicit target.
    pub fn explicit_target(
        &self,
        requested: Option<LatticeId>,
    ) -> Result<&StudioSession, StudioError> {
        if let Some(requested) = requested {
            return self
                .sessions
                .iter()
                .find(|session| {
                    session.id == requested
                        && session.connected
                        && session
                            .mcp_connection
                            .as_ref()
                            .is_some_and(|connection| connection.state.permits_requests())
                })
                .ok_or(StudioError::SessionNotFound(requested));
        }
        let mut connected = self.sessions.iter().filter(|session| {
            session.connected
                && session
                    .mcp_connection
                    .as_ref()
                    .is_some_and(|connection| connection.state.permits_requests())
        });
        let first = connected.next().ok_or(StudioError::NotConnected)?;
        if connected.next().is_some() {
            return Err(StudioError::AmbiguousSession);
        }
        Ok(first)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum StudioError {
    #[error("STUDIO_NOT_CONNECTED: no Roblox Studio MCP launcher/session is available")]
    NotConnected,
    #[error(
        "AMBIGUOUS_RESOURCE: more than one Studio session is connected; provide a target session"
    )]
    AmbiguousSession,
    #[error("RESOURCE_NOT_FOUND: Studio session {0} is unavailable")]
    SessionNotFound(LatticeId),
    #[error("STUDIO_CAPABILITY_UNAVAILABLE: session does not advertise {0:?}")]
    CapabilityUnavailable(RobloxCapability),
    #[error(
        "STUDIO_CONNECTION_UNBOUND: MCP connection does not match the Studio environment/process"
    )]
    ConnectionBindingMismatch,
    #[error("STUDIO_NOT_CONNECTED: MCP connection state {0:?} cannot accept requests")]
    ConnectionNotReady(StudioMcpConnectionState),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> StudioSession {
        let id = LatticeId::new();
        let environment_id = StudioEnvironmentId::from_fingerprint(b"studio-one");
        StudioSession {
            id,
            external_id: "studio".into(),
            place_label: None,
            state: StudioState::Edit,
            environment_id: Some(environment_id),
            process_id: Some(77),
            capabilities: BTreeSet::new(),
            connected: true,
            mcp_connection: Some(connection(id, environment_id, 77)),
            last_heartbeat_unix_ms: None,
        }
    }

    fn connection(
        studio_session_id: LatticeId,
        environment_id: StudioEnvironmentId,
        process_id: u32,
    ) -> StudioMcpConnectionSnapshot {
        StudioMcpConnectionSnapshot {
            id: LatticeId::new(),
            studio_session_id,
            environment_id,
            process_id,
            state: StudioMcpConnectionState::Connected,
            transport: StudioMcpTransportKind::Stdio,
            endpoint_identity: "test".into(),
            protocol_version: "test".into(),
            protocol_negotiation: "test".into(),
            protocol_session_model: "test".into(),
            protocol_fallback_reason: None,
            server_name: "test".into(),
            server_version: "1".into(),
            capabilities: Vec::new(),
            tools: Vec::new(),
            tool_catalog_revision: "b3:test".into(),
            connected_at_unix_ms: 0,
            last_successful_request_unix_ms: None,
            last_rtt_micros: None,
            failure_count: 0,
            last_error: None,
        }
    }

    #[test]
    fn ambiguity_requires_an_explicit_session() {
        let one = session();
        let two = session();
        let mut manager = StudioManager::default();
        manager.replace_sessions(vec![one.clone(), two]);
        assert_eq!(manager.explicit_target(None), Err(StudioError::AmbiguousSession));
        assert_eq!(manager.explicit_target(Some(one.id)), Ok(&one));
    }

    #[test]
    fn session_association_is_instance_specific() -> Result<(), StudioError> {
        let one = session();
        let one_id = one.id;
        let environment = StudioEnvironmentId::from_fingerprint(b"studio-one");
        let mut manager = StudioManager::default();
        manager.replace_sessions(vec![one]);
        manager.associate_environment(one_id, environment, Some(77))?;
        let associated = manager.explicit_target(Some(one_id))?;
        assert_eq!(associated.environment_id, Some(environment));
        assert_eq!(associated.process_id, Some(77));
        Ok(())
    }

    #[test]
    fn connection_binding_rejects_the_wrong_process() {
        let one = session();
        let environment_id = one.environment_id;
        assert!(environment_id.is_some());
        let snapshot = connection(
            one.id,
            environment_id.unwrap_or_else(|| StudioEnvironmentId::from_fingerprint(b"invalid")),
            88,
        );
        let mut manager = StudioManager::default();
        manager.replace_sessions(vec![one]);
        assert_eq!(
            manager.bind_mcp_connection(snapshot),
            Err(StudioError::ConnectionBindingMismatch)
        );
    }
}
