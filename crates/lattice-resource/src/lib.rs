//! Stable canonical identities and content revisions for Lattice.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;
use uuid::Uuid;

const RBX_SCHEME: &str = "rbx://";

/// A stable Lattice identifier. Display paths are deliberately not identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LatticeId(Uuid);

impl LatticeId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for LatticeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for LatticeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for LatticeId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

/// BLAKE3 identity of immutable content.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    /// Parses a BLAKE3 hash with an optional `b3:` prefix.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceRefError::InvalidContentHash`] when the value is not
    /// exactly one BLAKE3 digest in hexadecimal form.
    pub fn from_hex(value: &str) -> Result<Self, ResourceRefError> {
        let value = value.strip_prefix("b3:").unwrap_or(value);
        let hash = blake3::Hash::from_hex(value)
            .map_err(|_| ResourceRefError::InvalidContentHash(value.to_owned()))?;
        Ok(Self(*hash.as_bytes()))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b3:{}", blake3::Hash::from_bytes(self.0).to_hex())
    }
}

impl Serialize for ContentHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_hex(&value).map_err(D::Error::custom)
    }
}

/// Authority and addressing scope of a Roblox resource.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum ResourceScope {
    Workspace { workspace_id: LatticeId },
    Studio { session_id: LatticeId },
    Cloud { universe_id: u64, place_id: u64, version: u64 },
    Test { test_run_id: LatticeId },
}

/// Canonical resource category. Unknown future categories remain representable.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Project,
    Place,
    Instance,
    Script,
    Symbol,
    Result,
    Diagnostic,
    ChangeSet,
    TestResult,
    Other(String),
}

impl ResourceKind {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Project => "project",
            Self::Place => "place",
            Self::Instance => "instance",
            Self::Script => "script",
            Self::Symbol => "symbol",
            Self::Result | Self::TestResult => "result",
            Self::Diagnostic => "diagnostic",
            Self::ChangeSet => "changeset",
            Self::Other(value) => value,
        }
    }
}

impl From<&str> for ResourceKind {
    fn from(value: &str) -> Self {
        match value {
            "project" => Self::Project,
            "place" => Self::Place,
            "instance" => Self::Instance,
            "script" => Self::Script,
            "symbol" => Self::Symbol,
            "result" => Self::Result,
            "diagnostic" => Self::Diagnostic,
            "changeset" => Self::ChangeSet,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// A small, serializable, centrally resolvable `rbx://` reference.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ResourceRef {
    pub scope: ResourceScope,
    pub kind: ResourceKind,
    pub id: LatticeId,
}

impl ResourceRef {
    #[must_use]
    pub const fn workspace(workspace_id: LatticeId, kind: ResourceKind, id: LatticeId) -> Self {
        Self { scope: ResourceScope::Workspace { workspace_id }, kind, id }
    }

    #[must_use]
    pub const fn studio(session_id: LatticeId, kind: ResourceKind, id: LatticeId) -> Self {
        Self { scope: ResourceScope::Studio { session_id }, kind, id }
    }
}

impl fmt::Display for ResourceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.scope {
            ResourceScope::Workspace { workspace_id } => {
                write!(
                    f,
                    "rbx://workspace/{workspace_id}/{}/{id}",
                    self.kind.as_str(),
                    id = self.id
                )
            }
            ResourceScope::Studio { session_id } => {
                write!(f, "rbx://studio/{session_id}/{}/{id}", self.kind.as_str(), id = self.id)
            }
            ResourceScope::Cloud { universe_id, place_id, version } => write!(
                f,
                "rbx://cloud/{universe_id}/{place_id}/version/{version}/{}/{id}",
                self.kind.as_str(),
                id = self.id
            ),
            ResourceScope::Test { test_run_id } => {
                write!(f, "rbx://test/{test_run_id}/{}/{id}", self.kind.as_str(), id = self.id)
            }
        }
    }
}

impl FromStr for ResourceRef {
    type Err = ResourceRefError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let body = value.strip_prefix(RBX_SCHEME).ok_or(ResourceRefError::InvalidScheme)?;
        let segments: Vec<_> = body.split('/').collect();
        if segments.iter().any(|segment| segment.is_empty()) {
            return Err(ResourceRefError::InvalidGrammar);
        }

        match segments.as_slice() {
            ["workspace", workspace, kind, id] => Ok(Self {
                scope: ResourceScope::Workspace { workspace_id: parse_id(workspace)? },
                kind: ResourceKind::from(*kind),
                id: parse_id(id)?,
            }),
            ["studio", session, kind, id] => Ok(Self {
                scope: ResourceScope::Studio { session_id: parse_id(session)? },
                kind: ResourceKind::from(*kind),
                id: parse_id(id)?,
            }),
            ["cloud", universe, place, "version", version, kind, id] => Ok(Self {
                scope: ResourceScope::Cloud {
                    universe_id: parse_number(universe)?,
                    place_id: parse_number(place)?,
                    version: parse_number(version)?,
                },
                kind: ResourceKind::from(*kind),
                id: parse_id(id)?,
            }),
            ["test", run, kind, id] => Ok(Self {
                scope: ResourceScope::Test { test_run_id: parse_id(run)? },
                kind: ResourceKind::from(*kind),
                id: parse_id(id)?,
            }),
            _ => Err(ResourceRefError::InvalidGrammar),
        }
    }
}

impl Serialize for ResourceRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ResourceRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

fn parse_id(value: &str) -> Result<LatticeId, ResourceRefError> {
    value.parse().map_err(|_| ResourceRefError::InvalidId(value.to_owned()))
}

fn parse_number(value: &str) -> Result<u64, ResourceRefError> {
    value.parse().map_err(|_| ResourceRefError::InvalidNumber(value.to_owned()))
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ResourceRefError {
    #[error("resource reference must use the rbx:// scheme")]
    InvalidScheme,
    #[error("resource reference does not match the canonical grammar")]
    InvalidGrammar,
    #[error("invalid Lattice identifier: {0}")]
    InvalidId(String),
    #[error("invalid numeric identifier: {0}")]
    InvalidNumber(String),
    #[error("invalid BLAKE3 content hash: {0}")]
    InvalidContentHash(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn workspace_reference_round_trips(workspace in any::<[u8; 16]>(), entity in any::<[u8; 16]>()) {
            let reference = ResourceRef::workspace(
                LatticeId::from_uuid(Uuid::from_bytes(workspace)),
                ResourceKind::Script,
                LatticeId::from_uuid(Uuid::from_bytes(entity)),
            );
            prop_assert_eq!(reference.to_string().parse::<ResourceRef>(), Ok(reference));
        }
    }

    #[test]
    fn paths_are_rejected_as_identity() {
        assert!(
            "rbx://workspace/game.ReplicatedStorage/script/foo".parse::<ResourceRef>().is_err()
        );
    }

    #[test]
    fn hashes_are_prefixed_and_round_trip() -> Result<(), serde_json::Error> {
        let hash = ContentHash::of(b"inventory");
        assert_eq!(ContentHash::from_hex(&hash.to_string()), Ok(hash));
        assert_eq!(serde_json::to_string(&hash)?, format!("\"{hash}\""));
        Ok(())
    }

    #[test]
    fn resource_wire_form_is_the_canonical_uri() -> Result<(), serde_json::Error> {
        let reference =
            ResourceRef::workspace(LatticeId::new(), ResourceKind::Script, LatticeId::new());
        assert_eq!(serde_json::to_string(&reference)?, format!("\"{reference}\""));
        Ok(())
    }
}
