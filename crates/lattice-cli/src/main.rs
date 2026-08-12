use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use lattice_core::Lattice;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "lattice", version, about = "Abraxius Lattice native CLI")]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the headless native service for one workspace.
    Daemon {
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Manage canonical workspaces.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// Incrementally ingest and index a workspace.
    Index { workspace: PathBuf },
    /// Search indexed project names, symbols, paths, and source.
    Search {
        workspace: PathBuf,
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Inspect discoverable Roblox Studio MCP launchers/sessions.
    Studio {
        #[command(subcommand)]
        command: StudioCommand,
    },
    /// Inspect universal tool providers.
    Provider {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// Progressively discover canonical provider tools.
    Tool {
        #[command(subcommand)]
        command: ToolCommand,
    },
    /// Inspect versioned semantic capabilities.
    Capability {
        #[command(subcommand)]
        command: CapabilityCommand,
    },
    /// Legacy workspace-local MCP server; use `mcp stdio` for the daemon-backed server.
    #[command(hide = true)]
    McpServe { workspace: PathBuf },
    /// Connect Codex and other MCP clients to the authoritative daemon.
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    /// Configure Codex CLI to use Lattice from any editor terminal.
    Integration {
        #[command(subcommand)]
        command: IntegrationCommand,
    },
}

#[derive(Debug, Subcommand)]
enum WorkspaceCommand {
    Open { workspace: PathBuf },
    Status { workspace: PathBuf },
}

#[derive(Debug, Subcommand)]
enum StudioCommand {
    /// List known MCP sessions and the centralized platform resolution summary.
    List,
    /// Inspect native/Vinegar installations, processes, paths, and capabilities.
    Environment {
        /// Include all path states, related processes, and resolver diagnostics.
        #[arg(long)]
        verbose: bool,
    },
    /// Inspect Studio MCP or explicitly launch only its resolved stdio child.
    Mcp {
        /// Run the read-only live Studio MCP proof. Never launches Roblox Studio itself.
        #[arg(long)]
        connect: bool,
    },
    /// Pull Luau source from the real connected Studio into a native place.json workspace.
    Pull {
        /// Destination workspace containing place.json and src/.
        #[arg(long)]
        output: PathBuf,
        /// Replace existing files whose content differs from Studio.
        #[arg(long)]
        force: bool,
        /// Discover and compare only; do not write files.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ProviderCommand {
    List,
    Inspect { provider: lattice_tools::ProviderId },
}

#[derive(Debug, Subcommand)]
enum ToolCommand {
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    Inspect {
        tool: lattice_tools::ToolRef,
    },
}

#[derive(Debug, Subcommand)]
enum CapabilityCommand {
    List,
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    /// Forward MCP stdio to the already-running authoritative daemon.
    Stdio,
    /// Inspect the local daemon endpoint used by MCP clients.
    Status,
}

#[derive(Debug, Subcommand)]
enum IntegrationCommand {
    Codex {
        #[command(subcommand)]
        command: CodexCommand,
    },
}

#[derive(Debug, Subcommand)]
enum CodexCommand {
    Install,
    Status,
    Remove,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    match Arguments::parse().command {
        Command::Daemon { workspace } => run_daemon(&workspace).await?,
        Command::Workspace { command: WorkspaceCommand::Open { workspace } }
        | Command::Index { workspace } => {
            let mut lattice = Lattice::open(&workspace)?;
            print_json(&lattice.ingest()?)?;
        }
        Command::Workspace { command: WorkspaceCommand::Status { workspace } } => {
            let lattice = Lattice::open(&workspace)?;
            print_json(&lattice.status()?)?;
        }
        Command::Search { workspace, query, limit } => {
            let mut lattice = Lattice::open(&workspace)?;
            lattice.ingest()?;
            print_json(&lattice.search(&query, limit)?)?;
        }
        Command::Studio { command: StudioCommand::List } => {
            let inspection = lattice_platform::PlatformResolver::current().inspect()?;
            print_json(&serde_json::json!({
                "sessions": [],
                "connection_state": "not_started",
                "platform": platform_summary(&inspection)
            }))?;
        }
        Command::Studio { command: StudioCommand::Environment { verbose } } => {
            let inspection = lattice_platform::PlatformResolver::current().inspect()?;
            if verbose {
                print_json(&inspection)?;
            } else {
                print_json(&platform_summary(&inspection))?;
            }
        }
        Command::Studio { command: StudioCommand::Mcp { connect: false } } => {
            let inspection = lattice_platform::PlatformResolver::current().inspect()?;
            let endpoints = inspection
                .environments
                .iter()
                .map(lattice_studio::diagnose_mcp_endpoint)
                .collect::<Vec<_>>();
            print_json(&serde_json::json!({
                "launch_attempted": false,
                "studio_processes": inspection.environments.len(),
                "endpoints": endpoints,
            }))?;
        }
        Command::Studio { command: StudioCommand::Mcp { connect: true } } => {
            run_studio_mcp_proof().await?;
        }
        Command::Studio { command: StudioCommand::Pull { output, force, dry_run } } => {
            run_studio_pull(&output, force, dry_run).await?;
        }
        Command::Provider { command: ProviderCommand::List } => {
            let fabric = builtin_tool_fabric()?;
            print_json(&fabric.providers.list())?;
        }
        Command::Provider { command: ProviderCommand::Inspect { provider } } => {
            let fabric = builtin_tool_fabric()?;
            let descriptor = fabric.providers.get(provider).ok_or_else(|| {
                format!("PROVIDER_NOT_FOUND: provider {provider} is not registered")
            })?;
            print_json(descriptor)?;
        }
        Command::Tool { command: ToolCommand::Search { query, limit } } => {
            let fabric = builtin_tool_fabric()?;
            let matches = fabric
                .catalog
                .search(&query, limit)
                .into_iter()
                .map(|tool| {
                    serde_json::json!({
                        "tool_ref": tool.reference(),
                        "name": tool.native_name,
                        "title": tool.title,
                        "provider_id": tool.provider_id,
                        "capabilities": tool.capabilities,
                        "availability": tool.availability,
                        "trust": tool.trust,
                    })
                })
                .collect::<Vec<_>>();
            print_json(&matches)?;
        }
        Command::Tool { command: ToolCommand::Inspect { tool } } => {
            let fabric = builtin_tool_fabric()?;
            let descriptor =
                fabric.catalog.get(&tool).ok_or_else(|| format!("TOOL_NOT_FOUND: {tool}"))?;
            let input_schema = fabric.catalog.schema(descriptor.input_schema);
            let output_schema =
                descriptor.output_schema.and_then(|revision| fabric.catalog.schema(revision));
            print_json(&serde_json::json!({
                "tool_ref": tool,
                "descriptor": descriptor,
                "input_schema": input_schema,
                "output_schema": output_schema,
            }))?;
        }
        Command::Capability { command: CapabilityCommand::List } => {
            let fabric = builtin_tool_fabric()?;
            print_json(&fabric.capabilities.list())?;
        }
        Command::McpServe { workspace } => lattice_mcp::serve_stdio(&workspace).await?,
        Command::Mcp { command: McpCommand::Stdio } => lattice_mcp::serve_stdio_bridge().await?,
        Command::Mcp { command: McpCommand::Status } => {
            let endpoint = lattice_mcp::inspect_daemon()?;
            let reachable = if endpoint.is_some() {
                lattice_mcp::connect_daemon().await.is_ok()
            } else {
                false
            };
            print_json(&serde_json::json!({
                "daemon": if reachable { "reachable" } else { "unavailable" },
                "northbound_mcp": if reachable { "available" } else { "unavailable" },
                "endpoint": endpoint,
            }))?;
        }
        Command::Integration { command: IntegrationCommand::Codex { command } } => {
            run_codex_integration(command).await?;
        }
    }
    Ok(())
}

async fn run_codex_integration(command: CodexCommand) -> Result<(), Box<dyn std::error::Error>> {
    let executable = resolve_codex_executable()?;
    match command {
        CodexCommand::Install => {
            let lattice = resolve_lattice_executable()?;
            let runtime_dir = daemon_runtime_directory();
            let existing = codex_command(&executable, ["mcp", "get", "lattice", "--json"])?;
            if existing.status.success() {
                let configured = serde_json::from_slice::<serde_json::Value>(&existing.stdout)?;
                let actual = configured
                    .get("transport")
                    .map(|transport| {
                        serde_json::json!({
                            "command": transport.get("command").cloned().unwrap_or_default(),
                            "args": transport.get("args").cloned().unwrap_or_default(),
                            "env": transport.get("env").cloned().unwrap_or_else(|| serde_json::json!({})),
                        })
                    })
                    .unwrap_or_default();
                let expected = serde_json::json!({
                    "command": lattice.to_string_lossy(),
                    "args": ["mcp", "stdio"],
                    "env": runtime_dir.as_ref().map_or_else(
                        || serde_json::json!({}),
                        |value| serde_json::json!({ "LATTICE_RUNTIME_DIR": value }),
                    ),
                });
                let command_matches = actual.get("command") == expected.get("command")
                    && actual.get("args") == expected.get("args");
                let env_matches = actual.get("env") == expected.get("env");
                let status = if command_matches && env_matches {
                    "already_installed"
                } else if command_matches && runtime_dir.is_some() {
                    "updated"
                } else {
                    "conflict"
                };
                if status == "updated" {
                    remove_codex_lattice_server(&executable)?;
                } else {
                    print_json(&serde_json::json!({
                        "status": status,
                        "server": "lattice",
                        "transport": "stdio",
                        "command": lattice,
                        "runtime_dir": runtime_dir,
                        "detail": if status == "conflict" { "Codex already has a different server named lattice; remove it explicitly before reinstalling." } else { "Lattice is already configured." },
                    }))?;
                    if status == "conflict" {
                        return Err(
                            "CODEX_MCP_CONFLICT: Codex already has a different server named lattice"
                                .into(),
                        );
                    }
                    return Ok(());
                }
            }

            let mut arguments =
                vec![OsString::from("mcp"), OsString::from("add"), OsString::from("lattice")];
            if let Some(runtime_dir) = runtime_dir.as_ref() {
                arguments.push(OsString::from("--env"));
                arguments.push(OsString::from(format!("LATTICE_RUNTIME_DIR={runtime_dir}")));
            }
            arguments.extend([
                OsString::from("--"),
                lattice.as_os_str().to_owned(),
                OsString::from("mcp"),
                OsString::from("stdio"),
            ]);
            let result = codex_command(&executable, arguments)?;
            if !result.status.success() {
                return Err(format!(
                    "CODEX_MCP_INSTALL_FAILED: {}",
                    String::from_utf8_lossy(&result.stderr).trim()
                )
                .into());
            }
            let verification = codex_command(&executable, ["mcp", "get", "lattice", "--json"])?;
            if !verification.status.success() {
                return Err(
                    "CODEX_MCP_INSTALL_UNVERIFIED: Codex did not report lattice after installation"
                        .into(),
                );
            }
            print_json(&serde_json::json!({
                "status": "installed",
                "server": "lattice",
                "transport": "stdio",
                "command": lattice,
                "runtime_dir": runtime_dir,
            }))?;
        }
        CodexCommand::Status => {
            let version = codex_command(&executable, ["--version"])?;
            if !version.status.success() {
                return Err(format!(
                    "CODEX_VERSION_FAILED: {}",
                    String::from_utf8_lossy(&version.stderr).trim()
                )
                .into());
            }
            let version_text = String::from_utf8_lossy(&version.stdout).trim().to_owned();
            let result = codex_command(&executable, ["mcp", "list", "--json"])?;
            if !result.status.success() {
                return Err(format!(
                    "CODEX_MCP_STATUS_FAILED: {}",
                    String::from_utf8_lossy(&result.stderr).trim()
                )
                .into());
            }
            let configured: serde_json::Value = serde_json::from_slice(&result.stdout)?;
            let lattice_configured = configured.as_array().is_some_and(|servers| {
                servers.iter().any(|server| {
                    server.get("name").and_then(serde_json::Value::as_str) == Some("lattice")
                })
            });
            let daemon = lattice_mcp::inspect_daemon()?;
            let reachable =
                if daemon.is_some() { lattice_mcp::connect_daemon().await.is_ok() } else { false };
            print_json(&serde_json::json!({
                "codex": "available",
                "codex_version": version_text,
                "configured": lattice_configured,
                "server": "lattice",
                "transport": "stdio",
                "daemon_endpoint": daemon,
                "daemon": if reachable { "reachable" } else { "unavailable" },
            }))?;
        }
        CodexCommand::Remove => {
            let result = codex_command(&executable, ["mcp", "remove", "lattice"])?;
            if !result.status.success() {
                let error = String::from_utf8_lossy(&result.stderr);
                if error.contains("not found") || error.contains("does not exist") {
                    print_json(
                        &serde_json::json!({ "status": "already_removed", "server": "lattice" }),
                    )?;
                    return Ok(());
                }
                return Err(format!("CODEX_MCP_REMOVE_FAILED: {}", error.trim()).into());
            }
            print_json(&serde_json::json!({ "status": "removed", "server": "lattice" }))?;
        }
    }
    Ok(())
}

fn resolve_codex_executable() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(configured) = std::env::var_os("CODEX_PATH").map(PathBuf::from)
        && configured.is_file()
    {
        return Ok(configured);
    }
    let path = std::env::var_os("PATH").ok_or("CODEX_NOT_FOUND: PATH is unavailable")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err("CODEX_NOT_FOUND: codex is not available on PATH".into())
}

fn resolve_lattice_executable() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = std::env::var_os("LATTICE_CLI_PATH")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok());
    path.filter(|candidate| candidate.is_file())
        .ok_or_else(|| "LATTICE_CLI_NOT_FOUND: unable to resolve the lattice executable".into())
}

