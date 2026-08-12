use std::{
    collections::BTreeMap,
    fmt,
    path::{Component, Path, PathBuf},
    str::FromStr,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

pub const RESOLVER_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HostPlatform {
    Windows,
    Linux,
    MacOS,
    Unknown,
}

impl HostPlatform {
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::MacOS
        } else {
            Self::Unknown
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StudioRuntime {
    RobloxNative,
    VinegarNative,
    VinegarFlatpak,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathNamespace {
    Host,
    WineGuest,
    StudioGuest,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostPath(PathBuf);

impl HostPath {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(lexically_normalize(&path.into()))
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    #[must_use]
    pub fn join(&self, relative: impl AsRef<Path>) -> Self {
        Self::new(self.0.join(relative))
    }

    #[must_use]
    pub fn starts_with(&self, root: &Self) -> bool {
        self.0.starts_with(&root.0)
    }

    #[must_use]
    pub fn strip_prefix(&self, root: &Self) -> Option<&Path> {
        self.0.strip_prefix(&root.0).ok()
    }

    /// Lexically compares host paths using the known platform's default behavior.
    /// This is correlation evidence and must not become canonical resource identity.
    #[must_use]
    pub fn equivalent_on(&self, other: &Self, platform: HostPlatform) -> bool {
        match platform {
            HostPlatform::Windows => {
                let left = self
                    .0
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                let right = other
                    .0
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                left.len() == right.len()
                    && left.iter().zip(right).all(|(left, right)| left.eq_ignore_ascii_case(&right))
            }
            HostPlatform::Linux | HostPlatform::MacOS | HostPlatform::Unknown => self == other,
        }
    }
}

impl fmt::Display for HostPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(formatter)
    }
}

impl From<PathBuf> for HostPath {
    fn from(value: PathBuf) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WinePath {
    drive: char,
    components: Vec<String>,
}

impl WinePath {
    #[must_use]
    pub const fn drive(&self) -> char {
        self.drive
    }

    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.components.iter().map(String::as_str)
    }

    #[must_use]
    pub fn from_parts(drive: char, components: Vec<String>) -> Self {
        Self { drive: drive.to_ascii_uppercase(), components }
    }

    /// Compares Wine guest paths using Windows-style case-insensitive evidence.
    #[must_use]
    pub fn equivalent_guest(&self, other: &Self) -> bool {
        self.drive == other.drive
            && self.components.len() == other.components.len()
            && self
                .components
                .iter()
                .zip(&other.components)
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
    }
}

impl FromStr for WinePath {
    type Err = PathError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.starts_with("\\\\") || value.starts_with("//") {
            return Err(PathError::UnsupportedUnc);
        }
        let bytes = value.as_bytes();
        if bytes.len() < 3
            || !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || !matches!(bytes[2], b'\\' | b'/')
        {
            return Err(PathError::MalformedWinePath(value.to_owned()));
        }

        let mut components = Vec::new();
        for component in value[3..].split(['\\', '/']) {
            match component {
                "" | "." => {}
                ".." => return Err(PathError::Traversal),
                component if component.contains('\0') || component.contains(':') => {
                    return Err(PathError::MalformedWinePath(value.to_owned()));
                }
                component => components.push(component.to_owned()),
            }
        }
        Ok(Self::from_parts(char::from(bytes[0]), components))
    }
}

impl fmt::Display for WinePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:\\{}", self.drive, self.components.join("\\"))
    }
}

