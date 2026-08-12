//! Protocol-neutral provider, capability, tool, schema, and routing domain.
//!
//! This crate describes and resolves explicitly requested operations. It does
//! not plan work, select engineering strategy, or infer capability mappings.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
};

use lattice_platform::{StudioEnvironment, StudioEnvironmentId};
use lattice_resource::{ContentHash, LatticeId};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

const TOOL_REF_PREFIX: &str = "lattice://provider/";
const MAX_SCHEMA_BYTES: usize = 1024 * 1024;
const MAX_SCHEMA_DEPTH: usize = 64;

macro_rules! stable_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; 16]);

        impl $name {
            #[must_use]
            pub fn from_stable_key(key: &[u8]) -> Self {
                let hash = blake3::hash(key);
                let mut bytes = [0_u8; 16];
                bytes.copy_from_slice(&hash.as_bytes()[..16]);
                Self(bytes)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, concat!($prefix, "{}"), encode_hex(&self.0))
            }
        }

        impl FromStr for $name {
            type Err = ToolFabricError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                decode_id(value, $prefix).map(Self)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                String::deserialize(deserializer)?.parse().map_err(D::Error::custom)
            }
        }
    };
}

stable_id!(ProviderId, "provider_");
stable_id!(ToolId, "tool_");
stable_id!(ProviderResourceId, "provider_resource_");

impl ToolId {
    #[must_use]
    pub fn for_native_name(provider: ProviderId, native_name: &str) -> Self {
        tool_id(provider, native_name)
    }
}

impl ProviderResourceId {
    #[must_use]
    pub fn for_original_uri(provider: ProviderId, original_uri: &str) -> Self {
        let key = format!("{provider}\0{original_uri}");
        Self::from_stable_key(key.as_bytes())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProviderResourceRef {
    pub provider_id: ProviderId,
    pub resource_id: ProviderResourceId,
}

impl fmt::Display for ProviderResourceRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "lattice://provider/{}/resource/{}", self.provider_id, self.resource_id)
    }
}

impl FromStr for ProviderResourceRef {
    type Err = ToolFabricError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let body =
            value.strip_prefix(TOOL_REF_PREFIX).ok_or(ToolFabricError::InvalidResourceRef)?;
        let mut segments = body.split('/');
        let provider_id = segments.next().ok_or(ToolFabricError::InvalidResourceRef)?.parse()?;
        if segments.next() != Some("resource") {
            return Err(ToolFabricError::InvalidResourceRef);
        }
        let resource_id = segments.next().ok_or(ToolFabricError::InvalidResourceRef)?.parse()?;
        if segments.next().is_some() {
            return Err(ToolFabricError::InvalidResourceRef);
        }
        Ok(Self { provider_id, resource_id })
    }
}

