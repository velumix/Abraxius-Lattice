use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use clap::Parser;
use lattice_mcp::{DaemonListener, LatticeOperations, LocalOperations};
use lattice_platform::PlatformResolver;
use lattice_studio_bridge::{
    StudioBridgeAuthToken, StudioBridgeBackendState, StudioBridgeBroker,
    StudioBridgeEnvironmentSummary, StudioBridgeLimits, StudioBridgeServerConfig,
    bind_studio_bridge,
};
use tracing_subscriber::EnvFilter;

mod studio_mcp_supervisor;

const STUDIO_BRIDGE_TOKEN_ENV: &str = "LATTICE_STUDIO_BRIDGE_TOKEN";

#[derive(Debug, Parser)]
#[command(name = "lattice-daemon", version, about = "Abraxius Lattice native daemon")]
struct Arguments {
    #[arg(long)]
    workspace: Option<PathBuf>,

    /// Starts only the loopback Studio companion bridge. This is used by the
    /// desktop shell before a workspace has been opened; it does not fabricate
    /// project/index state.
    #[arg(long, conflicts_with = "workspace")]
    studio_bridge_only: bool,

    /// Enables the optional Studio companion transport on a loopback address.
    /// The Roblox plugin discovers and pairs locally; the legacy environment
    /// token remains an optional compatibility path and is never read from argv.
    #[arg(long)]
    studio_bridge_bind: Option<SocketAddr>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let arguments = Arguments::parse();
    if !arguments.studio_bridge_only && arguments.workspace.is_none() {
        return Err("LATTICE_WORKSPACE_REQUIRED: pass --workspace or --studio-bridge-only".into());
    }
    if arguments.studio_bridge_only && arguments.studio_bridge_bind.is_none() {
        return Err(
            "STUDIO_BRIDGE_BIND_REQUIRED: bridge-only mode requires --studio-bridge-bind".into()
        );
    }
    let (_operations, mut ipc_task) = if let Some(workspace) = arguments.workspace.as_ref() {
        let operations: Arc<dyn LatticeOperations> = Arc::new(LocalOperations::open(workspace)?);
        let status = operations.workspace_status()?;
        tracing::info!(
            workspace_id = %status.workspace_id,
            sources = status.source_count,
            revision = status.revision,
            "Lattice daemon ready"
        );
        println!("{}", serde_json::to_string(&status)?);
        let listener = DaemonListener::bind().await?;
        let endpoint = listener.endpoint_info()?;
        tracing::info!(
            transport = %endpoint.transport,
            address = %endpoint.address,
            "Lattice daemon MCP IPC ready"
        );
        let task = tokio::spawn(lattice_mcp::serve_daemon_ipc(listener, Arc::clone(&operations)));
        (Some(operations), Some(task))
    } else {
        tracing::info!("Lattice daemon running in Studio bridge-only mode");
        (None, None)
    };

    let bridge = if let Some(bind) = arguments.studio_bridge_bind {
        let token = std::env::var(STUDIO_BRIDGE_TOKEN_ENV)
            .ok()
            .map(|value| StudioBridgeAuthToken::new(value.into_bytes()))
            .transpose()?;
        let legacy_token = token.is_some();
        let broker = Arc::new(StudioBridgeBroker::new(StudioBridgeLimits::default()));
        let mut backend = StudioBridgeBackendState {
            index_state: Some("ready".to_owned()),
            capabilities: vec![
                "studio:state.read@1".to_owned(),
                "studio:selection.read@1".to_owned(),
                "roblox:source.read@1".to_owned(),
            ],
            ..StudioBridgeBackendState::default()
        };
        match PlatformResolver::current().inspect() {
            Ok(inspection) => {
                backend.environment = inspection.selected_environment.and_then(|selected| {
                    inspection
                        .environments
                        .into_iter()
                        .find(|environment| environment.id == selected)
                        .map(|environment| StudioBridgeEnvironmentSummary {
                            host_platform: environment.host_platform,
                            runtime: environment.runtime,
                            environment_id: environment.id,
                            process_id: environment
                                .studio_process
                                .as_ref()
                                .map(|process| process.pid),
                            deployment: environment
                                .studio_deployment
                                .and_then(|deployment| deployment.build_identifier),
                        })
                });
            }
            Err(error) => {
                backend.errors.push(format!("Studio environment is unavailable: {error}"));
            }
        }
        let backend = Arc::new(tokio::sync::Mutex::new(backend));
        broker.set_backend_state(backend.lock().await.clone()).await;
        let server =
            bind_studio_bridge(StudioBridgeServerConfig { bind, token }, Arc::clone(&broker))
                .await?;
        tracing::info!(
            address = %server.local_address(),
            auto_pairing = true,
            legacy_token,
            "Studio companion bridge ready for local auto-discovery"
        );
        let mcp_task = studio_mcp_supervisor::spawn(Arc::clone(&broker), Arc::clone(&backend));
        Some((server, broker, mcp_task))
    } else {
        None
    };

    if let Some((server, _broker, mcp_task)) = bridge {
        let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel();
        let mut bridge_task = tokio::spawn(server.serve_until(async move {
            let _shutdown_result = shutdown_receiver.await;
        }));
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal?;
                let _send_result = shutdown_sender.send(());
                bridge_task.await??;
                if let Some(task) = ipc_task.take() {
                    task.abort();
                }
                mcp_task.abort();
            }
            bridge_result = &mut bridge_task => {
                match bridge_result {
                    Ok(Ok(())) => tracing::warn!("Studio companion bridge stopped unexpectedly"),
                    Ok(Err(error)) => tracing::error!(%error, "Studio companion bridge failed"),
                    Err(error) => tracing::error!(%error, "Studio companion bridge task failed"),
                }
                // The optional provider is isolated: its failure does not take
                // down indexing, northbound MCP, or the daemon.
                tokio::signal::ctrl_c().await?;
                if let Some(task) = ipc_task.take() {
                    task.abort();
                }
                mcp_task.abort();
            }
        }
    } else {
        tokio::signal::ctrl_c().await?;
        if let Some(task) = ipc_task.take() {
            task.abort();
        }
    }
    tracing::info!("shutdown signal received");
    Ok(())
}
