//! Protocol-neutral canonical project and evidence model.

use lattice_resource::{ContentHash, LatticeId, ResourceRef};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Project,
    Place,
    Instance,
    Script,
    ModuleScript,
    LocalScript,
    Symbol,
    Function,
    Method,
    Type,
    Variable,
    RemoteEvent,
    RemoteFunction,
    Service,
    Controller,
    System,
    Package,
    Test,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: LatticeId,
    pub resource_ref: ResourceRef,
    pub kind: EntityKind,
    pub name: String,
    pub display_path: Option<String>,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourcePosition {
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub begin: SourcePosition,
    pub end: SourcePosition,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceUnit {
    pub entity: Entity,
    pub content_hash: ContentHash,
    pub language: String,
    pub byte_len: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceOrigin {
    StaticAst,
    TypeAnalysis,
    GraphRelationship,
    TextMatch,
    GitHistory,
    RuntimeLog,
    Playtest,
    OpenCloudExecution,
    UserStatement,
    SemanticInference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Certain,
    Probable,
    Possible,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub id: LatticeId,
    pub kind: String,
    pub resource_ref: ResourceRef,
    pub source_span: Option<SourceSpan>,
    pub revision: u64,
    pub origin: EvidenceOrigin,
    pub confidence: Confidence,
    pub payload_hash: ContentHash,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub resource_ref: ResourceRef,
    pub display_path: String,
    pub name: String,
    pub score_milli: i64,
    pub content_hash: ContentHash,
    pub evidence: Evidence,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LatticeErrorBody {
    pub code: ErrorCode,
    pub message: String,
    pub recoverable: bool,
    pub recommended_action: Option<String>,
    pub related_resources: Vec<ResourceRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    WorkspaceNotFound,
    ResourceNotFound,
    AmbiguousResource,
    StudioNotConnected,
    StudioCapabilityUnavailable,
    SourceParseFailed,
    SourceChanged,
    RevisionConflict,
    PolicyDenied,
    ChangeSetInvalid,
    ChangeSetConflict,
    ChangeSetApplyFailed,
    RollbackFailed,
    ResultExpired,
    ProviderNotFound,
    ProviderUnavailable,
    ProviderAuthRequired,
    ProviderConnectionFailed,
    ProviderDegraded,
    ToolNotFound,
    ToolUnavailable,
    ToolSchemaInvalid,
    ToolSchemaChanged,
    CapabilityNotFound,
    CapabilityUnavailable,
    AmbiguousCapability,
    InputSchemaFailed,
    OperationTimeout,
    OperationCancelled,
    ResultTooLarge,
    ProviderProtocolError,
    Internal,
}

impl ErrorCode {
    /// Stable ordering used by generated reference documentation.
    pub const ALL: &'static [Self] = &[
        Self::WorkspaceNotFound,
        Self::ResourceNotFound,
        Self::AmbiguousResource,
        Self::StudioNotConnected,
        Self::StudioCapabilityUnavailable,
        Self::SourceParseFailed,
        Self::SourceChanged,
        Self::RevisionConflict,
        Self::PolicyDenied,
        Self::ChangeSetInvalid,
        Self::ChangeSetConflict,
        Self::ChangeSetApplyFailed,
        Self::RollbackFailed,
        Self::ResultExpired,
        Self::ProviderNotFound,
        Self::ProviderUnavailable,
        Self::ProviderAuthRequired,
        Self::ProviderConnectionFailed,
        Self::ProviderDegraded,
        Self::ToolNotFound,
        Self::ToolUnavailable,
        Self::ToolSchemaInvalid,
        Self::ToolSchemaChanged,
        Self::CapabilityNotFound,
        Self::CapabilityUnavailable,
        Self::AmbiguousCapability,
        Self::InputSchemaFailed,
        Self::OperationTimeout,
        Self::OperationCancelled,
        Self::ResultTooLarge,
        Self::ProviderProtocolError,
        Self::Internal,
    ];

    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::WorkspaceNotFound => "WORKSPACE_NOT_FOUND",
            Self::ResourceNotFound => "RESOURCE_NOT_FOUND",
            Self::AmbiguousResource => "AMBIGUOUS_RESOURCE",
            Self::StudioNotConnected => "STUDIO_NOT_CONNECTED",
            Self::StudioCapabilityUnavailable => "STUDIO_CAPABILITY_UNAVAILABLE",
            Self::SourceParseFailed => "SOURCE_PARSE_FAILED",
            Self::SourceChanged => "SOURCE_CHANGED",
            Self::RevisionConflict => "REVISION_CONFLICT",
            Self::PolicyDenied => "POLICY_DENIED",
            Self::ChangeSetInvalid => "CHANGE_SET_INVALID",
            Self::ChangeSetConflict => "CHANGE_SET_CONFLICT",
            Self::ChangeSetApplyFailed => "CHANGE_SET_APPLY_FAILED",
            Self::RollbackFailed => "ROLLBACK_FAILED",
            Self::ResultExpired => "RESULT_EXPIRED",
            Self::ProviderNotFound => "PROVIDER_NOT_FOUND",
            Self::ProviderUnavailable => "PROVIDER_UNAVAILABLE",
            Self::ProviderAuthRequired => "PROVIDER_AUTH_REQUIRED",
            Self::ProviderConnectionFailed => "PROVIDER_CONNECTION_FAILED",
            Self::ProviderDegraded => "PROVIDER_DEGRADED",
            Self::ToolNotFound => "TOOL_NOT_FOUND",
            Self::ToolUnavailable => "TOOL_UNAVAILABLE",
            Self::ToolSchemaInvalid => "TOOL_SCHEMA_INVALID",
            Self::ToolSchemaChanged => "TOOL_SCHEMA_CHANGED",
            Self::CapabilityNotFound => "CAPABILITY_NOT_FOUND",
            Self::CapabilityUnavailable => "CAPABILITY_UNAVAILABLE",
            Self::AmbiguousCapability => "AMBIGUOUS_CAPABILITY",
            Self::InputSchemaFailed => "INPUT_SCHEMA_FAILED",
            Self::OperationTimeout => "OPERATION_TIMEOUT",
            Self::OperationCancelled => "OPERATION_CANCELLED",
            Self::ResultTooLarge => "RESULT_TOO_LARGE",
            Self::ProviderProtocolError => "PROVIDER_PROTOCOL_ERROR",
            Self::Internal => "INTERNAL",
        }
    }

    #[must_use]
    pub const fn recoverable(self) -> bool {
        !matches!(self, Self::RollbackFailed | Self::Internal)
    }

    #[must_use]
    pub const fn mechanical_action(self) -> &'static str {
        match self {
            Self::WorkspaceNotFound => "Select or register a canonical workspace.",
            Self::ResourceNotFound => "Resolve the canonical resource again.",
            Self::AmbiguousResource => "Provide an explicit canonical reference.",
            Self::StudioNotConnected => "Inspect Studio environment/session health and reconnect.",
            Self::StudioCapabilityUnavailable => {
                "Use a provider/platform where the capability is available."
            }
            Self::SourceParseFailed => "Inspect the source and parser diagnostics.",
            Self::SourceChanged | Self::RevisionConflict => {
                "Re-read the external source and reconcile the change."
            }
            Self::PolicyDenied => "Request an allowed operation or change policy explicitly.",
            Self::ChangeSetInvalid => "Correct the ChangeSet contract before retrying.",
            Self::ChangeSetConflict => "Refresh the target and resolve the conflicting revision.",
            Self::ChangeSetApplyFailed => "Inspect adapter evidence and retry only when safe.",
            Self::RollbackFailed => "Stop and inspect the target state before further mutation.",
            Self::ResultExpired => "Re-run the operation to create a new result.",
            Self::ProviderNotFound => "Inspect the provider registry and use a current ProviderId.",
            Self::ProviderUnavailable => {
                "Reconnect the provider or select another explicit implementation."
            }
            Self::ProviderAuthRequired => "Configure the provider credential reference.",
            Self::ProviderConnectionFailed => {
                "Inspect transport diagnostics and retry with bounded backoff."
            }
            Self::ProviderDegraded => {
                "Inspect health evidence before issuing expensive operations."
            }
            Self::ToolNotFound => "Search the active catalog for a current ToolRef.",
            Self::ToolUnavailable => "Reconnect the provider or use a configured alternative.",
            Self::ToolSchemaInvalid => "Quarantine the provider schema and inspect its catalog.",
            Self::ToolSchemaChanged => "Inspect the new SchemaRevision and validate inputs again.",
            Self::CapabilityNotFound => "Use a validated versioned CapabilityId.",
            Self::CapabilityUnavailable => {
                "Connect a provider that explicitly implements the capability."
            }
            Self::AmbiguousCapability => {
                "Provide an explicit target or configured provider preference."
            }
            Self::InputSchemaFailed => "Correct the arguments against the inspected schema.",
            Self::OperationTimeout => {
                "Inspect provider health; retry only when operation semantics allow it."
            }
            Self::OperationCancelled => "Confirm provider cancellation state before retrying.",
            Self::ResultTooLarge => {
                "Read the bounded result reference instead of requesting inline content."
            }
            Self::ProviderProtocolError => {
                "Inspect negotiated protocol profile and provider diagnostics."
            }
            Self::Internal => "Preserve the OperationId and inspect daemon logs.",
        }
    }
}

#[derive(Debug, Error)]
#[error("{body:?}")]
pub struct LatticeError {
    pub body: LatticeErrorBody,
}

impl LatticeError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            body: LatticeErrorBody {
                code,
                message: message.into(),
                recoverable,
                recommended_action: None,
                related_resources: Vec::new(),
            },
        }
    }

    #[must_use]
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.body.recommended_action = Some(action.into());
        self
    }
}