impl Serialize for ProviderResourceRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ProviderResourceRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?.parse().map_err(D::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderResourceDescriptor {
    pub id: ProviderResourceId,
    pub provider_id: ProviderId,
    /// Original provider URI is metadata and is never concatenated into a URI.
    pub original_uri: String,
    pub name: String,
    pub mime_type: Option<String>,
    pub availability: ToolAvailability,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityId(String);

impl CapabilityId {
    pub fn parse(value: impl Into<String>) -> Result<Self, ToolFabricError> {
        let value = value.into();
        validate_capability_id(&value)?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for CapabilityId {
    type Err = ToolFabricError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ToolRef {
    pub provider_id: ProviderId,
    pub tool_id: ToolId,
}

impl fmt::Display for ToolRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{TOOL_REF_PREFIX}{}/tool/{}", self.provider_id, self.tool_id)
    }
}

impl FromStr for ToolRef {
    type Err = ToolFabricError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let body = value.strip_prefix(TOOL_REF_PREFIX).ok_or(ToolFabricError::InvalidToolRef)?;
        let mut segments = body.split('/');
        let provider_id = segments.next().ok_or(ToolFabricError::InvalidToolRef)?.parse()?;
        if segments.next() != Some("tool") {
            return Err(ToolFabricError::InvalidToolRef);
        }
        let tool_id = segments.next().ok_or(ToolFabricError::InvalidToolRef)?.parse()?;
        if segments.next().is_some() {
            return Err(ToolFabricError::InvalidToolRef);
        }
        Ok(Self { provider_id, tool_id })
    }
}

impl Serialize for ToolRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ToolRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?.parse().map_err(D::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    BuiltIn,
    McpStdio,
    McpHttp,
    NativeAdapter,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderTrust {
    BuiltIn,
    Verified,
    UserConfigured,
    Untrusted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderTransportKind {
    InProcess,
    Stdio,
    StreamableHttp,
    NativeAdapter,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderConnectionState {
    Configured,
    Unavailable,
    Discovering,
    Connecting,
    Initializing,
    Connected,
    Degraded,
    Reconnecting,
    Disconnected,
    Failed,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHealthStatus {
    Healthy,
    Degraded,
    Unavailable,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationState {
    NotRequired,
    Required,
    Authenticated,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub status: ProviderHealthStatus,
    pub reason: String,
    pub last_successful_operation_unix_ms: Option<i64>,
    pub last_failure: Option<String>,
    pub rtt_micros: Option<u64>,
    pub consecutive_failures: u64,
    pub connected_at_unix_ms: Option<i64>,
    pub catalog_revision: Option<ContentHash>,
    pub tool_count: u64,
    pub resource_count: u64,
    pub authentication: AuthenticationState,
    pub connection_state: ProviderConnectionState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderMetadata {
    pub source: String,
    pub studio_environment_id: Option<StudioEnvironmentId>,
    pub studio_session_id: Option<LatticeId>,
    pub configured_priority: Option<i32>,
    pub protocol: Option<ProviderProtocolMetadata>,
}

/// Protocol details are descriptive provider state, not an authorization input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderProtocolMetadata {
    pub family: String,
    pub revision: String,
    pub negotiation: String,
    pub session_model: String,
    pub cancellation_model: String,
    pub catalog_change_model: String,
    pub fallback_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub kind: ProviderKind,
    pub name: String,
    pub version: Option<String>,
    pub trust: ProviderTrust,
    pub transport: ProviderTransportKind,
    pub health: ProviderHealth,
    pub metadata: ProviderMetadata,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticTruth {
    KnownTrue,
    KnownFalse,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperationSemantics {
    pub read_only: SemanticTruth,
    pub mutating: SemanticTruth,
    pub destructive: SemanticTruth,
    pub idempotent: SemanticTruth,
    pub open_world: SemanticTruth,
    pub network_access: SemanticTruth,
    pub filesystem_access: SemanticTruth,
    pub code_execution: SemanticTruth,
    pub credential_use: SemanticTruth,
    pub transaction_support: SemanticTruth,
}

impl OperationSemantics {
    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            read_only: SemanticTruth::Unknown,
            mutating: SemanticTruth::Unknown,
            destructive: SemanticTruth::Unknown,
            idempotent: SemanticTruth::Unknown,
            open_world: SemanticTruth::Unknown,
            network_access: SemanticTruth::Unknown,
            filesystem_access: SemanticTruth::Unknown,
            code_execution: SemanticTruth::Unknown,
            credential_use: SemanticTruth::Unknown,
            transaction_support: SemanticTruth::Unknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReportedSemantics {
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
    pub idempotent_hint: Option<bool>,
    pub open_world_hint: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaRevision(ContentHash);

impl fmt::Display for SchemaRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaValidationState {
    Valid,
    Quarantined,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SchemaRecord {
    pub revision: SchemaRevision,
    pub provider_schema: serde_json::Value,
    pub normalized_schema: serde_json::Value,
    pub validation: SchemaValidationState,
    pub byte_len: u64,
}

#[derive(Default)]
pub struct SchemaRegistry {
    schemas: BTreeMap<SchemaRevision, SchemaRecord>,
}

impl SchemaRegistry {
    pub fn register(
        &mut self,
        provider_schema: serde_json::Value,
    ) -> Result<SchemaRevision, ToolFabricError> {
        validate_schema(&provider_schema)?;
        let normalized = normalize_json(provider_schema.clone());
        let encoded = serde_json::to_vec(&normalized).map_err(ToolFabricError::Json)?;
        if encoded.len() > MAX_SCHEMA_BYTES {
            return Err(ToolFabricError::SchemaTooLarge(encoded.len()));
        }
        let revision = SchemaRevision(ContentHash::of(&encoded));
        self.schemas.entry(revision).or_insert(SchemaRecord {
            revision,
            provider_schema,
            normalized_schema: normalized,
            validation: SchemaValidationState::Valid,
            byte_len: u64::try_from(encoded.len()).unwrap_or(u64::MAX),
        });
        Ok(revision)
    }

    #[must_use]
    pub fn get(&self, revision: SchemaRevision) -> Option<&SchemaRecord> {
        self.schemas.get(&revision)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityBinding {
    pub capability: CapabilityId,
    pub target: CapabilityTarget,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapabilityTarget {
    Any,
    StudioSession { session_id: LatticeId },
    StudioEnvironment { environment_id: StudioEnvironmentId },
    CloudPlace { universe_id: u64, place_id: u64 },
    Workspace { workspace_id: LatticeId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTrust {
    BuiltIn,
    Verified,
    UserConfigured,
    Untrusted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAvailability {
    Available,
    Stale,
    Unavailable,
    Quarantined,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub id: ToolId,
    pub provider_id: ProviderId,
    pub native_name: String,
    pub title: Option<String>,
    /// Untrusted provider-supplied data, never policy authority.
    pub provider_description: Option<String>,
    pub input_schema: SchemaRevision,
    pub output_schema: Option<SchemaRevision>,
    pub capabilities: Vec<CapabilityBinding>,
    pub verified_semantics: OperationSemantics,
    pub reported_semantics: ReportedSemantics,
    pub trust: ToolTrust,
    pub availability: ToolAvailability,
}

impl ToolDescriptor {
    #[must_use]
    pub fn reference(&self) -> ToolRef {
        ToolRef { provider_id: self.provider_id, tool_id: self.id }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImportedTool {
    pub native_name: String,
    pub title: Option<String>,
    pub provider_description: Option<String>,
    pub input_schema: serde_json::Value,
    pub output_schema: Option<serde_json::Value>,
    pub capabilities: Vec<CapabilityBinding>,
    pub verified_semantics: OperationSemantics,
    pub reported_semantics: ReportedSemantics,
    pub trust: ToolTrust,
}

#[derive(Default)]
pub struct ProviderRegistry {
    providers: BTreeMap<ProviderId, ProviderDescriptor>,
}

impl ProviderRegistry {
    pub fn register(&mut self, descriptor: ProviderDescriptor) -> Result<(), ToolFabricError> {
        if self.providers.contains_key(&descriptor.id) {
            return Err(ToolFabricError::ProviderAlreadyRegistered(descriptor.id));
        }
        self.providers.insert(descriptor.id, descriptor);
        Ok(())
    }

    #[must_use]
    pub fn list(&self) -> Vec<&ProviderDescriptor> {
        self.providers.values().collect()
    }

    #[must_use]
    pub fn get(&self, id: ProviderId) -> Option<&ProviderDescriptor> {
        self.providers.get(&id)
    }

    pub fn update_health(
        &mut self,
        id: ProviderId,
        health: ProviderHealth,
    ) -> Result<(), ToolFabricError> {
        let provider = self.providers.get_mut(&id).ok_or(ToolFabricError::ProviderNotFound(id))?;
        provider.health = health;
        Ok(())
    }

    pub fn update_descriptor(
        &mut self,
        descriptor: ProviderDescriptor,
    ) -> Result<(), ToolFabricError> {
        let current = self
            .providers
            .get_mut(&descriptor.id)
            .ok_or(ToolFabricError::ProviderNotFound(descriptor.id))?;
        let health = current.health.clone();
        *current = descriptor;
        current.health = health;
        Ok(())
    }
}

#[derive(Default)]
pub struct ToolCatalog {
    tools: BTreeMap<ToolId, ToolDescriptor>,
    provider_tools: BTreeMap<ProviderId, BTreeSet<ToolId>>,
    schema_registry: SchemaRegistry,
    catalog_revisions: BTreeMap<ProviderId, ContentHash>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogRefresh {
    pub provider_id: ProviderId,
    pub catalog_revision: ContentHash,
    pub added: Vec<ToolId>,
    pub schema_changed: Vec<ToolId>,
    pub removed: Vec<ToolId>,
    pub unchanged: Vec<ToolId>,
}

impl ToolCatalog {
    pub fn refresh_provider(
        &mut self,
        provider_id: ProviderId,
        imported: Vec<ImportedTool>,
    ) -> Result<CatalogRefresh, ToolFabricError> {
        let mut names = BTreeSet::new();
        let mut next = BTreeMap::new();
        for tool in imported {
            if tool.native_name.trim().is_empty() {
                return Err(ToolFabricError::InvalidToolName);
            }
            if !names.insert(tool.native_name.clone()) {
                return Err(ToolFabricError::DuplicateToolName(tool.native_name));
            }
            let id = tool_id(provider_id, &tool.native_name);
            let input_schema = self.schema_registry.register(tool.input_schema)?;
            let output_schema = tool
                .output_schema
                .map(|schema| self.schema_registry.register(schema))
                .transpose()?;
            next.insert(
                id,
                ToolDescriptor {
                    id,
                    provider_id,
                    native_name: tool.native_name,
                    title: tool.title,
                    provider_description: tool.provider_description,
                    input_schema,
                    output_schema,
                    capabilities: tool.capabilities,
                    verified_semantics: tool.verified_semantics,
                    reported_semantics: tool.reported_semantics,
                    trust: tool.trust,
                    availability: ToolAvailability::Available,
                },
            );
        }

        let previous = self.provider_tools.get(&provider_id).cloned().unwrap_or_default();
        let current = next.keys().copied().collect::<BTreeSet<_>>();
        let mut refresh = CatalogRefresh {
            provider_id,
            catalog_revision: catalog_revision(next.values())?,
            added: Vec::new(),
            schema_changed: Vec::new(),
            removed: Vec::new(),
            unchanged: Vec::new(),
        };
        for id in &current {
            match self.tools.get(id) {
                None => refresh.added.push(*id),
                Some(old)
                    if old.input_schema != next[id].input_schema
                        || old.output_schema != next[id].output_schema =>
                {
                    refresh.schema_changed.push(*id);
                }
                Some(_) => refresh.unchanged.push(*id),
            }
        }
        for removed in previous.difference(&current) {
            refresh.removed.push(*removed);
            if let Some(tool) = self.tools.get_mut(removed) {
                tool.availability = ToolAvailability::Unavailable;
            }
        }
        self.tools.extend(next);
        self.provider_tools.insert(provider_id, current);
        self.catalog_revisions.insert(provider_id, refresh.catalog_revision);
        Ok(refresh)
    }

    #[must_use]
    pub fn get(&self, reference: &ToolRef) -> Option<&ToolDescriptor> {
        self.tools.get(&reference.tool_id).filter(|tool| tool.provider_id == reference.provider_id)
    }

    #[must_use]
    pub fn active_tools(&self) -> Vec<&ToolDescriptor> {
        self.tools
            .values()
            .filter(|tool| tool.availability == ToolAvailability::Available)
            .collect()
    }

    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Vec<&ToolDescriptor> {
        let terms = query.split_whitespace().map(str::to_ascii_lowercase).collect::<Vec<_>>();
        self.tools
            .values()
            .filter(|tool| tool.availability == ToolAvailability::Available)
            .filter(|tool| {
                let haystack = format!(
                    "{} {} {} {}",
                    tool.native_name,
                    tool.title.as_deref().unwrap_or_default(),
                    tool.provider_description.as_deref().unwrap_or_default(),
                    tool.capabilities
                        .iter()
                        .map(|binding| binding.capability.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                )
                .to_ascii_lowercase();
                terms.iter().all(|term| haystack.contains(term))
            })
            .take(limit.clamp(1, 100))
            .collect()
    }

    #[must_use]
    pub fn schema(&self, revision: SchemaRevision) -> Option<&SchemaRecord> {
        self.schema_registry.get(revision)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityDefinition {
    pub id: CapabilityId,
    pub meaning: String,
    pub input_contract: String,
    pub output_contract: String,
    pub side_effects: OperationSemantics,
}

#[derive(Default)]
pub struct CapabilityRegistry {
    definitions: BTreeMap<CapabilityId, CapabilityDefinition>,
}

impl CapabilityRegistry {
    pub fn register(&mut self, definition: CapabilityDefinition) -> Result<(), ToolFabricError> {
        if self.definitions.contains_key(&definition.id) {
            return Err(ToolFabricError::CapabilityAlreadyRegistered(definition.id));
        }
        self.definitions.insert(definition.id.clone(), definition);
        Ok(())
    }

    #[must_use]
    pub fn list(&self) -> Vec<&CapabilityDefinition> {
        self.definitions.values().collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutingRequest {
    pub capability: CapabilityId,
    pub target: CapabilityTarget,
    pub preferred_provider: Option<ProviderId>,
}

pub struct DeterministicRouter;

impl DeterministicRouter {
    pub fn resolve(
        catalog: &ToolCatalog,
        providers: &ProviderRegistry,
        request: &RoutingRequest,
    ) -> Result<ToolRef, ToolFabricError> {
        let candidates = catalog
            .active_tools()
            .into_iter()
            .filter(|tool| {
                providers.get(tool.provider_id).is_some_and(|provider| {
                    matches!(
                        provider.health.connection_state,
                        ProviderConnectionState::Connected | ProviderConnectionState::Degraded
                    )
                })
            })
            .filter(|tool| {
                request.preferred_provider.is_none_or(|provider| tool.provider_id == provider)
            })
            .filter(|tool| {
                tool.capabilities.iter().any(|binding| {
                    binding.capability == request.capability
                        && target_matches(&binding.target, &request.target)
                })
            })
            .map(ToolDescriptor::reference)
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [] => Err(ToolFabricError::CapabilityUnavailable(request.capability.clone())),
            [only] => Ok(only.clone()),
            _ => Err(ToolFabricError::AmbiguousCapability {
                capability: request.capability.clone(),
                candidates,
            }),
        }
    }
}

pub struct BuiltinToolFabric {
    pub providers: ProviderRegistry,
    pub catalog: ToolCatalog,
    pub capabilities: CapabilityRegistry,
}

impl BuiltinToolFabric {
    /// Registers only capabilities that are implemented in the current build.
    /// Planned Git/Trace integrations remain visible but unavailable.
    pub fn from_environments(environments: &[StudioEnvironment]) -> Result<Self, ToolFabricError> {
        let mut providers = ProviderRegistry::default();
        let mut catalog = ToolCatalog::default();
        let mut capabilities = CapabilityRegistry::default();

        let workspace_status = CapabilityId::parse("lattice:workspace.status@1")?;
        let workspace_search = CapabilityId::parse("lattice:workspace.search@1")?;
        for definition in [
            CapabilityDefinition {
                id: workspace_status.clone(),
                meaning: "Read the active canonical workspace status".into(),
                input_contract: "empty object".into(),
                output_contract: "workspace identity and index revision".into(),
                side_effects: read_only_semantics(),
            },
            CapabilityDefinition {
                id: workspace_search.clone(),
                meaning: "Search the canonical workspace index".into(),
                input_contract: "query and bounded result limit".into(),
                output_contract: "compact resource references and evidence".into(),
                side_effects: read_only_semantics(),
            },
        ] {
            capabilities.register(definition)?;
        }

        let lattice_provider = ProviderId::from_stable_key(b"builtin:lattice-core");
        providers.register(provider_descriptor(
            lattice_provider,
            ProviderKind::BuiltIn,
            "Lattice Core",
            ProviderTrust::BuiltIn,
            ProviderTransportKind::InProcess,
            ProviderConnectionState::Connected,
            ProviderHealthStatus::Healthy,
            "built-in workspace services are available",
            "builtin:lattice-core",
            None,
        ))?;
        let refresh = catalog.refresh_provider(
            lattice_provider,
            vec![
                builtin_tool(
                    "lattice.workspace.status",
                    "Workspace Status",
                    workspace_status,
                    serde_json::json!({"type":"object","additionalProperties":false}),
                ),
                builtin_tool(
                    "lattice.search",
                    "Workspace Search",
                    workspace_search,
                    serde_json::json!({
                        "type":"object",
                        "properties":{
                            "query":{"type":"string"},
                            "limit":{"type":"integer","minimum":1,"maximum":50}
                        },
                        "required":["query"],
                        "additionalProperties":false
                    }),
                ),
            ],
        )?;
        update_catalog_health(&mut providers, lattice_provider, &refresh, 2)?;

        let git_provider = ProviderId::from_stable_key(b"builtin:git");
        providers.register(provider_descriptor(
            git_provider,
            ProviderKind::NativeAdapter,
            "Git",
            ProviderTrust::BuiltIn,
            ProviderTransportKind::NativeAdapter,
            ProviderConnectionState::Unavailable,
            ProviderHealthStatus::Unavailable,
            "git2 integration is not implemented in the current repository",
            "builtin:git",
            None,
        ))?;

        let trace_provider = ProviderId::from_stable_key(b"builtin:trace");
        providers.register(provider_descriptor(
            trace_provider,
            ProviderKind::NativeAdapter,
            "Flight Recorder",
            ProviderTrust::BuiltIn,
            ProviderTransportKind::NativeAdapter,
            ProviderConnectionState::Unavailable,
            ProviderHealthStatus::Unavailable,
            "Flight Recorder is not implemented in the current repository",
            "builtin:trace",
            None,
        ))?;

        for environment in environments {
            let key = format!("studio:{}", environment.id);
            let provider = ProviderId::from_stable_key(key.as_bytes());
            let (connection_state, reason) = if environment.mcp_launcher.is_some() {
                (
                    ProviderConnectionState::Configured,
                    "Studio MCP launch specification is resolved; lazy connection is not open",
                )
            } else {
                (
                    ProviderConnectionState::Unavailable,
                    "Studio environment is resolved, but no MCP launch specification is available",
                )
            };
            providers.register(provider_descriptor(
                provider,
                ProviderKind::McpStdio,
                "Roblox Studio MCP",
                ProviderTrust::BuiltIn,
                ProviderTransportKind::Stdio,
                connection_state,
                ProviderHealthStatus::Unavailable,
                reason,
                &key,
                Some(environment.id),
            ))?;
        }

        Ok(Self { providers, catalog, capabilities })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationId(LatticeId);

impl OperationId {
    #[must_use]
    pub fn new() -> Self {
        Self(LatticeId::new())
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "op_{}", self.0)
    }
}

impl FromStr for OperationId {
    type Err = ToolFabricError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let id = value.strip_prefix("op_").ok_or(ToolFabricError::InvalidOperationId)?;
        id.parse().map(Self).map_err(|_| ToolFabricError::InvalidOperationId)
    }
}

impl Serialize for OperationId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for OperationId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?.parse().map_err(D::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Queued,
    Validating,
    WaitingForPolicy,
    Executing,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Error)]
pub enum ToolFabricError {
    #[error("PROVIDER_NOT_FOUND: provider {0} is not registered")]
    ProviderNotFound(ProviderId),
    #[error("PROVIDER_ALREADY_REGISTERED: provider {0} is already registered")]
    ProviderAlreadyRegistered(ProviderId),
    #[error("TOOL_NOT_FOUND: tool reference is malformed")]
    InvalidToolRef,
    #[error("RESOURCE_NOT_FOUND: provider resource reference is malformed")]
    InvalidResourceRef,
    #[error("TOOL_ID_INVALID: malformed stable identifier")]
    InvalidStableId,
    #[error("OPERATION_ID_INVALID: malformed operation identifier")]
    InvalidOperationId,
    #[error("CAPABILITY_NOT_FOUND: malformed capability identifier {0}")]
    InvalidCapabilityId(String),
    #[error("CAPABILITY_ALREADY_REGISTERED: {0}")]
    CapabilityAlreadyRegistered(CapabilityId),
    #[error("CAPABILITY_UNAVAILABLE: {0}")]
    CapabilityUnavailable(CapabilityId),
    #[error("AMBIGUOUS_CAPABILITY: {capability} has multiple deterministic matches")]
    AmbiguousCapability { capability: CapabilityId, candidates: Vec<ToolRef> },
    #[error("TOOL_SCHEMA_INVALID: schema root must be a JSON object")]
    SchemaRootNotObject,
    #[error("TOOL_SCHEMA_INVALID: schema exceeds maximum nesting depth")]
    SchemaTooDeep,
    #[error("TOOL_SCHEMA_INVALID: schema is {0} bytes, exceeding the 1 MiB limit")]
    SchemaTooLarge(usize),
    #[error("TOOL_SCHEMA_INVALID: {0}")]
    Json(#[source] serde_json::Error),
    #[error("TOOL_SCHEMA_INVALID: duplicate native tool name {0}")]
    DuplicateToolName(String),
    #[error("TOOL_SCHEMA_INVALID: native tool name is empty")]
    InvalidToolName,
    #[error("INPUT_SCHEMA_FAILED: {0}")]
    InputSchemaFailed(String),
}

/// Validates the deterministic subset of JSON Schema used by the broker. A
/// schema using unsupported keywords is retained for inspection, but this
/// validator never claims those keywords were enforced.
pub fn validate_tool_input(
    schema: &serde_json::Value,
    input: &serde_json::Value,
) -> Result<(), ToolFabricError> {
    let object = schema.as_object().ok_or(ToolFabricError::SchemaRootNotObject)?;
    if object.get("type").and_then(serde_json::Value::as_str) == Some("object")
        && !input.is_object()
    {
        return Err(ToolFabricError::InputSchemaFailed("input must be a JSON object".into()));
    }
    let input_object = input
        .as_object()
        .ok_or_else(|| ToolFabricError::InputSchemaFailed("input must be a JSON object".into()))?;
    if let Some(required) = object.get("required").and_then(serde_json::Value::as_array) {
        for name in required.iter().filter_map(serde_json::Value::as_str) {
            if !input_object.contains_key(name) {
                return Err(ToolFabricError::InputSchemaFailed(format!(
                    "required property {name} is missing"
                )));
            }
        }
    }
    if let Some(properties) = object.get("properties").and_then(serde_json::Value::as_object) {
        for (name, value) in input_object {
            let Some(property_schema) = properties.get(name) else {
                if object.get("additionalProperties") == Some(&serde_json::Value::Bool(false)) {
                    return Err(ToolFabricError::InputSchemaFailed(format!(
                        "additional property {name} is not allowed"
                    )));
                }
                continue;
            };
            if let Some(expected) = property_schema.get("type").and_then(serde_json::Value::as_str)
                && !json_type_matches(value, expected)
            {
                return Err(ToolFabricError::InputSchemaFailed(format!(
                    "property {name} must be {expected}"
                )));
            }
        }
    }
    Ok(())
}

fn json_type_matches(value: &serde_json::Value, expected: &str) -> bool {
    match expected {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "string" => value.is_string(),
        _ => false,
    }
}

fn tool_id(provider: ProviderId, native_name: &str) -> ToolId {
    let key = format!("{provider}\0{native_name}");
    ToolId::from_stable_key(key.as_bytes())
}

fn catalog_revision<'a>(
    tools: impl Iterator<Item = &'a ToolDescriptor>,
) -> Result<ContentHash, ToolFabricError> {
    let encoded = serde_json::to_vec(&tools.collect::<Vec<_>>()).map_err(ToolFabricError::Json)?;
    Ok(ContentHash::of(&encoded))
}

fn target_matches(binding: &CapabilityTarget, requested: &CapabilityTarget) -> bool {
    binding == requested || matches!(binding, CapabilityTarget::Any)
}

fn read_only_semantics() -> OperationSemantics {
    OperationSemantics {
        read_only: SemanticTruth::KnownTrue,
        mutating: SemanticTruth::KnownFalse,
        destructive: SemanticTruth::KnownFalse,
        idempotent: SemanticTruth::KnownTrue,
        open_world: SemanticTruth::KnownFalse,
        network_access: SemanticTruth::KnownFalse,
        filesystem_access: SemanticTruth::KnownTrue,
        code_execution: SemanticTruth::KnownFalse,
        credential_use: SemanticTruth::KnownFalse,
        transaction_support: SemanticTruth::KnownFalse,
    }
}

#[allow(clippy::too_many_arguments)] // Bootstrap keeps every security/health field explicit.
fn provider_descriptor(
    id: ProviderId,
    kind: ProviderKind,
    name: &str,
    trust: ProviderTrust,
    transport: ProviderTransportKind,
    connection_state: ProviderConnectionState,
    status: ProviderHealthStatus,
    reason: &str,
    source: &str,
    studio_environment_id: Option<StudioEnvironmentId>,
) -> ProviderDescriptor {
    ProviderDescriptor {
        id,
        kind,
        name: name.into(),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        trust,
        transport,
        health: ProviderHealth {
            status,
            reason: reason.into(),
            last_successful_operation_unix_ms: None,
            last_failure: None,
            rtt_micros: None,
            consecutive_failures: 0,
            connected_at_unix_ms: None,
            catalog_revision: None,
            tool_count: 0,
            resource_count: 0,
            authentication: AuthenticationState::NotRequired,
            connection_state,
        },
        metadata: ProviderMetadata {
            source: source.into(),
            studio_environment_id,
            studio_session_id: None,
            configured_priority: None,
            protocol: None,
        },
    }
}

fn builtin_tool(
    native_name: &str,
    title: &str,
    capability: CapabilityId,
    input_schema: serde_json::Value,
) -> ImportedTool {
    ImportedTool {
        native_name: native_name.into(),
        title: Some(title.into()),
        provider_description: Some("Lattice-owned deterministic operation".into()),
        input_schema,
        output_schema: None,
        capabilities: vec![CapabilityBinding { capability, target: CapabilityTarget::Any }],
        verified_semantics: read_only_semantics(),
        reported_semantics: ReportedSemantics {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        },
        trust: ToolTrust::BuiltIn,
    }
}

fn update_catalog_health(
    providers: &mut ProviderRegistry,
    provider_id: ProviderId,
    refresh: &CatalogRefresh,
    tool_count: u64,
) -> Result<(), ToolFabricError> {
    let mut health = providers
        .get(provider_id)
        .ok_or(ToolFabricError::ProviderNotFound(provider_id))?
        .health
        .clone();
    health.catalog_revision = Some(refresh.catalog_revision);
    health.tool_count = tool_count;
    providers.update_health(provider_id, health)
}

fn validate_capability_id(value: &str) -> Result<(), ToolFabricError> {
    let Some((qualified, version)) = value.rsplit_once('@') else {
        return Err(ToolFabricError::InvalidCapabilityId(value.to_owned()));
    };
    let Some((namespace, name)) = qualified.split_once(':') else {
        return Err(ToolFabricError::InvalidCapabilityId(value.to_owned()));
    };
    let valid_piece = |piece: &str| {
        !piece.is_empty()
            && piece.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
            })
    };
    if !valid_piece(namespace)
        || !valid_piece(name)
        || version.is_empty()
        || !version.bytes().all(|byte| byte.is_ascii_digit())
        || version.starts_with('0')
    {
        return Err(ToolFabricError::InvalidCapabilityId(value.to_owned()));
    }
    Ok(())
}

fn validate_schema(value: &serde_json::Value) -> Result<(), ToolFabricError> {
    fn walk(value: &serde_json::Value, depth: usize) -> Result<(), ToolFabricError> {
        if depth > MAX_SCHEMA_DEPTH {
            return Err(ToolFabricError::SchemaTooDeep);
        }
        match value {
            serde_json::Value::Array(values) => {
                for value in values {
                    walk(value, depth + 1)?;
                }
            }
            serde_json::Value::Object(values) => {
                for value in values.values() {
                    walk(value, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    if !value.is_object() {
        return Err(ToolFabricError::SchemaRootNotObject);
    }
    walk(value, 0)
}

fn normalize_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(normalize_json).collect())
        }
        serde_json::Value::Object(values) => {
            let normalized =
                values.into_iter().map(|(key, value)| (key, normalize_json(value))).collect();
            serde_json::Value::Object(normalized)
        }
        other => other,
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn decode_id(value: &str, prefix: &str) -> Result<[u8; 16], ToolFabricError> {
    let hex = value.strip_prefix(prefix).ok_or(ToolFabricError::InvalidStableId)?;
    if hex.len() != 32 {
        return Err(ToolFabricError::InvalidStableId);
    }
    let mut output = [0_u8; 16];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        let high = decode_nibble(pair[0]).ok_or(ToolFabricError::InvalidStableId)?;
        let low = decode_nibble(pair[1]).ok_or(ToolFabricError::InvalidStableId)?;
        output[index] = (high << 4) | low;
    }
    Ok(output)
}

const fn decode_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_provider(id: ProviderId, name: &str) -> ProviderDescriptor {
        ProviderDescriptor {
            id,
            kind: ProviderKind::BuiltIn,
            name: name.into(),
            version: None,
            trust: ProviderTrust::BuiltIn,
            transport: ProviderTransportKind::InProcess,
            health: ProviderHealth {
                status: ProviderHealthStatus::Healthy,
                reason: "test".into(),
                last_successful_operation_unix_ms: None,
                last_failure: None,
                rtt_micros: None,
                consecutive_failures: 0,
                connected_at_unix_ms: Some(1),
                catalog_revision: None,
                tool_count: 0,
                resource_count: 0,
                authentication: AuthenticationState::NotRequired,
                connection_state: ProviderConnectionState::Connected,
            },
            metadata: ProviderMetadata {
                source: "test".into(),
                studio_environment_id: None,
                studio_session_id: None,
                configured_priority: None,
                protocol: None,
            },
        }
    }

    fn imported(name: &str, capability: Option<CapabilityBinding>) -> ImportedTool {
        ImportedTool {
            native_name: name.into(),
            title: None,
            provider_description: Some("untrusted description".into()),
            input_schema: serde_json::json!({"type":"object","properties":{"value":{"type":"string"}}}),
            output_schema: None,
            capabilities: capability.into_iter().collect(),
            verified_semantics: OperationSemantics::unknown(),
            reported_semantics: ReportedSemantics {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: None,
                open_world_hint: None,
            },
            trust: ToolTrust::Untrusted,
        }
    }

    #[test]
    fn identities_and_tool_references_are_stable() -> Result<(), Box<dyn std::error::Error>> {
        let provider = ProviderId::from_stable_key(b"studio-a");
        assert_eq!(provider, ProviderId::from_stable_key(b"studio-a"));
        let tool = tool_id(provider, "execute_luau");
        let reference = ToolRef { provider_id: provider, tool_id: tool };
        assert_eq!(reference.to_string().parse::<ToolRef>()?, reference);
        Ok(())
    }

    #[test]
    fn provider_resource_uri_is_opaque_not_concatenated() -> Result<(), Box<dyn std::error::Error>>
    {
        let provider = ProviderId::from_stable_key(b"resources");
        let original = "custom://host/a/b?q=x#fragment/ü";
        let resource_id = ProviderResourceId::for_original_uri(provider, original);
        let reference = ProviderResourceRef { provider_id: provider, resource_id };
        let encoded = reference.to_string();
        assert!(!encoded.contains(original));
        assert_eq!(encoded.parse::<ProviderResourceRef>()?, reference);
        Ok(())
    }

    #[test]
    fn input_validation_rejects_missing_wrong_and_additional_properties() {
        let schema = serde_json::json!({
            "type":"object",
            "properties":{"query":{"type":"string"}},
            "required":["query"],
            "additionalProperties":false
        });
        assert!(validate_tool_input(&schema, &serde_json::json!({"query":"ok"})).is_ok());
        assert!(validate_tool_input(&schema, &serde_json::json!({})).is_err());
        assert!(validate_tool_input(&schema, &serde_json::json!({"query":1})).is_err());
        assert!(
            validate_tool_input(&schema, &serde_json::json!({"query":"ok","extra":true})).is_err()
        );
    }

    #[test]
    fn operation_ids_use_the_wire_prefix() -> Result<(), Box<dyn std::error::Error>> {
        let id = OperationId::new();
        let wire = id.to_string();
        assert!(wire.starts_with("op_"));
        assert_eq!(wire.parse::<OperationId>()?, id);
        assert_eq!(serde_json::from_str::<OperationId>(&serde_json::to_string(&id)?)?, id);
        Ok(())
    }

    #[test]
    fn schema_change_retains_tool_identity_and_marks_revision_change()
    -> Result<(), Box<dyn std::error::Error>> {
        let provider = ProviderId::from_stable_key(b"provider");
        let mut catalog = ToolCatalog::default();
        let first = catalog.refresh_provider(provider, vec![imported("tool_a", None)])?;
        let id = first.added[0];
        let mut changed = imported("tool_a", None);
        changed.input_schema = serde_json::json!({"type":"object","required":["value"]});
        let second = catalog.refresh_provider(provider, vec![changed])?;
        assert_eq!(second.schema_changed, vec![id]);
        assert!(second.added.is_empty());
        Ok(())
    }

    #[test]
    fn removed_tools_remain_historical_but_leave_active_search()
    -> Result<(), Box<dyn std::error::Error>> {
        let provider = ProviderId::from_stable_key(b"provider");
        let mut catalog = ToolCatalog::default();
        let first = catalog.refresh_provider(provider, vec![imported("tool_a", None)])?;
        let reference = ToolRef { provider_id: provider, tool_id: first.added[0] };
        let second = catalog.refresh_provider(provider, Vec::new())?;
        assert_eq!(second.removed, vec![reference.tool_id]);
        assert_eq!(
            catalog.get(&reference).map(|tool| tool.availability),
            Some(ToolAvailability::Unavailable)
        );
        assert!(catalog.search("tool_a", 10).is_empty());
        Ok(())
    }

    #[test]
    fn provider_hints_do_not_change_verified_semantics() -> Result<(), Box<dyn std::error::Error>> {
        let provider = ProviderId::from_stable_key(b"malicious");
        let mut catalog = ToolCatalog::default();
        let refresh =
            catalog.refresh_provider(provider, vec![imported("delete_database", None)])?;
        let reference = ToolRef { provider_id: provider, tool_id: refresh.added[0] };
        let tool = catalog.get(&reference).ok_or(ToolFabricError::InvalidToolRef)?;
        assert_eq!(tool.reported_semantics.read_only_hint, Some(true));
        assert_eq!(tool.verified_semantics.read_only, SemanticTruth::Unknown);
        Ok(())
    }

    #[test]
    fn unconstrained_multiple_implementations_are_ambiguous()
    -> Result<(), Box<dyn std::error::Error>> {
        let capability = CapabilityId::parse("roblox:runtime.execute-luau@1")?;
        let first = ProviderId::from_stable_key(b"studio");
        let second = ProviderId::from_stable_key(b"cloud");
        let mut providers = ProviderRegistry::default();
        providers.register(healthy_provider(first, "Studio"))?;
        providers.register(healthy_provider(second, "Cloud"))?;
        let binding =
            CapabilityBinding { capability: capability.clone(), target: CapabilityTarget::Any };
        let mut catalog = ToolCatalog::default();
        catalog.refresh_provider(first, vec![imported("execute", Some(binding.clone()))])?;
        catalog.refresh_provider(second, vec![imported("execute", Some(binding))])?;
        let result = DeterministicRouter::resolve(
            &catalog,
            &providers,
            &RoutingRequest { capability, target: CapabilityTarget::Any, preferred_provider: None },
        );
        assert!(matches!(result, Err(ToolFabricError::AmbiguousCapability { .. })));
        Ok(())
    }
}
