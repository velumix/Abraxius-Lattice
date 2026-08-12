//! Daemon-owned Studio MCP lifecycle.
//!
//! The Studio companion bridge and Roblox's official Studio MCP are different
//! transports.  The companion is useful for plugin telemetry, while this
//! supervisor keeps the real Studio MCP stdio process alive and publishes its
//! verified health to the companion.  Launching is delegated entirely to the
//! platform-resolved launcher; this module never guesses Wine, Flatpak, or
//! deployment paths.

use std::{sync::Arc, time::Duration};

use lattice_mcp::{
    ResolvedStudioMcpProcessLauncher, StudioMcpProcessLauncher, StudioMcpSessionBinding,
};
use lattice_platform::PlatformResolver;
use lattice_resource::LatticeId;
use lattice_studio_bridge::{
    StudioBridgeBackendState, StudioBridgeBroker, StudioBridgeServiceHealth,
};
use tokio::{sync::Mutex, task::JoinHandle};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const HEALTH_INTERVAL: Duration = Duration::from_secs(2);
const RETRY_INTERVAL: Duration = Duration::from_secs(5);
const STARTUP_RETRY_INTERVAL: Duration = Duration::from_secs(2);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(3);

/// Starts the persistent, read-only Studio MCP monitor.
pub fn spawn(
    broker: Arc<StudioBridgeBroker>,
    backend: Arc<Mutex<StudioBridgeBackendState>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let launcher = ResolvedStudioMcpProcessLauncher;
        loop {
            if let Err(error) = run_connection(&launcher, &broker, &backend).await {
                tracing::warn!(%error, "Studio MCP connection unavailable");
                set_health(&broker, &backend, "unavailable", Some(error), None, true).await;
                tokio::time::sleep(RETRY_INTERVAL).await;
            } else {
                // A clean disconnect is still unexpected for the monitor, but
                // it is safe to retry without making the daemon unhealthy.
                tokio::time::sleep(STARTUP_RETRY_INTERVAL).await;
            }
        }
    })
}

async fn run_connection(
    process_launcher: &ResolvedStudioMcpProcessLauncher,
    broker: &Arc<StudioBridgeBroker>,
    backend: &Arc<Mutex<StudioBridgeBackendState>>,
) -> Result<(), String> {
    let inspection = PlatformResolver::current().inspect().map_err(|error| error.to_string())?;
    let selected = inspection
        .selected_environment
        .ok_or_else(|| "no uniquely selected Studio environment".to_owned())?;
    let environment = inspection
        .environments
        .into_iter()
        .find(|environment| environment.id == selected)
        .ok_or_else(|| "selected Studio environment disappeared".to_owned())?;
    let process = environment
        .studio_process
        .as_ref()
        .ok_or_else(|| "selected Studio environment has no running Studio process".to_owned())?;
    let binding = StudioMcpSessionBinding {
        studio_session_id: LatticeId::new(),
        environment_id: environment.id,
        process_id: process.pid,
    };

    set_health(
        broker,
        backend,
        "connecting",
        Some("Launching the resolved Studio MCP process".to_owned()),
        None,
        false,
    )
    .await;

    let mut launched = process_launcher
        .launch(&environment, binding, REQUEST_TIMEOUT)
        .await
        .map_err(|error| error.to_string())?;

    let (mut studio_id, place_name) = match wait_for_studio(&mut launched).await {
        Ok(studio) => studio,
        Err(error) => {
            let _ = launched.disconnect(CLEANUP_TIMEOUT).await;
            return Err(error);
        }
    };
    let mut place_name = place_name;
    set_place_name(broker, backend, place_name.clone()).await;

    let state_result = launched
        .client_mut()
        .call_tool("get_studio_state", serde_json::json!({ "studio_id": studio_id }))
        .await
        .map_err(|error| error.to_string());
    let state_result = match state_result {
        Ok(result) => result,
        Err(error) => {
            let stderr = launched.stderr().await;
            let _ = launched.disconnect(CLEANUP_TIMEOUT).await;
            return Err(format!("{error}; stderr: {}", stderr.trim()));
        }
    };
    let snapshot = launched.snapshot().clone();
    let latency_ms = micros_to_millis(snapshot.last_rtt_micros);
    set_health(
        broker,
        backend,
        "connected",
        Some(format!(
            "Studio MCP · protocol {} · {} tools · state verified ({})",
            snapshot.protocol_version,
            snapshot.tools.len(),
            result_summary(&state_result.value)
        )),
        latency_ms,
        false,
    )
    .await;

    loop {
        tokio::time::sleep(HEALTH_INTERVAL).await;
        let studio = match launched
            .client_mut()
            .call_tool("list_roblox_studios", serde_json::json!({}))
            .await
        {
            Ok(result) => match studio_descriptors(&result.value)?.as_slice() {
                [only] => only.clone(),
                [] => return Err("Studio MCP no longer reports a running Studio".to_owned()),
                many => {
                    return Err(format!(
                        "Studio MCP returned {} Studio sessions; refusing ambiguous binding",
                        many.len()
                    ));
                }
            },
            Err(error) => {
                let stderr = launched.stderr().await;
                let _ = launched.disconnect(CLEANUP_TIMEOUT).await;
                return Err(format!(
                    "Studio listing health request failed: {error}; stderr: {}",
                    stderr.trim()
                ));
            }
        };
        if studio.id != studio_id || studio.name != place_name {
            studio_id = studio.id;
            place_name = studio.name;
            set_place_name(broker, backend, place_name.clone()).await;
        }
        match launched
            .client_mut()
            .call_tool("get_studio_state", serde_json::json!({ "studio_id": studio_id }))
            .await
        {
            Ok(result) => {
                let snapshot = launched.snapshot();
                set_health(
                    broker,
                    backend,
                    "connected",
                    Some(format!(
                        "Studio MCP · protocol {} · {} tools · state verified ({})",
                        snapshot.protocol_version,
                        snapshot.tools.len(),
                        result_summary(&result.value)
                    )),
                    micros_to_millis(Some(result.rtt_micros)),
                    false,
                )
                .await;
            }
            Err(error) => {
                let stderr = launched.stderr().await;
                let _ = launched.disconnect(CLEANUP_TIMEOUT).await;
                return Err(format!("health request failed: {error}; stderr: {}", stderr.trim()));
            }
        }
    }
}

