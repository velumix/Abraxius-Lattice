//! Lattice-owned MCP lifecycle compatibility profiles.
//!
//! Version-dependent behavior is selected here so provider and Studio adapters
//! do not scatter protocol comparisons or accidentally use RMCP's legacy
//! default lifecycle.

use lattice_tools::ProviderProtocolMetadata;
use rmcp::{ClientLifecycleMode, model::ProtocolVersion};
use serde::{Deserialize, Serialize};

pub const MODERN_MCP_REVISION: &str = "2026-07-28";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpProtocolProfile {
    Modern2026_07_28,
    Legacy2025 { revision: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpNegotiationPath {
    ServerDiscover,
    InitializeFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpSessionModel {
    StatelessPerRequest,
    LegacyConnectionSession,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpCatalogChangeModel {
    SubscriptionsListen,
    LegacyNotifications,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpProtocolFeatures {
    pub per_request_metadata: bool,
    pub session_model: McpSessionModel,
    pub cancellation_model: String,
    pub catalog_change_model: McpCatalogChangeModel,
    pub standalone_http_get_stream: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpNegotiation {
    pub profile: McpProtocolProfile,
    pub negotiated_revision: String,
    pub path: McpNegotiationPath,
    pub fallback_reason: Option<String>,
    pub features: McpProtocolFeatures,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum McpStartupMode {
    Automatic,
    VerifiedLegacyFallback { reason: String },
}

impl McpNegotiation {
    #[must_use]
    pub fn from_negotiated_version(version: &ProtocolVersion) -> Self {
        if version >= &ProtocolVersion::V_2026_07_28 {
            Self {
                profile: McpProtocolProfile::Modern2026_07_28,
                negotiated_revision: MODERN_MCP_REVISION.into(),
                path: McpNegotiationPath::ServerDiscover,
                fallback_reason: None,
                features: McpProtocolFeatures {
                    per_request_metadata: true,
                    session_model: McpSessionModel::StatelessPerRequest,
                    cancellation_model: "transport-specific request cancellation".into(),
                    catalog_change_model: McpCatalogChangeModel::SubscriptionsListen,
                    standalone_http_get_stream: false,
                },
            }
        } else {
            Self {
                profile: McpProtocolProfile::Legacy2025 { revision: version.to_string() },
                negotiated_revision: version.to_string(),
                path: McpNegotiationPath::InitializeFallback,
                fallback_reason: Some(
                    "server/discover returned JSON-RPC method-not-found; legacy initialize was used"
                        .into(),
                ),
                features: McpProtocolFeatures {
                    per_request_metadata: false,
                    session_model: McpSessionModel::LegacyConnectionSession,
                    cancellation_model: "notifications/cancelled".into(),
                    catalog_change_model: McpCatalogChangeModel::LegacyNotifications,
                    standalone_http_get_stream: true,
                },
            }
        }
    }

    #[must_use]
    pub fn with_fallback_reason(mut self, reason: Option<String>) -> Self {
        if reason.is_some() {
            self.fallback_reason = reason;
            self.path = McpNegotiationPath::InitializeFallback;
        }
        self
    }

    #[must_use]
    pub fn provider_metadata(&self) -> ProviderProtocolMetadata {
        ProviderProtocolMetadata {
            family: "MCP".into(),
            revision: self.negotiated_revision.clone(),
            negotiation: match self.path {
                McpNegotiationPath::ServerDiscover => "server/discover",
                McpNegotiationPath::InitializeFallback => "initialize fallback",
            }
            .into(),
            session_model: match self.features.session_model {
                McpSessionModel::StatelessPerRequest => "stateless per request",
                McpSessionModel::LegacyConnectionSession => "legacy connection session",
            }
            .into(),
            cancellation_model: self.features.cancellation_model.clone(),
            catalog_change_model: match self.features.catalog_change_model {
                McpCatalogChangeModel::SubscriptionsListen => "subscriptions/listen",
                McpCatalogChangeModel::LegacyNotifications => "legacy notifications",
            }
            .into(),
            fallback_reason: self.fallback_reason.clone(),
        }
    }
}

/// Deterministic compatibility startup. RMCP only falls back when discovery
/// returns JSON-RPC `METHOD_NOT_FOUND`; malformed or incompatible responses are
/// propagated as failures.
#[must_use]
pub fn automatic_lifecycle() -> ClientLifecycleMode {
    ClientLifecycleMode::Auto {
        preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        legacy_version: Some(ProtocolVersion::V_2025_11_25),
    }
}

#[must_use]
pub fn lifecycle_for(mode: &McpStartupMode) -> ClientLifecycleMode {
    match mode {
        McpStartupMode::Automatic => automatic_lifecycle(),
        McpStartupMode::VerifiedLegacyFallback { .. } => ClientLifecycleMode::Initialize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_do_not_share_session_semantics() {
        let modern = McpNegotiation::from_negotiated_version(&ProtocolVersion::V_2026_07_28);
        assert_eq!(modern.path, McpNegotiationPath::ServerDiscover);
        assert_eq!(modern.features.session_model, McpSessionModel::StatelessPerRequest);
        assert!(!modern.features.standalone_http_get_stream);

        let legacy = McpNegotiation::from_negotiated_version(&ProtocolVersion::V_2025_11_25);
        assert_eq!(legacy.path, McpNegotiationPath::InitializeFallback);
        assert_eq!(legacy.features.session_model, McpSessionModel::LegacyConnectionSession);
        assert!(legacy.features.standalone_http_get_stream);
    }
}