fn daemon_runtime_directory() -> Option<String> {
    std::env::var_os("LATTICE_RUNTIME_DIR")
        .or_else(|| std::env::var_os("XDG_RUNTIME_DIR"))
        .map(|value| value.to_string_lossy().into_owned())
        .or_else(|| {
            lattice_mcp::inspect_daemon().ok().flatten().and_then(|endpoint| {
                if endpoint.transport != "unix" {
                    return None;
                }
                Path::new(&endpoint.address)
                    .parent()
                    .and_then(Path::parent)
                    .map(|path| path.to_string_lossy().into_owned())
            })
        })
}

fn remove_codex_lattice_server(executable: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let result = codex_command(executable, ["mcp", "remove", "lattice"])?;
    if result.status.success() {
        return Ok(());
    }
    Err(format!("CODEX_MCP_UPDATE_FAILED: {}", String::from_utf8_lossy(&result.stderr).trim())
        .into())
}

fn codex_command<I, S>(
    executable: &Path,
    arguments: I,
) -> Result<std::process::Output, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Ok(std::process::Command::new(executable)
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()?)
}

fn builtin_tool_fabric() -> Result<lattice_tools::BuiltinToolFabric, Box<dyn std::error::Error>> {
    let inspection = lattice_platform::PlatformResolver::current().inspect()?;
    Ok(lattice_tools::BuiltinToolFabric::from_environments(&inspection.environments)?)
}