async fn wait_for_studio(
    launched: &mut lattice_mcp::LaunchedStudioMcpClient,
) -> Result<(String, Option<String>), String> {
    for _ in 0..20 {
        let result = launched
            .client_mut()
            .call_tool("list_roblox_studios", serde_json::json!({}))
            .await
            .map_err(|error| error.to_string())?;
        let studios = studio_descriptors(&result.value)?;
        match studios.as_slice() {
            [only] => return Ok((only.id.clone(), only.name.clone())),
            [] => tokio::time::sleep(Duration::from_millis(500)).await,
            many => {
                return Err(format!(
                    "Studio MCP returned {} Studio sessions; refusing ambiguous binding",
                    many.len()
                ));
            }
        }
    }
    Err("Studio MCP did not register the running Studio within 10 seconds".to_owned())
}

#[derive(Clone, Debug)]
struct StudioDescriptor {
    id: String,
    name: Option<String>,
}

fn studio_descriptors(value: &serde_json::Value) -> Result<Vec<StudioDescriptor>, String> {
    let text = value
        .pointer("/content/0/text")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "list_roblox_studios returned no text payload".to_owned())?;
    let decoded: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| format!("invalid Studio MCP payload: {error}"))?;
    decoded
        .get("studios")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "list_roblox_studios returned no studios array".to_owned())?
        .iter()
        .map(|studio| {
            let id = studio
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| "Studio MCP result has a Studio without an id".to_owned())?;
            let name = studio.get("name").and_then(serde_json::Value::as_str).map(str::to_owned);
            Ok(StudioDescriptor { id, name })
        })
        .collect()
}

fn micros_to_millis(micros: Option<u64>) -> Option<u64> {
    micros.map(|value| value.div_ceil(1_000))
}

fn result_summary(value: &serde_json::Value) -> String {
    value
        .pointer("/content/0/text")
        .and_then(serde_json::Value::as_str)
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .and_then(|value| value.get("mode").and_then(serde_json::Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| "response received".to_owned())
}

async fn set_health(
    broker: &Arc<StudioBridgeBroker>,
    backend: &Arc<Mutex<StudioBridgeBackendState>>,
    state: &str,
    detail: Option<String>,
    latency_ms: Option<u64>,
    record_error: bool,
) {
    let mut state_snapshot = backend.lock().await;
    state_snapshot.mcp = StudioBridgeServiceHealth { state: state.to_owned(), detail, latency_ms };
    if state == "unavailable" {
        state_snapshot.place_name = None;
    }
    if record_error {
        if let Some(error) = state_snapshot.mcp.detail.clone() {
            state_snapshot.errors.retain(|entry| !entry.starts_with("Studio MCP: "));
            state_snapshot.errors.push(format!("Studio MCP: {error}"));
            state_snapshot.errors.truncate(8);
        }
    } else {
        state_snapshot.errors.retain(|entry| !entry.starts_with("Studio MCP: "));
    }
    broker.set_backend_state(state_snapshot.clone()).await;
}

async fn set_place_name(
    broker: &Arc<StudioBridgeBroker>,
    backend: &Arc<Mutex<StudioBridgeBackendState>>,
    place_name: Option<String>,
) {
    let mut state_snapshot = backend.lock().await;
    state_snapshot.place_name = place_name;
    broker.set_backend_state(state_snapshot.clone()).await;
}
