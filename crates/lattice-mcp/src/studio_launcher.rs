use std::{
    collections::BTreeMap, future::Future, pin::Pin, process::Stdio, sync::Arc, time::Duration,
};

use lattice_platform::{StudioEnvironment, StudioMcpLauncher as StudioMcpLaunchSpec};
use lattice_studio::{StudioMcpConnectionSnapshot, StudioMcpTransportKind};
use rmcp::{RoleClient, transport::async_rw::AsyncRwTransport};
use thiserror::Error;
use tokio::{
    io::AsyncReadExt,
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
    task::JoinHandle,
};

use crate::{McpStartupMode, StudioMcpClient, StudioMcpClientError, StudioMcpSessionBinding};

const STDERR_LIMIT: usize = 1024 * 1024;

pub trait StudioMcpProcessLauncher: Send + Sync {
    fn launch<'a>(
        &'a self,
        environment: &'a StudioEnvironment,
        binding: StudioMcpSessionBinding,
        request_timeout: Duration,
    ) -> Pin<
        Box<dyn Future<Output = Result<LaunchedStudioMcpClient, StudioMcpLaunchError>> + Send + 'a>,
    >;
}

/// Launches only the already-resolved Studio MCP command. It never starts or
/// restarts Roblox Studio and never searches for platform paths itself.
#[derive(Clone, Copy, Debug, Default)]
pub struct ResolvedStudioMcpProcessLauncher;

impl StudioMcpProcessLauncher for ResolvedStudioMcpProcessLauncher {
    fn launch<'a>(
        &'a self,
        environment: &'a StudioEnvironment,
        binding: StudioMcpSessionBinding,
        request_timeout: Duration,
    ) -> Pin<
        Box<dyn Future<Output = Result<LaunchedStudioMcpClient, StudioMcpLaunchError>> + Send + 'a>,
    > {
        Box::pin(async move {
            if binding.environment_id != environment.id {
                return Err(StudioMcpLaunchError::EnvironmentMismatch);
            }
            let process = environment
                .studio_process
                .as_ref()
                .ok_or(StudioMcpLaunchError::StudioNotRunning)?;
            if binding.process_id != process.pid {
                return Err(StudioMcpLaunchError::ProcessMismatch);
            }
            let launcher =
                environment.mcp_launcher.as_ref().ok_or(StudioMcpLaunchError::LaunchUnavailable)?;
            validate_launch_spec(launcher)?;
            let endpoint = format!(
                "stdio:{}:{}",
                environment.id,
                launcher
                    .target_executable
                    .as_ref()
                    .and_then(|target| target.guest.as_ref())
                    .map_or_else(|| launcher.executable.to_string(), ToString::to_string)
            );

            let first = spawn_resolved(launcher)?;
            let first_result = StudioMcpClient::connect_with_startup(
                first.transport,
                binding,
                StudioMcpTransportKind::Stdio,
                endpoint.clone(),
                request_timeout,
                McpStartupMode::Automatic,
            )
            .await;
            match first_result {
                Ok(client) => Ok(LaunchedStudioMcpClient {
                    client,
                    child: first.child,
                    stderr_task: first.stderr_task,
                    stderr_capture: first.stderr_capture,
                }),
                Err(source) => {
                    let stderr = cleanup_failed_attempt(
                        first.child,
                        first.stderr_task,
                        &first.stderr_capture,
                    )
                    .await;
                    if !legacy_studio_evidence(&source, &stderr) {
                        return Err(StudioMcpLaunchError::Negotiation { source, stderr });
                    }

                    // StudioMCP versions observed in Vinegar close the process
                    // after explicitly reporting that initialize was expected.
                    // Relaunching is intentionally limited to this exact,
                    // provider-originated evidence; arbitrary protocol errors
                    // never trigger a downgrade.
                    let fallback_reason = format!(
                        "server/discover child closed after StudioMCP explicitly required initialize; first-attempt stderr: {}",
                        stderr.trim()
                    );
                    let second = spawn_resolved(launcher)?;
                    match StudioMcpClient::connect_with_startup(
                        second.transport,
                        binding,
                        StudioMcpTransportKind::Stdio,
                        endpoint,
                        request_timeout,
                        McpStartupMode::VerifiedLegacyFallback { reason: fallback_reason },
                    )
                    .await
                    {
                        Ok(client) => Ok(LaunchedStudioMcpClient {
                            client,
                            child: second.child,
                            stderr_task: second.stderr_task,
                            stderr_capture: second.stderr_capture,
                        }),
                        Err(source) => {
                            let stderr = cleanup_failed_attempt(
                                second.child,
                                second.stderr_task,
                                &second.stderr_capture,
                            )
                            .await;
                            Err(StudioMcpLaunchError::Negotiation { source, stderr })
                        }
                    }
                }
            }
        })
    }
}

struct SpawnedStudioMcp {
    child: Child,
    stderr_task: JoinHandle<()>,
    stderr_capture: Arc<Mutex<Vec<u8>>>,
    transport: AsyncRwTransport<RoleClient, ChildStdout, ChildStdin>,
}

fn validate_launch_spec(launcher: &StudioMcpLaunchSpec) -> Result<(), StudioMcpLaunchError> {
    if !launcher.executable.as_path().is_file() {
        return Err(StudioMcpLaunchError::LauncherMissing);
    }
    if let Some(target) = launcher.target_executable.as_ref().and_then(|path| path.host.as_ref())
        && !target.as_path().is_file()
    {
        return Err(StudioMcpLaunchError::TargetMissing);
    }
    Ok(())
}