async fn run_studio_mcp_proof() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;

    use lattice_mcp::{
        ResolvedStudioMcpProcessLauncher, StudioMcpProcessLauncher, StudioMcpSessionBinding,
    };
    use lattice_resource::LatticeId;

    let inspection = lattice_platform::PlatformResolver::current().inspect()?;
    let selected = inspection
        .selected_environment
        .ok_or("STUDIO_ENVIRONMENT_AMBIGUOUS: select exactly one running Studio environment")?;
    let environment = inspection
        .environments
        .iter()
        .find(|environment| environment.id == selected)
        .ok_or("STUDIO_ENVIRONMENT_NOT_FOUND: selected environment disappeared")?;
    let process = environment
        .studio_process
        .as_ref()
        .ok_or("STUDIO_NOT_CONNECTED: no running Studio process is correlated")?;
    let studio_session_id = LatticeId::new();
    let binding = StudioMcpSessionBinding {
        studio_session_id,
        environment_id: environment.id,
        process_id: process.pid,
    };
    let mut launched = ResolvedStudioMcpProcessLauncher
        .launch(environment, binding, Duration::from_secs(15))
        .await?;
    let (studios, studio_external_id) = wait_for_sole_studio(&mut launched).await?;
    let state = launched
        .client_mut()
        .call_tool("get_studio_state", serde_json::json!({"studio_id": studio_external_id}))
        .await?;
    let connection = launched.snapshot().clone();
    let stderr = launched.stderr().await;
    launched.disconnect(Duration::from_secs(5)).await?;
    print_json(&serde_json::json!({
        "studio_launched": false,
        "studio_mcp_child_launched": true,
        "environment_id": environment.id,
        "studio_pid": process.pid,
        "studio_session": {
            "lattice_session_id": studio_session_id,
            "studio_external_id": studio_external_id,
            "environment_id": environment.id,
            "process_id": process.pid,
        },
        "connection": {
            "id": connection.id,
            "protocol_version": connection.protocol_version,
            "protocol_negotiation": connection.protocol_negotiation,
            "protocol_session_model": connection.protocol_session_model,
            "protocol_fallback_reason": connection.protocol_fallback_reason,
            "server_name": connection.server_name,
            "server_version": connection.server_version,
            "tool_catalog_revision": connection.tool_catalog_revision,
            "tool_count": connection.tools.len(),
            "tools": connection.tools.iter().map(|tool| tool.name.clone()).collect::<Vec<_>>(),
        },
        "list_roblox_studios": studios,
        "get_studio_state": state,
        "bounded_stderr": stderr,
        "disconnected": true,
    }))?;
    Ok(())
}

