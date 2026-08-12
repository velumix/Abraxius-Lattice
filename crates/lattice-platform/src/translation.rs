use std::collections::BTreeMap;

use crate::{
    HostPath, PathError, PlatformFileSystem, StudioEnvironment, WinePath,
    filesystem::FileSystemErrorKind,
};

pub trait PathTranslator {
    fn guest_to_host(
        &self,
        path: &WinePath,
        environment: &StudioEnvironment,
    ) -> Result<HostPath, PathError>;

    fn host_to_guest(
        &self,
        path: &HostPath,
        environment: &StudioEnvironment,
    ) -> Result<WinePath, PathError>;
}

pub struct WinePathTranslator<'a> {
    filesystem: &'a dyn PlatformFileSystem,
}

impl<'a> WinePathTranslator<'a> {
    #[must_use]
    pub const fn new(filesystem: &'a dyn PlatformFileSystem) -> Self {
        Self { filesystem }
    }
}

impl PathTranslator for WinePathTranslator<'_> {
    fn guest_to_host(
        &self,
        path: &WinePath,
        environment: &StudioEnvironment,
    ) -> Result<HostPath, PathError> {
        let root = environment
            .wine_drive_mappings
            .get(&path.drive())
            .ok_or(PathError::DriveNotMapped(path.drive()))?;
        let mut translated = root.clone();
        for component in path.components() {
            translated = translated.join(component);
        }
        if !translated.starts_with(root) {
            return Err(PathError::PrefixEscape);
        }
        self.reject_symlink_escape(root, &translated)?;
        Ok(translated)
    }

    fn host_to_guest(
        &self,
        path: &HostPath,
        environment: &StudioEnvironment,
    ) -> Result<WinePath, PathError> {
        let normalized_path = self.filesystem.canonicalize(path).unwrap_or_else(|_| path.clone());
        let mappings = canonical_mappings(self.filesystem, &environment.wine_drive_mappings);
        let (_, drive, root) = mappings
            .into_iter()
            .filter_map(|(drive, root)| {
                normalized_path
                    .strip_prefix(&root)
                    .map(|_| (root.as_path().components().count(), drive, root))
            })
            .max_by_key(|(depth, _, _)| *depth)
            .ok_or(PathError::HostPathNotMapped)?;
        let relative = normalized_path.strip_prefix(&root).ok_or(PathError::HostPathNotMapped)?;
        let components = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();
        Ok(WinePath::from_parts(drive, components))
    }
}

impl WinePathTranslator<'_> {
    fn reject_symlink_escape(&self, root: &HostPath, target: &HostPath) -> Result<(), PathError> {
        let canonical_root = self.filesystem.canonicalize(root).unwrap_or_else(|_| root.clone());
        let mut existing = target.clone();
        loop {
            match self.filesystem.canonicalize(&existing) {
                Ok(canonical) => {
                    if canonical.starts_with(&canonical_root) {
                        return Ok(());
                    }
                    return Err(PathError::PrefixEscape);
                }
                Err(error) if error.kind == FileSystemErrorKind::Missing => {
                    let Some(parent) = existing.as_path().parent() else {
                        return Err(PathError::PrefixEscape);
                    };
                    let parent = HostPath::new(parent.to_path_buf());
                    if parent == existing || !parent.starts_with(root) {
                        return Err(PathError::PrefixEscape);
                    }
                    existing = parent;
                }
                Err(_) => return Ok(()),
            }
        }
    }
}

fn canonical_mappings(
    filesystem: &dyn PlatformFileSystem,
    mappings: &BTreeMap<char, HostPath>,
) -> Vec<(char, HostPath)> {
    mappings
        .iter()
        .map(|(drive, root)| {
            (*drive, filesystem.canonicalize(root).unwrap_or_else(|_| root.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HostPlatform, RESOLVER_VERSION, RealFileSystem, StudioEnvironmentCapabilities,
        StudioEnvironmentId, StudioRuntime,
    };
    use proptest::prelude::*;
    use tempfile::TempDir;

    fn environment(drive_c: HostPath) -> StudioEnvironment {
        StudioEnvironment {
            id: StudioEnvironmentId::from_fingerprint(b"translation-property"),
            resolver_version: RESOLVER_VERSION,
            host_platform: HostPlatform::Linux,
            runtime: StudioRuntime::VinegarNative,
            studio_process: None,
            related_processes: Vec::new(),
            config_root: None,
            data_root: None,
            cache_root: None,
            studio_prefix: drive_c.as_path().parent().map(|path| HostPath::new(path.to_path_buf())),
            roblox_appdata: None,
            studio_deployment: None,
            logs_root: None,
            profiler_root: None,
            wine_runtime: None,
            mcp_launcher: None,
            wine_drive_mappings: BTreeMap::from([('C', drive_c)]),
            capabilities: StudioEnvironmentCapabilities::default(),
            paths: BTreeMap::new(),
            diagnostics: Vec::new(),
        }
    }

    proptest! {
        #[test]
        fn fully_resolvable_guest_host_round_trip(
            components in prop::collection::vec("[A-Za-z0-9_-]{1,12}", 0..8)
        ) {
            let temporary = TempDir::new().map_err(|error| TestCaseError::fail(error.to_string()))?;
            let drive_c = HostPath::new(temporary.path().join("drive_c"));
            std::fs::create_dir_all(drive_c.as_path())
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let environment = environment(drive_c);
            let guest = WinePath::from_parts('C', components);
            let translator = WinePathTranslator::new(&RealFileSystem);
            let host = translator
                .guest_to_host(&guest, &environment)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let round_trip = translator
                .host_to_guest(&host, &environment)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(round_trip, guest);
        }
    }
}