impl Serialize for WinePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for WinePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::from_str(&String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedPath {
    pub host: Option<HostPath>,
    pub guest: Option<WinePath>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudioPathRole {
    Config,
    Data,
    Cache,
    Prefix,
    RobloxAppData,
    Deployment,
    Logs,
    Profiler,
    CrashData,
    TemporaryExports,
    McpServer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathAvailability {
    Available,
    Missing,
    PermissionDenied,
    SandboxDenied,
    Unavailable,
    Ambiguous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionOrigin {
    Detected,
    Configured,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PathResolution {
    pub role: StudioPathRole,
    pub availability: PathAvailability,
    pub value: Option<ResolvedPath>,
    pub origin: ResolutionOrigin,
    pub detail: Option<String>,
}

impl PathResolution {
    #[must_use]
    pub fn unavailable(role: StudioPathRole, detail: impl Into<String>) -> Self {
        Self {
            role,
            availability: PathAvailability::Unavailable,
            value: None,
            origin: ResolutionOrigin::Detected,
            detail: Some(detail.into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct StudioEnvironmentCapabilities {
    pub host_filesystem_access: bool,
    pub wine_path_translation: bool,
    pub process_telemetry: bool,
    pub studio_logs: bool,
    pub profiler_files: bool,
    pub crash_data: bool,
    pub studio_mcp_launch: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StudioEnvironmentId([u8; 16]);

impl StudioEnvironmentId {
    #[must_use]
    pub fn from_fingerprint(fingerprint: &[u8]) -> Self {
        let digest = blake3::hash(fingerprint);
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        Self(bytes)
    }
}

impl fmt::Display for StudioEnvironmentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "env_{}", encode_hex(&self.0))
    }
}

impl Serialize for StudioEnvironmentId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for StudioEnvironmentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        decode_id(&String::deserialize(deserializer)?, "env_").map(Self).map_err(D::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StudioDeploymentId([u8; 16]);

impl StudioDeploymentId {
    #[must_use]
    pub(crate) fn from_fingerprint(fingerprint: &[u8]) -> Self {
        let digest = blake3::hash(fingerprint);
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        Self(bytes)
    }
}

impl fmt::Display for StudioDeploymentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "deployment_{}", encode_hex(&self.0))
    }
}

impl Serialize for StudioDeploymentId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for StudioDeploymentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        decode_id(&String::deserialize(deserializer)?, "deployment_")
            .map(Self)
            .map_err(D::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioDeployment {
    pub id: StudioDeploymentId,
    pub path: HostPath,
    pub build_identifier: Option<String>,
    pub runtime: StudioRuntime,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioProcessInfo {
    pub pid: u32,
    pub executable: Option<HostPath>,
    pub parent_pid: Option<u32>,
    pub runtime: StudioRuntime,
    pub deployment: Option<StudioDeploymentId>,
    pub start_time_unix_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WineRuntime {
    pub executable: HostPath,
    pub root: HostPath,
    pub source_process_id: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioMcpLauncher {
    pub executable: HostPath,
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub target_executable: Option<ResolvedPath>,
    pub experimental: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolutionDiagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioEnvironmentCandidate {
    pub id: StudioEnvironmentId,
    pub host_platform: HostPlatform,
    pub runtime: StudioRuntime,
    pub config_root: Option<HostPath>,
    pub data_root: Option<HostPath>,
    pub cache_root: Option<HostPath>,
    pub process_hint: Option<u32>,
    pub process_start_time_unix_seconds: Option<u64>,
    pub origin: ResolutionOrigin,
    pub diagnostics: Vec<ResolutionDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioEnvironment {
    pub id: StudioEnvironmentId,
    pub resolver_version: u32,
    pub host_platform: HostPlatform,
    pub runtime: StudioRuntime,
    pub studio_process: Option<StudioProcessInfo>,
    pub related_processes: Vec<StudioProcessInfo>,
    pub config_root: Option<HostPath>,
    pub data_root: Option<HostPath>,
    pub cache_root: Option<HostPath>,
    pub studio_prefix: Option<HostPath>,
    pub roblox_appdata: Option<ResolvedPath>,
    pub studio_deployment: Option<StudioDeployment>,
    pub logs_root: Option<ResolvedPath>,
    pub profiler_root: Option<ResolvedPath>,
    pub wine_runtime: Option<WineRuntime>,
    pub mcp_launcher: Option<StudioMcpLauncher>,
    pub wine_drive_mappings: BTreeMap<char, HostPath>,
    pub capabilities: StudioEnvironmentCapabilities,
    pub paths: BTreeMap<StudioPathRole, PathResolution>,
    pub diagnostics: Vec<ResolutionDiagnostic>,
}

impl StudioEnvironment {
    #[must_use]
    pub fn path(&self, role: StudioPathRole) -> Option<&PathResolution> {
        self.paths.get(&role)
    }

    /// Builds the personal-path-free environment identity for future trace manifests.
    #[must_use]
    pub fn fingerprint(&self) -> StudioEnvironmentFingerprint {
        StudioEnvironmentFingerprint {
            resolver_version: self.resolver_version,
            host_platform: self.host_platform,
            runtime: self.runtime,
            environment_id: self.id,
            deployment_id: self.studio_deployment.as_ref().map(|deployment| deployment.id),
            deployment_build: self
                .studio_deployment
                .as_ref()
                .and_then(|deployment| deployment.build_identifier.clone()),
            process_id: self.studio_process.as_ref().map(|process| process.pid),
            process_start_time_unix_seconds: self
                .studio_process
                .as_ref()
                .map(|process| process.start_time_unix_seconds),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StudioEnvironmentFingerprint {
    pub resolver_version: u32,
    pub host_platform: HostPlatform,
    pub runtime: StudioRuntime,
    pub environment_id: StudioEnvironmentId,
    pub deployment_id: Option<StudioDeploymentId>,
    pub deployment_build: Option<String>,
    pub process_id: Option<u32>,
    pub process_start_time_unix_seconds: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionStatus {
    Resolved,
    NotFound,
    Ambiguous,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlatformInspection {
    pub host_platform: HostPlatform,
    pub status: InspectionStatus,
    pub selected_environment: Option<StudioEnvironmentId>,
    pub environments: Vec<StudioEnvironment>,
    pub diagnostics: Vec<ResolutionDiagnostic>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResolverOverrides {
    pub vinegar_data_root: Option<HostPath>,
    pub studio_prefix: Option<HostPath>,
    pub studio_deployment: Option<HostPath>,
    pub roblox_appdata: Option<HostPath>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PathError {
    #[error("malformed absolute Wine path: {0}")]
    MalformedWinePath(String),
    #[error("UNC Wine paths are not supported")]
    UnsupportedUnc,
    #[error("path traversal is not permitted")]
    Traversal,
    #[error("drive {0}: is not mapped in this Studio environment")]
    DriveNotMapped(char),
    #[error("translated path escapes the configured drive mapping")]
    PrefixEscape,
    #[error("host path is not represented by a Wine drive mapping")]
    HostPathNotMapped,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PlatformError {
    #[error("unsupported host platform")]
    UnsupportedHost,
    #[error("invalid configured override {name}: {reason}")]
    InvalidOverride { name: String, reason: String },
    #[error("environment candidate does not belong to this resolver")]
    ForeignCandidate,
    #[error("path error: {0}")]
    Path(#[from] PathError),
}

fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn decode_id(value: &str, prefix: &str) -> Result<[u8; 16], String> {
    let value = value.strip_prefix(prefix).ok_or_else(|| format!("missing {prefix} prefix"))?;
    if value.len() != 32 {
        return Err("identifier must contain 32 hexadecimal digits".to_owned());
    }
    let mut bytes = [0_u8; 16];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|error| error.to_string())?;
        bytes[index] = u8::from_str_radix(text, 16).map_err(|error| error.to_string())?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn wine_path_rejects_traversal_and_unc() {
        assert_eq!(WinePath::from_str(r"C:\safe\..\escape"), Err(PathError::Traversal));
        assert_eq!(WinePath::from_str(r"\\server\share"), Err(PathError::UnsupportedUnc));
    }

    #[test]
    fn comparison_is_namespace_and_platform_aware() -> Result<(), PathError> {
        let upper = HostPath::new("C:/Users/Player/Roblox");
        let lower = HostPath::new("c:/users/player/roblox");
        assert!(upper.equivalent_on(&lower, HostPlatform::Windows));
        assert!(!upper.equivalent_on(&lower, HostPlatform::Linux));
        let guest_upper = WinePath::from_str(r"C:\Users\Player\Roblox")?;
        let guest_lower = WinePath::from_str(r"c:\users\player\roblox")?;
        assert!(guest_upper.equivalent_guest(&guest_lower));
        Ok(())
    }

    #[test]
    fn host_path_lexical_normalization_does_not_require_existence() {
        assert_eq!(
            HostPath::new("/not-created/./child/../result"),
            HostPath::new("/not-created/result")
        );
    }

    #[test]
    fn identifiers_are_stable_and_serializable() -> Result<(), Box<dyn std::error::Error>> {
        let first = StudioEnvironmentId::from_fingerprint(b"vinegar:/example");
        let second = StudioEnvironmentId::from_fingerprint(b"vinegar:/example");
        assert_eq!(first, second);
        let encoded = serde_json::to_string(&first)?;
        let decoded: StudioEnvironmentId = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, first);
        Ok(())
    }

    proptest! {
        #[test]
        fn wine_path_parse_display_round_trip(
            drive in proptest::char::range('A', 'Z'),
            components in prop::collection::vec("[A-Za-z0-9_-]{1,12}", 0..8)
        ) {
            let original = WinePath::from_parts(drive, components);
            let parsed = WinePath::from_str(&original.to_string());
            prop_assert_eq!(parsed, Ok(original));
        }

        #[test]
        fn parent_components_are_always_rejected(
            drive in proptest::char::range('A', 'Z'),
            prefix in "[A-Za-z0-9_-]{1,12}",
            suffix in "[A-Za-z0-9_-]{1,12}"
        ) {
            let value = format!(r"{drive}:\{prefix}\..\{suffix}");
            prop_assert_eq!(WinePath::from_str(&value), Err(PathError::Traversal));
        }
    }
}