/// Pulls the source-bearing portion of the live `DataModel` through the real
/// Studio MCP transport. This is deliberately a source pull first: the
/// official Studio MCP exposes hierarchy and script reads, not a portable
/// place serializer. A future `.rbxjson` mode can build on the same verified
/// transport without changing the launcher or protocol boundary.
async fn run_studio_pull(
    output: &Path,
    force: bool,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;

    use lattice_mcp::{
        ResolvedStudioMcpProcessLauncher, StudioMcpProcessLauncher, StudioMcpSessionBinding,
    };
    use lattice_resource::LatticeId;

    const SERVICES: &[&str] = &[
        "Workspace",
        "Lighting",
        "ReplicatedFirst",
        "ReplicatedStorage",
        "ServerScriptService",
        "ServerStorage",
        "StarterGui",
        "StarterPack",
        "StarterPlayer",
        "SoundService",
        "Teams",
        "Chat",
        "TextChatService",
    ];

    let inspection = lattice_platform::PlatformResolver::current().inspect()?;
    let selected = inspection
        .selected_environment
        .ok_or("STUDIO_ENVIRONMENT_AMBIGUOUS: select exactly one running Studio environment")?;
    let environment = inspection
        .environments
        .iter()
        .find(|environment| environment.id == selected)
        .ok_or("STUDIO_ENVIRONMENT_NOT_FOUND: selected environment disappeared")?;
    let process = environment
        .studio_process
        .as_ref()
        .ok_or("STUDIO_NOT_CONNECTED: no running Studio process is correlated")?;
    let binding = StudioMcpSessionBinding {
        studio_session_id: LatticeId::new(),
        environment_id: environment.id,
        process_id: process.pid,
    };
    let mut launched = ResolvedStudioMcpProcessLauncher
        .launch(environment, binding, Duration::from_secs(20))
        .await?;
    let (studio_listing, studio_external_id) = wait_for_sole_studio(&mut launched).await?;
    let place_name = studio_name(&studio_listing.value, &studio_external_id).unwrap_or_else(|| {
        output
            .file_name()
            .map_or_else(|| "RobloxPlace".into(), |name| name.to_string_lossy().into_owned())
    });

    let mut nodes = Vec::new();
    for service in SERVICES {
        let result = launched
            .client_mut()
            .call_tool(
                "search_game_tree",
                serde_json::json!({
                    "path": service,
                    "instance_type": "LuaSourceContainer",
                    "max_depth": 10,
                    "head_limit": 5000,
                    "datamodel_type": "Edit",
                    "studio_id": studio_external_id,
                }),
            )
            .await?;
        nodes.extend(parse_script_nodes(&result.value)?);
    }
    nodes.sort_by(|left, right| left.full_path.cmp(&right.full_path));
    nodes.dedup_by(|left, right| left.full_path == right.full_path);

    let paths = nodes.iter().map(|node| node.full_path.clone()).collect::<BTreeSet<_>>();
    let mut files = Vec::with_capacity(nodes.len());
    for node in nodes {
        let local_path = local_script_path(output, &node, &paths)?;
        let source_result = launched
            .client_mut()
            .call_tool(
                "script_read",
                serde_json::json!({
                    "target_file": node.full_path,
                    "should_read_entire_file": true,
                    "studio_id": studio_external_id,
                }),
            )
            .await?;
        let source = extract_script_source(&source_result.value)?;
        files.push(PullFile { studio_path: node.full_path, local_path, source });
    }
    let disconnect_result = launched.disconnect(Duration::from_secs(5)).await;
    disconnect_result?;

    let mut conflicts = Vec::new();
    let mut new_files = 0_u64;
    let mut changed_files = 0_u64;
    for file in &files {
        match fs::read_to_string(&file.local_path) {
            Ok(existing) if existing == file.source => {}
            Ok(_) => {
                changed_files = changed_files.saturating_add(1);
                if !force {
                    conflicts.push(file.local_path.display().to_string());
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                new_files = new_files.saturating_add(1);
            }
            Err(error) => {
                return Err(format!(
                    "PULL_READ_DESTINATION_FAILED: {}: {error}",
                    file.local_path.display()
                )
                .into());
            }
        }
    }
    let place_path = output.join("place.json");
    if place_path.is_file() && !force {
        conflicts.push(place_path.display().to_string());
    }
    if !conflicts.is_empty() && !dry_run {
        print_json(&serde_json::json!({
            "status": "conflict",
            "reason": "existing files would be overwritten",
            "output": output,
            "place_name": place_name,
            "scripts": files.len(),
            "new_files": new_files,
            "changed_files": changed_files,
            "conflicts": conflicts,
            "next": "rerun with --force only after reviewing the conflict list",
        }))?;
        return Ok(());
    }

    let mut tree = serde_json::json!({"$className": "DataModel"});
    let mut services = BTreeSet::new();
    for file in &files {
        if let Some(service) = file.studio_path.split('.').next() {
            services.insert(service.to_owned());
        }
    }
    for service in services {
        tree[service] = serde_json::json!({"$path": format!("src/{service}")});
    }
    let project = serde_json::json!({
        "name": place_name,
        "format": "abraxius-v1",
        "tree": tree,
    });
    if !dry_run {
        fs::create_dir_all(output)?;
        for file in &files {
            if let Some(parent) = file.local_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&file.local_path, &file.source)?;
        }
        fs::write(&place_path, serde_json::to_vec_pretty(&project)?)?;
    }
    print_json(&serde_json::json!({
        "status": if dry_run { "dry_run" } else { "pulled" },
        "output": output,
        "place_name": place_name,
        "scripts": files.len(),
        "new_files": new_files,
        "changed_files": changed_files,
        "conflicts": conflicts,
        "place_json": place_path,
    }))?;
    Ok(())
}

