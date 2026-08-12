//! Centralized, protocol-neutral host and Roblox Studio environment resolution.
//!
//! Consumers request semantic roles from [`StudioEnvironment`]. Platform and
//! Vinegar layout details are intentionally confined to this crate.

mod filesystem;
mod model;
mod process;
mod resolver;
mod translation;

pub use filesystem::{
    FileSystemEntry, FileSystemError, FileSystemErrorKind, PlatformFileSystem, RealFileSystem,
};
pub use model::{
    HostPath, HostPlatform, InspectionStatus, PathAvailability, PathError, PathNamespace,
    PathResolution, PlatformError, PlatformInspection, RESOLVER_VERSION, ResolutionDiagnostic,
    ResolutionOrigin, ResolvedPath, ResolverOverrides, StudioDeployment, StudioDeploymentId,
    StudioEnvironment, StudioEnvironmentCandidate, StudioEnvironmentCapabilities,
    StudioEnvironmentFingerprint, StudioEnvironmentId, StudioMcpLauncher, StudioPathRole,
    StudioProcessInfo, StudioRuntime, WinePath, WineRuntime,
};
pub use process::{ProcessSnapshot, ProcessSource, SysinfoProcessSource};
pub use resolver::{PlatformContext, PlatformResolver, StudioEnvironmentResolver};
pub use translation::{PathTranslator, WinePathTranslator};
