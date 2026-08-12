use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use lattice_connections::{
    ConnectionBroker, MemoryResultStore, PolicyDecision, PolicyEnforcer, PolicyRequest,
    ToolProvider,
};
use lattice_mcp::{
    ExternalMcpProvider, ExternalMcpProviderConfig, McpNegotiationPath, McpProtocolProfile,
    McpSessionModel,
};
use lattice_tools::ProviderTrust;

struct FixturePolicy;

impl PolicyEnforcer for FixturePolicy {
    fn authorize(&self, _request: &PolicyRequest) -> PolicyDecision {
        PolicyDecision { allowed: true, reason: "controlled fixture".into() }
    }
}

fn provider(profile: &str, audit_path: &Path) -> Result<Arc<ExternalMcpProvider>, Box<dyn Error>> {
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_lattice-native-provider-fixture"));
    let environment = BTreeMap::from([(
        "LATTICE_FIXTURE_AUDIT_PATH".into(),
        audit_path.to_string_lossy().into_owned(),
    )]);
    Ok(Arc::new(ExternalMcpProvider::new(ExternalMcpProviderConfig {
        stable_key: format!("test:native-stdio-fixture:{profile}"),
        name: format!("Native Rust {profile} Fixture"),
        executable,
        arguments: vec![profile.into()],
        environment,
        working_directory: None,
        trust: ProviderTrust::Verified,
        shutdown_timeout: Duration::from_secs(2),
    })?))
}

fn broker() -> ConnectionBroker {
    ConnectionBroker::new(4, Arc::new(FixturePolicy), Arc::new(MemoryResultStore::default()))
}

#[tokio::test]
async fn modern_wire_uses_discover_per_request_metadata_and_no_session_handshake()
-> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let audit_path = temporary.path().join("modern.audit");
    let provider = provider("modern", &audit_path)?;
    let provider_id = ToolProvider::descriptor(provider.as_ref()).id;
    let broker = broker();
    broker.register(provider.clone(), 2).await?;
    broker.connect(provider_id).await?;

    let negotiation = provider.protocol_status().await.ok_or("missing protocol status")?;
    assert_eq!(negotiation.profile, McpProtocolProfile::Modern2026_07_28);
    assert_eq!(negotiation.path, McpNegotiationPath::ServerDiscover);
    assert_eq!(negotiation.features.session_model, McpSessionModel::StatelessPerRequest);
    let inspected = broker
        .providers()
        .await
        .into_iter()
        .find(|descriptor| descriptor.id == provider_id)
        .ok_or("provider missing from broker inspection")?;
    let protocol = inspected.metadata.protocol.ok_or("protocol metadata missing")?;
    assert_eq!(protocol.revision, "2026-07-28");
    assert_eq!(protocol.negotiation, "server/discover");
    assert_eq!(protocol.session_model, "stateless per request");

    let tools = broker.search_tools("fixture echo", 10).await;
    assert_eq!(tools.len(), 1);
    let result = broker
        .call(
            "integration-test",
            tools[0].reference(),
            serde_json::json!({"value":"hello modern fabric"}),
            Some(Duration::from_secs(2)),
            None,
        )
        .await?;
    assert!(result.inline.ok_or("missing inline result")?.to_string().contains("hello modern"));
    broker.disconnect(provider_id).await?;

    let audit = fs::read_to_string(audit_path)?;
    assert_eq!(audit.lines().next(), Some("server/discover"));
    assert!(!audit.lines().any(|line| line == "initialize"));
    assert!(!audit.lines().any(|line| line == "notifications/initialized"));
    assert!(audit.lines().any(|line| line == "tools/list"));
    assert!(audit.lines().any(|line| line == "tools/call"));
    Ok(())
}

#[tokio::test]
async fn legacy_wire_falls_back_only_after_method_not_found() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let audit_path = temporary.path().join("legacy.audit");
    let provider = provider("legacy", &audit_path)?;
    let provider_id = ToolProvider::descriptor(provider.as_ref()).id;
    let broker = broker();
    broker.register(provider.clone(), 2).await?;
    broker.connect(provider_id).await?;

    let negotiation = provider.protocol_status().await.ok_or("missing protocol status")?;
    assert!(matches!(negotiation.profile, McpProtocolProfile::Legacy2025 { .. }));
    assert_eq!(negotiation.path, McpNegotiationPath::InitializeFallback);
    assert!(negotiation.fallback_reason.is_some());
    broker.disconnect(provider_id).await?;

    let audit = fs::read_to_string(audit_path)?;
    let methods = audit.lines().collect::<Vec<_>>();
    assert_eq!(methods.first().copied(), Some("server/discover"));
    assert!(methods.contains(&"initialize"));
    assert!(methods.contains(&"notifications/initialized"));
    assert!(methods.contains(&"tools/list"));
    Ok(())
}

#[tokio::test]
async fn malformed_discovery_never_downgrades_to_initialize() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let audit_path = temporary.path().join("malformed.audit");
    let provider = provider("malformed-discovery", &audit_path)?;
    let provider_id = ToolProvider::descriptor(provider.as_ref()).id;
    let broker = broker();
    broker.register(provider, 2).await?;
    assert!(broker.connect(provider_id).await.is_err());

    let audit = fs::read_to_string(audit_path)?;
    assert_eq!(audit.lines().collect::<Vec<_>>(), vec!["server/discover"]);
    Ok(())
}