#[derive(Clone, Debug)]
struct PullNode {
    full_path: String,
    class_name: String,
}

struct PullFile {
    studio_path: String,
    local_path: PathBuf,
    source: String,
}

fn parse_script_nodes(
    value: &serde_json::Value,
) -> Result<Vec<PullNode>, Box<dyn std::error::Error>> {
    let text = value
        .pointer("/content/0/text")
        .and_then(serde_json::Value::as_str)
        .ok_or("STUDIO_MCP_PROTOCOL_ERROR: search_game_tree returned no text payload")?;
    if text.trim_start().starts_with("No instances found") {
        return Ok(Vec::new());
    }
    let payload = decode_json_text(text)?;
    let array = payload
        .as_array()
        .ok_or("STUDIO_MCP_PROTOCOL_ERROR: search_game_tree did not return an array")?;
    Ok(array
        .iter()
        .filter_map(|node| {
            let full_path = node
                .get("fullPath")
                .or_else(|| node.get("path"))
                .and_then(serde_json::Value::as_str)?;
            let class_name = node
                .get("className")
                .or_else(|| node.get("class"))
                .and_then(serde_json::Value::as_str)?;
            if !matches!(class_name, "Script" | "LocalScript" | "ModuleScript") {
                return None;
            }
            Some(PullNode { full_path: full_path.to_owned(), class_name: class_name.to_owned() })
        })
        .collect::<Vec<_>>())
}