fn spawn_resolved(
    launcher: &StudioMcpLaunchSpec,
) -> Result<SpawnedStudioMcp, StudioMcpLaunchError> {
    let mut command = Command::new(launcher.executable.as_path());
    command
        .args(&launcher.arguments)
        .env_clear()
        .envs(allowed_studio_environment())
        .envs(&launcher.environment)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child =
        command.spawn().map_err(|error| StudioMcpLaunchError::Spawn(error.to_string()))?;
    let stdin = child.stdin.take().ok_or(StudioMcpLaunchError::MissingStdin)?;
    let stdout = child.stdout.take().ok_or(StudioMcpLaunchError::MissingStdout)?;
    let stderr = child.stderr.take().ok_or(StudioMcpLaunchError::MissingStderr)?;
    let stderr_capture = Arc::new(Mutex::new(Vec::new()));
    let stderr_task = tokio::spawn(drain_stderr(stderr, Arc::clone(&stderr_capture)));
    let transport = AsyncRwTransport::new_client(stdout, stdin);
    Ok(SpawnedStudioMcp { child, stderr_task, stderr_capture, transport })
}

fn legacy_studio_evidence(source: &StudioMcpClientError, stderr: &str) -> bool {
    matches!(source, StudioMcpClientError::Initialize(_))
        && stderr.contains("expect initialized request")
        && stderr.contains("server/discover")
}

async fn cleanup_failed_attempt(
    mut child: Child,
    stderr_task: JoinHandle<()>,
    capture: &Arc<Mutex<Vec<u8>>>,
) -> String {
    let _ = child.kill().await;
    let _ = child.wait().await;
    let _ = tokio::time::timeout(Duration::from_secs(1), stderr_task).await;
    bounded_stderr(capture).await
}

pub struct LaunchedStudioMcpClient {
    client: StudioMcpClient,
    child: Child,
    stderr_task: JoinHandle<()>,
    stderr_capture: Arc<Mutex<Vec<u8>>>,
}

impl LaunchedStudioMcpClient {
    #[must_use]
    pub fn snapshot(&self) -> &StudioMcpConnectionSnapshot {
        self.client.snapshot()
    }

    pub fn client_mut(&mut self) -> &mut StudioMcpClient {
        &mut self.client
    }

    pub async fn stderr(&self) -> String {
        bounded_stderr(&self.stderr_capture).await
    }

    pub async fn disconnect(mut self, timeout: Duration) -> Result<(), StudioMcpLaunchError> {
        let client_result = self.client.disconnect(timeout).await;
        let exited = tokio::time::timeout(timeout, self.child.wait()).await;
        if !matches!(exited, Ok(Ok(_))) {
            self.child
                .kill()
                .await
                .map_err(|error| StudioMcpLaunchError::Cleanup(error.to_string()))?;
            self.child
                .wait()
                .await
                .map_err(|error| StudioMcpLaunchError::Cleanup(error.to_string()))?;
        }
        self.stderr_task.abort();
        client_result.map_err(StudioMcpLaunchError::Client)
    }
}

fn allowed_studio_environment() -> BTreeMap<String, String> {
    const ALLOWLIST: &[&str] = &[
        "DBUS_SESSION_BUS_ADDRESS",
        "DISPLAY",
        "HOME",
        "LANG",
        "LC_ALL",
        "PATH",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
    ];
    ALLOWLIST
        .iter()
        .filter_map(|name| std::env::var(name).ok().map(|value| ((*name).to_owned(), value)))
        .collect()
}

async fn drain_stderr(mut stderr: tokio::process::ChildStderr, capture: Arc<Mutex<Vec<u8>>>) {
    let mut buffer = [0_u8; 8192];
    loop {
        let Ok(read) = stderr.read(&mut buffer).await else {
            return;
        };
        if read == 0 {
            return;
        }
        let mut retained = capture.lock().await;
        let remaining = STDERR_LIMIT.saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..read.min(remaining)]);
    }
}

async fn bounded_stderr(capture: &Arc<Mutex<Vec<u8>>>) -> String {
    String::from_utf8_lossy(&capture.lock().await).into_owned()
}

#[derive(Debug, Error)]
pub enum StudioMcpLaunchError {
    #[error("STUDIO_MCP_ENVIRONMENT_MISMATCH: binding does not identify this environment")]
    EnvironmentMismatch,
    #[error("STUDIO_NOT_CONNECTED: the resolved Studio environment has no running process")]
    StudioNotRunning,
    #[error("STUDIO_MCP_PROCESS_MISMATCH: binding does not identify this Studio process")]
    ProcessMismatch,
    #[error("STUDIO_MCP_LAUNCH_UNAVAILABLE: no verified command is resolved")]
    LaunchUnavailable,
    #[error("STUDIO_MCP_LAUNCHER_MISSING: resolved launcher no longer exists")]
    LauncherMissing,
    #[error("STUDIO_MCP_TARGET_MISSING: resolved StudioMCP executable no longer exists")]
    TargetMissing,
    #[error("STUDIO_MCP_SPAWN_FAILED: {0}")]
    Spawn(String),
    #[error("STUDIO_MCP_TRANSPORT_FAILED: child stdin is unavailable")]
    MissingStdin,
    #[error("STUDIO_MCP_TRANSPORT_FAILED: child stdout is unavailable")]
    MissingStdout,
    #[error("STUDIO_MCP_TRANSPORT_FAILED: child stderr is unavailable")]
    MissingStderr,
    #[error("STUDIO_MCP_NEGOTIATION_FAILED: {source}; stderr: {stderr}")]
    Negotiation { source: StudioMcpClientError, stderr: String },
    #[error("STUDIO_MCP_CLIENT_FAILED: {0}")]
    Client(StudioMcpClientError),
    #[error("STUDIO_MCP_CLEANUP_FAILED: {0}")]
    Cleanup(String),
}