fn decode_json_text(text: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    if let Ok(value) = serde_json::from_str(text.trim()) {
        return Ok(value);
    }
    let start = text
        .char_indices()
        .find_map(|(index, character)| (character == '[' || character == '{').then_some(index))
        .ok_or_else(|| {
            format!("STUDIO_MCP_PROTOCOL_ERROR: no JSON payload in Studio text result: {text}")
        })?;
    Ok(serde_json::from_str(&text[start..])?)
}

fn local_script_path(
    output: &Path,
    node: &PullNode,
    all_paths: &BTreeSet<String>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let parts = node
        .full_path
        .strip_prefix("game.")
        .unwrap_or(&node.full_path)
        .split('.')
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(format!("PULL_INVALID_SCRIPT_PATH: {}", node.full_path).into());
    }
    let service = safe_component(parts[0])?;
    let mut path = output.join("src").join(service);
    for part in &parts[1..parts.len() - 1] {
        path.push(safe_component(part)?);
    }
    let name = safe_component(parts[parts.len() - 1])?;
    let extension = match node.class_name.as_str() {
        "Script" => "server.luau",
        "LocalScript" => "client.luau",
        "ModuleScript" => "luau",
        _ => return Err(format!("PULL_UNSUPPORTED_SCRIPT_CLASS: {}", node.class_name).into()),
    };
    let has_children = all_paths.iter().any(|candidate| {
        candidate.strip_prefix("game.").unwrap_or(candidate).starts_with(&format!(
            "{}.",
            node.full_path.strip_prefix("game.").unwrap_or(&node.full_path)
        ))
    });
    if has_children {
        Ok(path.join(name).join(format!("init.{extension}")))
    } else {
        Ok(path.join(format!("{name}.{extension}")))
    }
}

fn safe_component(value: &str) -> Result<String, Box<dyn std::error::Error>> {
    if value.is_empty() || value == "." || value == ".." {
        return Err("PULL_INVALID_PATH_COMPONENT".into());
    }
    Ok(value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '_' | '-' | ' ') {
                character
            } else {
                '_'
            }
        })
        .collect())
}

fn extract_script_source(value: &serde_json::Value) -> Result<String, Box<dyn std::error::Error>> {
    let text = value
        .pointer("/content/0/text")
        .and_then(serde_json::Value::as_str)
        .ok_or("STUDIO_MCP_PROTOCOL_ERROR: script_read returned no text payload")?;
    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(text.trim())
        && let Some(source) = payload.get("source").and_then(serde_json::Value::as_str)
    {
        return Ok(source.to_owned());
    }
    Ok(text
        .lines()
        .map(|line| line.find('→').map_or(line, |index| &line[index + '→'.len_utf8()..]))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn studio_name(value: &serde_json::Value, id: &str) -> Option<String> {
    let text = value.pointer("/content/0/text").and_then(serde_json::Value::as_str)?;
    let payload = decode_json_text(text).ok()?;
    payload
        .get("studios")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .find(|studio| studio.get("id").and_then(serde_json::Value::as_str) == Some(id))
        .and_then(|studio| studio.get("name").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
}

async fn wait_for_sole_studio(
    launched: &mut lattice_mcp::LaunchedStudioMcpClient,
) -> Result<(lattice_studio::StudioMcpToolResult, String), Box<dyn std::error::Error>> {
    for _ in 0..20 {
        let result =
            launched.client_mut().call_tool("list_roblox_studios", serde_json::json!({})).await?;
        let ids = studio_ids(&result.value)?;
        match ids.as_slice() {
            [only] => return Ok((result, only.clone())),
            [] => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
            _ => {
                return Err(format!(
                    "AMBIGUOUS_STUDIO_SESSION: automatic proof observed {} Studio instances",
                    ids.len()
                )
                .into());
            }
        }
    }
    Err("STUDIO_MCP_UNAVAILABLE: no running Studio registered with StudioMCP within 10 seconds"
        .into())
}

fn studio_ids(value: &serde_json::Value) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let text = value
        .pointer("/content/0/text")
        .and_then(serde_json::Value::as_str)
        .ok_or("STUDIO_MCP_PROTOCOL_ERROR: list_roblox_studios returned no text payload")?;
    let decoded: serde_json::Value = serde_json::from_str(text)?;
    let studios = decoded
        .get("studios")
        .and_then(serde_json::Value::as_array)
        .ok_or("STUDIO_MCP_PROTOCOL_ERROR: list_roblox_studios returned no studios array")?;
    studios
        .iter()
        .map(|studio| {
            studio
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| "STUDIO_MCP_PROTOCOL_ERROR: Studio result has no id".into())
        })
        .collect()
}

async fn run_daemon(workspace: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut lattice = Lattice::open(workspace)?;
    lattice.ingest()?;
    print_json(&lattice.status()?)?;
    tokio::signal::ctrl_c().await?;
    Ok(())
}

fn print_json(value: &impl serde::Serialize) -> Result<(), Box<dyn std::error::Error>> {
    let output = serde_json::to_string_pretty(value)?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{output}")?;
    Ok(())
}

fn platform_summary(inspection: &lattice_platform::PlatformInspection) -> serde_json::Value {
    let environments = inspection
        .environments
        .iter()
        .map(|environment| {
            serde_json::json!({
                "id": environment.id,
                "runtime": environment.runtime,
                "process": environment.studio_process,
                "deployment": environment.studio_deployment,
                "config": environment.config_root,
                "data": environment.data_root,
                "cache": environment.cache_root,
                "prefix": environment.studio_prefix,
                "roblox_appdata": environment.roblox_appdata,
                "logs": environment.logs_root,
                "profiler": environment.profiler_root,
                "wine_runtime": environment.wine_runtime,
                "wine_drive_mappings": environment.wine_drive_mappings,
                "related_process_count": environment.related_processes.len(),
                "capabilities": environment.capabilities,
                "mcp_launcher": environment.mcp_launcher,
                "mcp_server": environment.path(lattice_platform::StudioPathRole::McpServer),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "host_platform": inspection.host_platform,
        "status": inspection.status,
        "selected_environment": inspection.selected_environment,
        "environments": environments,
        "diagnostics": inspection.diagnostics,
    })
}
