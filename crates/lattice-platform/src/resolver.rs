use std::{collections::BTreeMap, path::Path};

use crate::{
    FileSystemErrorKind, HostPath, HostPlatform, InspectionStatus, PathAvailability,
    PathResolution, PlatformError, PlatformFileSystem, PlatformInspection, ProcessSnapshot,
    ProcessSource, RESOLVER_VERSION, RealFileSystem, ResolutionDiagnostic, ResolutionOrigin,
    ResolvedPath, ResolverOverrides, StudioDeployment, StudioDeploymentId, StudioEnvironment,
    StudioEnvironmentCandidate, StudioEnvironmentCapabilities, StudioEnvironmentId,
    StudioMcpLauncher, StudioPathRole, StudioProcessInfo, StudioRuntime, SysinfoProcessSource,
    WinePath, WineRuntime,
};

const VINEGAR_FLATPAK_ID: &str = "org.vinegarhq.Vinegar";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformContext {
    pub host_platform: HostPlatform,
    pub home_dir: Option<HostPath>,
    pub config_dir: Option<HostPath>,
    pub data_local_dir: Option<HostPath>,
    pub cache_dir: Option<HostPath>,
    pub sandboxed: bool,
    pub studio_mcp_command: Option<HostPath>,
    pub overrides: ResolverOverrides,
}

impl PlatformContext {
    #[must_use]
    pub fn current() -> Self {
        Self {
            host_platform: HostPlatform::current(),
            home_dir: dirs::home_dir().map(HostPath::new),
            config_dir: dirs::config_dir().map(HostPath::new),
            data_local_dir: dirs::data_local_dir().map(HostPath::new),
            cache_dir: dirs::cache_dir().map(HostPath::new),
            sandboxed: std::env::var_os("FLATPAK_ID").is_some(),
            studio_mcp_command: std::env::var_os("LATTICE_STUDIO_MCP_COMMAND").map(HostPath::new),
            overrides: ResolverOverrides {
                vinegar_data_root: environment_path("LATTICE_VINEGAR_DATA_ROOT"),
                studio_prefix: environment_path("LATTICE_STUDIO_PREFIX"),
                studio_deployment: environment_path("LATTICE_STUDIO_DEPLOYMENT"),
                roblox_appdata: environment_path("LATTICE_ROBLOX_APPDATA"),
            },
        }
    }
}

pub trait StudioEnvironmentResolver {
    fn detect_all(&self) -> Result<Vec<StudioEnvironmentCandidate>, PlatformError>;
    fn resolve(
        &self,
        candidate: &StudioEnvironmentCandidate,
    ) -> Result<StudioEnvironment, PlatformError>;
}

pub struct PlatformResolver<F = RealFileSystem, P = SysinfoProcessSource> {
    context: PlatformContext,
    filesystem: F,
    process_source: P,
}

impl PlatformResolver<RealFileSystem, SysinfoProcessSource> {
    #[must_use]
    pub fn current() -> Self {
        Self {
            context: PlatformContext::current(),
            filesystem: RealFileSystem,
            process_source: SysinfoProcessSource,
        }
    }
}

impl<F, P> PlatformResolver<F, P>
where
    F: PlatformFileSystem,
    P: ProcessSource,
{
    #[must_use]
    pub const fn new(context: PlatformContext, filesystem: F, process_source: P) -> Self {
        Self { context, filesystem, process_source }
    }

    #[must_use]
    pub const fn context(&self) -> &PlatformContext {
        &self.context
    }

    pub fn inspect(&self) -> Result<PlatformInspection, PlatformError> {
        let candidates = self.detect_all()?;
        let mut environments = candidates
            .iter()
            .map(|candidate| self.resolve(candidate))
            .collect::<Result<Vec<_>, _>>()?;
        environments.sort_by_key(|environment| environment.id);

        let process_environments = environments
            .iter()
            .filter(|environment| environment.studio_process.is_some())
            .map(|environment| environment.id)
            .collect::<Vec<_>>();
        let (status, selected_environment) = match (environments.len(), process_environments.len())
        {
            (0, _) => (InspectionStatus::NotFound, None),
            (_, 1) => (InspectionStatus::Resolved, process_environments.first().copied()),
            (1, 0) => (InspectionStatus::Resolved, environments.first().map(|value| value.id)),
            _ => (InspectionStatus::Ambiguous, None),
        };
        let mut diagnostics = Vec::new();
        match status {
            InspectionStatus::Resolved => diagnostics.push(diagnostic(
                "STUDIO_ENVIRONMENT_RESOLVED",
                "one Studio environment was selected from filesystem and process evidence",
            )),
            InspectionStatus::NotFound => diagnostics.push(diagnostic(
                "STUDIO_ENVIRONMENT_NOT_FOUND",
                "no native Studio or Vinegar environment was detected",
            )),
            InspectionStatus::Ambiguous => diagnostics.push(diagnostic(
                "STUDIO_ENVIRONMENT_AMBIGUOUS",
                "multiple Studio environments remain; select an environment or running session explicitly",
            )),
            InspectionStatus::Unavailable => diagnostics.push(diagnostic(
                "STUDIO_ENVIRONMENT_UNAVAILABLE",
                "the current host platform is unsupported",
            )),
        }

        Ok(PlatformInspection {
            host_platform: self.context.host_platform,
            status,
            selected_environment,
            environments,
            diagnostics,
        })
    }

    fn detect_linux(
        &self,
        processes: &[ProcessSnapshot],
    ) -> Result<Vec<StudioEnvironmentCandidate>, PlatformError> {
        let home = self.context.home_dir.as_ref().ok_or(PlatformError::UnsupportedHost)?;
        let flatpak_root = home.join(Path::new(".var/app").join(VINEGAR_FLATPAK_ID));
        let flatpak = CandidateRoots {
            runtime: StudioRuntime::VinegarFlatpak,
            config: flatpak_root.join("config/vinegar"),
            data: flatpak_root.join("data/vinegar"),
            cache: flatpak_root.join("cache/vinegar"),
            origin: ResolutionOrigin::Detected,
        };

        let config = self
            .context
            .config_dir
            .as_ref()
            .map(|root| root.join("vinegar"))
            .ok_or(PlatformError::UnsupportedHost)?;
        let default_data = self
            .context
            .data_local_dir
            .as_ref()
            .map(|root| root.join("vinegar"))
            .ok_or(PlatformError::UnsupportedHost)?;
        let cache = self
            .context
            .cache_dir
            .as_ref()
            .map(|root| root.join("vinegar"))
            .ok_or(PlatformError::UnsupportedHost)?;
        let (data, origin) = self
            .context
            .overrides
            .vinegar_data_root
            .as_ref()
            .map_or((default_data, ResolutionOrigin::Detected), |configured| {
                (configured.clone(), ResolutionOrigin::Configured)
            });
        if origin == ResolutionOrigin::Configured {
            self.validate_override("vinegar_data_root", &data)?;
        }
        let native =
            CandidateRoots { runtime: StudioRuntime::VinegarNative, config, data, cache, origin };

        let mut candidates = Vec::new();
        let has_native_override = self.context.overrides.vinegar_data_root.is_some()
            || self.context.overrides.studio_prefix.is_some()
            || self.context.overrides.studio_deployment.is_some()
            || self.context.overrides.roblox_appdata.is_some();
        for roots in [flatpak, native] {
            let process_matches = processes
                .iter()
                .filter(|process| self.process_matches_roots(process, &roots))
                .collect::<Vec<_>>();
            if !self.any_detected([&roots.config, &roots.data, &roots.cache])
                && process_matches.is_empty()
                && !(roots.runtime == StudioRuntime::VinegarNative && has_native_override)
            {
                continue;
            }
            if process_matches.is_empty() {
                candidates.push(self.make_candidate(&roots, None));
            } else {
                candidates.extend(
                    process_matches
                        .into_iter()
                        .map(|process| self.make_candidate(&roots, Some(process))),
                );
            }
        }
        Ok(candidates)
    }

    fn detect_windows(&self, processes: &[ProcessSnapshot]) -> Vec<StudioEnvironmentCandidate> {
        let Some(local_data) = self.context.data_local_dir.as_ref() else {
            return Vec::new();
        };
        let roots = CandidateRoots {
            runtime: StudioRuntime::RobloxNative,
            config: local_data.join("Roblox"),
            data: local_data.join("Roblox"),
            cache: local_data.join("Roblox"),
            origin: ResolutionOrigin::Detected,
        };
        self.native_candidates(&roots, processes)
    }

    fn detect_macos(&self, processes: &[ProcessSnapshot]) -> Vec<StudioEnvironmentCandidate> {
        let Some(home) = self.context.home_dir.as_ref() else {
            return Vec::new();
        };
        let roots = CandidateRoots {
            runtime: StudioRuntime::RobloxNative,
            config: home.join("Library/Preferences/Roblox"),
            data: home.join("Library/Roblox"),
            cache: home.join("Library/Caches/Roblox"),
            origin: ResolutionOrigin::Detected,
        };
        let application_detected = self.any_detected([
            &HostPath::new("/Applications/RobloxStudio.app"),
            &home.join("Applications/RobloxStudio.app"),
        ]);
        let mut candidates = self.native_candidates(&roots, processes);
        if application_detected && candidates.is_empty() {
            candidates.push(self.make_candidate(&roots, None));
        }
        candidates
    }

    fn native_candidates(
        &self,
        roots: &CandidateRoots,
        processes: &[ProcessSnapshot],
    ) -> Vec<StudioEnvironmentCandidate> {
        let matches =
            processes.iter().filter(|process| is_studio_process(process)).collect::<Vec<_>>();
        if !self.any_detected([&roots.config, &roots.data, &roots.cache]) && matches.is_empty() {
            return Vec::new();
        }
        if matches.is_empty() {
            vec![self.make_candidate(roots, None)]
        } else {
            matches.into_iter().map(|process| self.make_candidate(roots, Some(process))).collect()
        }
    }

    fn make_candidate(
        &self,
        roots: &CandidateRoots,
        process: Option<&ProcessSnapshot>,
    ) -> StudioEnvironmentCandidate {
        let fingerprint = format!(
            "{}:{:?}:{}:{}:{}",
            RESOLVER_VERSION,
            roots.runtime,
            roots.data,
            process.map_or(0, |value| value.pid),
            process.map_or(0, |value| value.start_time_unix_seconds)
        );
        StudioEnvironmentCandidate {
            id: StudioEnvironmentId::from_fingerprint(fingerprint.as_bytes()),
            host_platform: self.context.host_platform,
            runtime: roots.runtime,
            config_root: Some(roots.config.clone()),
            data_root: Some(roots.data.clone()),
            cache_root: Some(roots.cache.clone()),
            process_hint: process.map(|value| value.pid),
            process_start_time_unix_seconds: process.map(|value| value.start_time_unix_seconds),
            origin: roots.origin,
            diagnostics: vec![diagnostic(
                match roots.runtime {
                    StudioRuntime::VinegarFlatpak => "VINEGAR_FLATPAK_ROOT_DETECTED",
                    StudioRuntime::VinegarNative => "VINEGAR_XDG_ROOT_DETECTED",
                    StudioRuntime::RobloxNative => "NATIVE_STUDIO_ROOT_DETECTED",
                    StudioRuntime::Unknown => "STUDIO_ROOT_DETECTED",
                },
                format!("candidate data root: {}", roots.data),
            )],
        }
    }

    fn process_matches_roots(&self, process: &ProcessSnapshot, roots: &CandidateRoots) -> bool {
        if !is_studio_process(process) {
            return false;
        }
        let prefix = self
            .context
            .overrides
            .studio_prefix
            .clone()
            .unwrap_or_else(|| roots.data.join("prefixes/studio"));
        process.environment_value("WINEPREFIX").is_some_and(|value| HostPath::new(value) == prefix)
            || process_mentions_path(process, &prefix)
            || process_mentions_path(process, &roots.data)
            || (roots.runtime == StudioRuntime::VinegarFlatpak
                && process.environment_value("FLATPAK_ID") == Some(VINEGAR_FLATPAK_ID))
    }

    fn resolve_candidate(
        &self,
        candidate: &StudioEnvironmentCandidate,
        processes: &[ProcessSnapshot],
    ) -> Result<StudioEnvironment, PlatformError> {
        if candidate.host_platform != self.context.host_platform {
            return Err(PlatformError::ForeignCandidate);
        }
        let mut diagnostics = candidate.diagnostics.clone();
        let config_resolution = self.resolve_known_path(
            StudioPathRole::Config,
            candidate.config_root.clone(),
            ResolutionOrigin::Detected,
            None,
        );
        let data_resolution = self.resolve_known_path(
            StudioPathRole::Data,
            candidate.data_root.clone(),
            candidate.origin,
            None,
        );
        let cache_resolution = self.resolve_known_path(
            StudioPathRole::Cache,
            candidate.cache_root.clone(),
            ResolutionOrigin::Detected,
            None,
        );

        let prefix_attempt = if matches!(
            candidate.runtime,
            StudioRuntime::VinegarNative | StudioRuntime::VinegarFlatpak
        ) {
            let prefix =
                self.context.overrides.studio_prefix.clone().or_else(|| {
                    candidate.data_root.as_ref().map(|root| root.join("prefixes/studio"))
                });
            if let Some(configured) = self.context.overrides.studio_prefix.as_ref() {
                self.validate_override("studio_prefix", configured)?;
            }
            prefix
        } else {
            None
        };
        let prefix_origin = if self.context.overrides.studio_prefix.is_some() {
            ResolutionOrigin::Configured
        } else {
            ResolutionOrigin::Detected
        };
        let prefix_resolution =
            self.resolve_known_path(StudioPathRole::Prefix, prefix_attempt, prefix_origin, None);
        let studio_prefix = available_host(&prefix_resolution);
        if studio_prefix.is_none()
            && matches!(
                candidate.runtime,
                StudioRuntime::VinegarNative | StudioRuntime::VinegarFlatpak
            )
        {
            diagnostics.push(diagnostic(
                "VINEGAR_STUDIO_PREFIX_MISSING",
                "Vinegar was detected but its Studio Wine prefix is unavailable",
            ));
        }

        let wine_drive_mappings = studio_prefix
            .as_ref()
            .map_or_else(BTreeMap::new, |prefix| self.discover_drive_mappings(prefix));
        let hinted_process = candidate.process_hint.and_then(|pid| {
            processes.iter().find(|process| {
                process.pid == pid
                    && candidate
                        .process_start_time_unix_seconds
                        .is_none_or(|start| process.start_time_unix_seconds == start)
            })
        });
        let (studio_deployment, deployment_resolution) =
            self.resolve_deployment(candidate, hinted_process)?;
        let deployment_id = studio_deployment.as_ref().map(|deployment| deployment.id);
        let studio_process =
            hinted_process.map(|process| process_info(process, candidate.runtime, deployment_id));
        if let Some(process) = studio_process.as_ref() {
            diagnostics.push(diagnostic(
                "STUDIO_PROCESS_CORRELATED",
                format!("Studio process {} is correlated with this environment", process.pid),
            ));
        }

        let (roblox_appdata, appdata_resolution) = self.resolve_roblox_appdata(
            candidate,
            studio_prefix.as_ref(),
            &wine_drive_mappings,
            hinted_process,
            processes,
            &mut diagnostics,
        )?;
        let appdata_host = roblox_appdata.as_ref().and_then(|path| path.host.as_ref());
        let fallback_logs = match candidate.runtime {
            StudioRuntime::VinegarNative | StudioRuntime::VinegarFlatpak => {
                candidate.cache_root.as_ref().map(|root| root.join("logs"))
            }
            StudioRuntime::RobloxNative if candidate.host_platform == HostPlatform::MacOS => {
                self.context.home_dir.as_ref().map(|home| home.join("Library/Logs/Roblox"))
            }
            StudioRuntime::RobloxNative => {
                candidate.data_root.as_ref().map(|root| root.join("logs"))
            }
            StudioRuntime::Unknown => None,
        };
        let logs_attempt = self.first_detected(
            [appdata_host.map(|root| root.join("logs")), fallback_logs].into_iter().flatten(),
        );
        let logs_guest =
            logs_attempt.as_ref().and_then(|path| guest_for_host(path, &wine_drive_mappings));
        let logs_resolution = self.resolve_known_path(
            StudioPathRole::Logs,
            logs_attempt,
            ResolutionOrigin::Detected,
            logs_guest,
        );
        let logs_root = available_resolved(&logs_resolution);

        let profiler_attempt = self.first_detected(
            [
                appdata_host.map(|root| root.join("Profiler")),
                logs_root.as_ref().and_then(|path| path.host.clone()),
            ]
            .into_iter()
            .flatten(),
        );
        let profiler_guest =
            profiler_attempt.as_ref().and_then(|path| guest_for_host(path, &wine_drive_mappings));
        let profiler_resolution = self.resolve_known_path(
            StudioPathRole::Profiler,
            profiler_attempt,
            ResolutionOrigin::Detected,
            profiler_guest,
        );
        let profiler_root = available_resolved(&profiler_resolution);
        let crash_attempt = self.first_detected(
            [
                appdata_host.map(|root| root.join("CrashReports")),
                appdata_host.map(|root| root.join("logs/crashes")),
            ]
            .into_iter()
            .flatten(),
        );
        let crash_guest =
            crash_attempt.as_ref().and_then(|path| guest_for_host(path, &wine_drive_mappings));
        let crash_resolution = self.resolve_known_path(
            StudioPathRole::CrashData,
            crash_attempt,
            ResolutionOrigin::Detected,
            crash_guest,
        );
        let exports_attempt = candidate.data_root.as_ref().map(|root| root.join("exports"));
        let exports_resolution = self.resolve_known_path(
            StudioPathRole::TemporaryExports,
            exports_attempt,
            ResolutionOrigin::Detected,
            None,
        );

        let related_processes = studio_process.as_ref().map_or_else(Vec::new, |main| {
            Self::related_processes(
                main.pid,
                candidate.runtime,
                deployment_id,
                studio_prefix.as_ref(),
                processes,
            )
        });
        let wine_runtime = self.resolve_wine_runtime(candidate, hinted_process, processes);
        let (mcp_launcher, mcp_resolution) = self.resolve_mcp_launcher(
            candidate,
            studio_deployment.as_ref(),
            &wine_drive_mappings,
            studio_prefix.as_ref(),
            wine_runtime.as_ref(),
        );
        if candidate.host_platform == HostPlatform::Linux
            && mcp_resolution.availability == PathAvailability::Available
            && mcp_launcher.is_none()
        {
            diagnostics.push(diagnostic(
                "STUDIO_MCP_BINARY_DETECTED_INVOCATION_UNAVAILABLE",
                "StudioMCP.exe exists, but Roblox does not document a Linux/Vinegar launcher contract; the binary was not executed",
            ));
        }

        let mut paths = BTreeMap::new();
        for resolution in [
            config_resolution,
            data_resolution,
            cache_resolution,
            prefix_resolution,
            appdata_resolution,
            deployment_resolution,
            logs_resolution,
            profiler_resolution,
            crash_resolution,
            exports_resolution,
            mcp_resolution,
        ] {
            paths.insert(resolution.role, resolution);
        }

        let capabilities = StudioEnvironmentCapabilities {
            host_filesystem_access: paths.values().any(|path| {
                path.availability == PathAvailability::Available
                    && path.value.as_ref().is_some_and(|value| value.host.is_some())
            }),
            wine_path_translation: !wine_drive_mappings.is_empty(),
            process_telemetry: studio_process.is_some(),
            studio_logs: paths
                .get(&StudioPathRole::Logs)
                .is_some_and(|path| path.availability == PathAvailability::Available),
            profiler_files: paths
                .get(&StudioPathRole::Profiler)
                .is_some_and(|path| path.availability == PathAvailability::Available),
            crash_data: paths
                .get(&StudioPathRole::CrashData)
                .is_some_and(|path| path.availability == PathAvailability::Available),
            studio_mcp_launch: mcp_launcher.is_some(),
        };

        Ok(StudioEnvironment {
            id: candidate.id,
            resolver_version: RESOLVER_VERSION,
            host_platform: candidate.host_platform,
            runtime: candidate.runtime,
            studio_process,
            related_processes,
            config_root: available_host(
                paths.get(&StudioPathRole::Config).ok_or(PlatformError::ForeignCandidate)?,
            ),
            data_root: available_host(
                paths.get(&StudioPathRole::Data).ok_or(PlatformError::ForeignCandidate)?,
            ),
            cache_root: available_host(
                paths.get(&StudioPathRole::Cache).ok_or(PlatformError::ForeignCandidate)?,
            ),
            studio_prefix,
            roblox_appdata,
            studio_deployment,
            logs_root,
            profiler_root,
            wine_runtime,
            mcp_launcher,
            wine_drive_mappings,
            capabilities,
            paths,
            diagnostics,
        })
    }

    fn resolve_roblox_appdata(
        &self,
        candidate: &StudioEnvironmentCandidate,
        prefix: Option<&HostPath>,
        mappings: &BTreeMap<char, HostPath>,
        process: Option<&ProcessSnapshot>,
        processes: &[ProcessSnapshot],
        diagnostics: &mut Vec<ResolutionDiagnostic>,
    ) -> Result<(Option<ResolvedPath>, PathResolution), PlatformError> {
        if let Some(configured) = self.context.overrides.roblox_appdata.as_ref() {
            self.validate_override("roblox_appdata", configured)?;
            let guest = guest_for_host(configured, mappings);
            let resolution = self.resolve_known_path(
                StudioPathRole::RobloxAppData,
                Some(configured.clone()),
                ResolutionOrigin::Configured,
                guest,
            );
            return Ok((available_resolved(&resolution), resolution));
        }
        if candidate.runtime == StudioRuntime::RobloxNative {
            let resolution = self.resolve_known_path(
                StudioPathRole::RobloxAppData,
                candidate.data_root.clone(),
                ResolutionOrigin::Detected,
                None,
            );
            return Ok((available_resolved(&resolution), resolution));
        }
        let Some(prefix) = prefix else {
            return Ok((
                None,
                PathResolution::unavailable(
                    StudioPathRole::RobloxAppData,
                    "Studio prefix is unavailable",
                ),
            ));
        };

        if let Some(data_root) = candidate.data_root.as_ref() {
            let redirected = data_root.join("appdata/Roblox");
            let redirected_guest = guest_for_host(&redirected, mappings);
            let process_evidence = self.is_detected(&redirected)
                && processes.iter().any(|process| {
                    process_mentions_resolved_path(process, &redirected, redirected_guest.as_ref())
                });
            if process_evidence {
                let resolution = self.resolve_known_path(
                    StudioPathRole::RobloxAppData,
                    Some(redirected),
                    ResolutionOrigin::Detected,
                    redirected_guest,
                );
                diagnostics.push(diagnostic(
                    "VINEGAR_APPDATA_REDIRECT_CORRELATED",
                    "active process arguments identify Vinegar's host-side Roblox AppData redirect",
                ));
                return Ok((available_resolved(&resolution), resolution));
            }
        }

        let users_root = prefix.join("drive_c/users");
        let users = match self.filesystem.read_directory(&users_root) {
            Ok(entries) => entries
                .into_iter()
                .filter(|entry| entry.is_directory && !is_wine_shared_user(&entry.file_name))
                .collect::<Vec<_>>(),
            Err(error) => {
                let availability = self.access_error_availability(error.kind);
                return Ok((
                    None,
                    PathResolution {
                        role: StudioPathRole::RobloxAppData,
                        availability,
                        value: None,
                        origin: ResolutionOrigin::Detected,
                        detail: Some(error.message),
                    },
                ));
            }
        };
        let with_roblox = users
            .iter()
            .filter(|user| self.is_detected(&user.path.join("AppData/Local/Roblox")))
            .collect::<Vec<_>>();
        let process_user = process.and_then(|process| {
            ["WINEUSERNAME", "USERNAME"]
                .into_iter()
                .find_map(|name| process.environment_value(name))
        });
        let selected = if let Some(process_user) = process_user {
            users.iter().find(|user| user.file_name.eq_ignore_ascii_case(process_user))
        } else if with_roblox.len() == 1 {
            with_roblox.first().copied()
        } else if with_roblox.is_empty() && users.len() == 1 {
            users.first()
        } else {
            None
        };
        let Some(user) = selected else {
            let detail = format!(
                "{} Wine user profiles remain after Roblox AppData correlation",
                if with_roblox.is_empty() { users.len() } else { with_roblox.len() }
            );
            diagnostics.push(diagnostic("WINE_USER_AMBIGUOUS", &detail));
            return Ok((
                None,
                PathResolution {
                    role: StudioPathRole::RobloxAppData,
                    availability: PathAvailability::Ambiguous,
                    value: None,
                    origin: ResolutionOrigin::Detected,
                    detail: Some(detail),
                },
            ));
        };
        let host = user.path.join("AppData/Local/Roblox");
        let guest = WinePath::from_parts(
            'C',
            vec![
                "users".to_owned(),
                user.file_name.clone(),
                "AppData".to_owned(),
                "Local".to_owned(),
                "Roblox".to_owned(),
            ],
        );
        let resolution = self.resolve_known_path(
            StudioPathRole::RobloxAppData,
            Some(host),
            ResolutionOrigin::Detected,
            Some(guest),
        );
        if resolution.availability == PathAvailability::Available {
            diagnostics.push(diagnostic(
                "WINE_USER_RESOLVED",
                format!("Wine profile '{}' owns the detected Roblox AppData", user.file_name),
            ));
        }
        Ok((available_resolved(&resolution), resolution))
    }

    fn resolve_deployment(
        &self,
        candidate: &StudioEnvironmentCandidate,
        process: Option<&ProcessSnapshot>,
    ) -> Result<(Option<StudioDeployment>, PathResolution), PlatformError> {
        if let Some(configured) = self.context.overrides.studio_deployment.as_ref() {
            self.validate_override("studio_deployment", configured)?;
            let deployment = make_deployment(configured.clone(), candidate.runtime);
            let resolution = self.resolve_known_path(
                StudioPathRole::Deployment,
                Some(configured.clone()),
                ResolutionOrigin::Configured,
                None,
            );
            return Ok((Some(deployment), resolution));
        }
        let candidates = match (candidate.host_platform, candidate.runtime) {
            (HostPlatform::MacOS, StudioRuntime::RobloxNative) => {
                let mut applications = vec![HostPath::new("/Applications/RobloxStudio.app")];
                if let Some(home) = self.context.home_dir.as_ref() {
                    applications.push(home.join("Applications/RobloxStudio.app"));
                }
                applications.into_iter().filter(|path| self.is_detected(path)).collect()
            }
            (_, StudioRuntime::RobloxNative) => candidate
                .data_root
                .as_ref()
                .map(|root| self.deployment_directories(&root.join("Versions")))
                .unwrap_or_default(),
            (_, StudioRuntime::VinegarNative | StudioRuntime::VinegarFlatpak) => candidate
                .data_root
                .as_ref()
                .map(|root| self.deployment_directories(&root.join("versions")))
                .unwrap_or_default(),
            (_, StudioRuntime::Unknown) => Vec::new(),
        };
        let selected = select_deployment(&self.filesystem, &candidates, process);
        match selected {
            DeploymentSelection::Selected(path) => {
                let deployment = make_deployment(path.clone(), candidate.runtime);
                let resolution = self.resolve_known_path(
                    StudioPathRole::Deployment,
                    Some(path),
                    ResolutionOrigin::Detected,
                    None,
                );
                Ok((Some(deployment), resolution))
            }
            DeploymentSelection::Ambiguous(count) => Ok((
                None,
                PathResolution {
                    role: StudioPathRole::Deployment,
                    availability: PathAvailability::Ambiguous,
                    value: None,
                    origin: ResolutionOrigin::Detected,
                    detail: Some(format!("{count} Studio deployments remain after correlation")),
                },
            )),
            DeploymentSelection::Missing(attempted) => Ok((
                None,
                self.resolve_known_path(
                    StudioPathRole::Deployment,
                    attempted,
                    ResolutionOrigin::Detected,
                    None,
                ),
            )),
        }
    }

    fn deployment_directories(&self, versions: &HostPath) -> Vec<HostPath> {
        self.filesystem.read_directory(versions).map_or_else(
            |_| Vec::new(),
            |entries| {
                entries
                    .into_iter()
                    .filter(|entry| entry.is_directory)
                    .map(|entry| entry.path)
                    .collect()
            },
        )
    }

    fn discover_drive_mappings(&self, prefix: &HostPath) -> BTreeMap<char, HostPath> {
        let mut mappings = BTreeMap::new();
        let drive_c = prefix.join("drive_c");
        if self.is_detected(&drive_c) {
            mappings.insert('C', drive_c);
        }
        let dos_devices = prefix.join("dosdevices");
        if let Ok(entries) = self.filesystem.read_directory(&dos_devices) {
            for entry in entries {
                let bytes = entry.file_name.as_bytes();
                if bytes.len() != 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
                    continue;
                }
                if let Ok(target) = self.filesystem.read_link(&entry.path) {
                    mappings.insert(char::from(bytes[0]).to_ascii_uppercase(), target);
                }
            }
        }
        mappings
    }

    fn related_processes(
        main_pid: u32,
        runtime: StudioRuntime,
        deployment: Option<StudioDeploymentId>,
        prefix: Option<&HostPath>,
        processes: &[ProcessSnapshot],
    ) -> Vec<StudioProcessInfo> {
        let parents = processes
            .iter()
            .map(|process| (process.pid, process.parent_pid))
            .collect::<BTreeMap<_, _>>();
        processes
            .iter()
            .filter(|process| process.pid != main_pid)
            .filter(|process| {
                let same_prefix = prefix.is_some_and(|prefix| {
                    process
                        .environment_value("WINEPREFIX")
                        .is_some_and(|value| HostPath::new(value) == *prefix)
                });
                same_prefix
                    || is_ancestor(process.pid, main_pid, &parents)
                    || is_ancestor(main_pid, process.pid, &parents)
            })
            .filter(|process| {
                prefix.is_none_or(|prefix| {
                    process
                        .environment_value("WINEPREFIX")
                        .is_some_and(|value| HostPath::new(value) == *prefix)
                        || process_mentions_path(process, prefix)
                        || is_ancestor(process.pid, main_pid, &parents)
                })
            })
            .map(|process| process_info(process, runtime, deployment))
            .collect()
    }

    fn resolve_mcp_launcher(
        &self,
        candidate: &StudioEnvironmentCandidate,
        deployment: Option<&StudioDeployment>,
        mappings: &BTreeMap<char, HostPath>,
        studio_prefix: Option<&HostPath>,
        wine_runtime: Option<&WineRuntime>,
    ) -> (Option<StudioMcpLauncher>, PathResolution) {
        if let Some(command) = self.context.studio_mcp_command.as_ref()
            && self.is_file(command)
        {
            return (
                Some(StudioMcpLauncher {
                    executable: command.clone(),
                    arguments: Vec::new(),
                    environment: BTreeMap::new(),
                    target_executable: None,
                    experimental: false,
                }),
                self.resolve_known_path(
                    StudioPathRole::McpServer,
                    Some(command.clone()),
                    ResolutionOrigin::Configured,
                    guest_for_host(command, mappings),
                ),
            );
        }
        let launcher = match candidate.host_platform {
            HostPlatform::Windows => candidate.data_root.as_ref().and_then(|root| {
                let launcher = root.join("mcp.bat");
                self.is_file(&launcher).then_some(StudioMcpLauncher {
                    executable: launcher,
                    arguments: Vec::new(),
                    environment: BTreeMap::new(),
                    target_executable: None,
                    experimental: false,
                })
            }),
            HostPlatform::MacOS => deployment.and_then(|deployment| {
                let launcher = deployment.path.join("Contents/MacOS/StudioMCP");
                self.is_file(&launcher).then_some(StudioMcpLauncher {
                    executable: launcher,
                    arguments: Vec::new(),
                    environment: BTreeMap::new(),
                    target_executable: None,
                    experimental: false,
                })
            }),
            HostPlatform::Linux | HostPlatform::Unknown => None,
        };
        if let Some(launcher) = launcher {
            let resolution = self.resolve_known_path(
                StudioPathRole::McpServer,
                Some(launcher.executable.clone()),
                ResolutionOrigin::Detected,
                None,
            );
            return (Some(launcher), resolution);
        }
        let binary = deployment.map(|deployment| deployment.path.join("StudioMCP.exe"));
        let guest = binary.as_ref().and_then(|path| guest_for_host(path, mappings));
        let resolution = self.resolve_known_path(
            StudioPathRole::McpServer,
            binary.clone(),
            ResolutionOrigin::Detected,
            guest.clone(),
        );
        if candidate.host_platform == HostPlatform::Linux
            && resolution.availability == PathAvailability::Available
            && let (Some(runtime), Some(prefix), Some(binary), Some(guest)) =
                (wine_runtime, studio_prefix, binary, guest)
        {
            return (
                Some(StudioMcpLauncher {
                    executable: runtime.executable.clone(),
                    arguments: vec![guest.to_string()],
                    environment: BTreeMap::from([("WINEPREFIX".into(), prefix.to_string())]),
                    target_executable: Some(ResolvedPath {
                        host: Some(binary),
                        guest: Some(guest),
                    }),
                    experimental: true,
                }),
                resolution,
            );
        }
        (None, resolution)
    }

    fn resolve_wine_runtime(
        &self,
        candidate: &StudioEnvironmentCandidate,
        studio_process: Option<&ProcessSnapshot>,
        processes: &[ProcessSnapshot],
    ) -> Option<WineRuntime> {
        if !matches!(
            candidate.runtime,
            StudioRuntime::VinegarNative | StudioRuntime::VinegarFlatpak
        ) {
            return None;
        }
        studio_process.into_iter().chain(processes.iter()).find_map(|process| {
            let executable = process.executable.as_ref()?;
            let path = executable.as_path();
            let file_name = path.file_name()?.to_string_lossy();
            let (root, runtime_candidate) = if file_name.eq_ignore_ascii_case("wine")
                || file_name.eq_ignore_ascii_case("wine64")
            {
                let bin = path.parent()?;
                (bin.parent()?, path.to_path_buf())
            } else if file_name.eq_ignore_ascii_case("wineserver") {
                let bin = path.parent()?;
                (bin.parent()?, bin.join("wine"))
            } else if file_name.eq_ignore_ascii_case("wine-preloader") {
                let library = path.ancestors().find(|ancestor| {
                    ancestor.file_name().is_some_and(|name| name.to_string_lossy() == "lib")
                })?;
                let root = library.parent()?;
                (root, root.join("bin/wine"))
            } else {
                return None;
            };
            let executable = HostPath::new(runtime_candidate);
            if self.is_file(&executable) {
                return Some(WineRuntime {
                    executable,
                    root: HostPath::new(root),
                    source_process_id: Some(process.pid),
                });
            }

            self.resolve_flatpak_wine_runtime(candidate, process)
        })
    }

    /// Resolves a Wine runtime whose executable is visible inside the
    /// Flatpak namespace but not at the same path from the host. Vinegar's
    /// packaged fork intentionally uses `/app/kombucha`; Lattice runs outside
    /// that namespace and must correlate it with the installed app files.
    fn resolve_flatpak_wine_runtime(
        &self,
        candidate: &StudioEnvironmentCandidate,
        process: &ProcessSnapshot,
    ) -> Option<WineRuntime> {
        if candidate.runtime != StudioRuntime::VinegarFlatpak {
            return None;
        }

        let process_path = process.executable.as_ref()?.as_path();
        let guest_root = HostPath::new("/app/kombucha");
        let relative_process_path = process_path.strip_prefix(guest_root.as_path()).ok()?;
        if relative_process_path.as_os_str().is_empty() {
            return None;
        }

        self.flatpak_wine_roots(candidate).into_iter().find_map(|root| {
            let process_executable = root.join(relative_process_path);
            let wine_executable = root.join("bin/wine");
            (self.is_file(&process_executable) && self.is_file(&wine_executable)).then_some(
                WineRuntime {
                    executable: wine_executable,
                    root,
                    source_process_id: Some(process.pid),
                },
            )
        })
    }

    fn flatpak_wine_roots(&self, candidate: &StudioEnvironmentCandidate) -> Vec<HostPath> {
        let mut roots = Vec::new();

        let mut flatpak_roots = Vec::new();
        if let Some(home) = self.context.home_dir.as_ref() {
            flatpak_roots.push(home.join(".local/share/flatpak"));
        }
        if let Some(data_local_dir) = self.context.data_local_dir.as_ref() {
            flatpak_roots.push(data_local_dir.join("flatpak"));
        }
        flatpak_roots.push(HostPath::new("/var/lib/flatpak"));

        for flatpak_root in flatpak_roots {
            let app_root = flatpak_root.join(format!("app/{VINEGAR_FLATPAK_ID}"));
            let Ok(architectures) = self.filesystem.read_directory(&app_root) else {
                continue;
            };
            for architecture in architectures.into_iter().filter(|entry| entry.is_directory) {
                let Ok(branches) = self.filesystem.read_directory(&architecture.path) else {
                    continue;
                };
                for branch in branches.into_iter().filter(|entry| entry.is_directory) {
                    if let Ok(active_commit) =
                        self.filesystem.read_link(&branch.path.join("active"))
                    {
                        let active_root = active_commit.join("files/kombucha");
                        if self.is_file(&active_root.join("bin/wine")) {
                            roots.push(active_root);
                        }
                    }
                    let Ok(commits) = self.filesystem.read_directory(&branch.path) else {
                        continue;
                    };
                    roots.extend(
                        commits
                            .into_iter()
                            .filter(|entry| entry.is_directory)
                            .map(|entry| entry.path.join("files/kombucha"))
                            .filter(|root| self.is_file(&root.join("bin/wine"))),
                    );
                }
            }
        }

        if let Some(data_root) = candidate.data_root.as_ref()
            && let Ok(entries) = self.filesystem.read_directory(data_root)
        {
            roots.extend(
                entries
                    .into_iter()
                    .filter(|entry| entry.is_directory && entry.file_name.starts_with("kombucha"))
                    .map(|entry| entry.path),
            );
        }

        let mut unique_roots = Vec::new();
        for root in roots {
            if !unique_roots.contains(&root) {
                unique_roots.push(root);
            }
        }
        roots = unique_roots;
        roots
    }

    fn resolve_known_path(
        &self,
        role: StudioPathRole,
        host: Option<HostPath>,
        origin: ResolutionOrigin,
        guest: Option<WinePath>,
    ) -> PathResolution {
        let Some(host) = host else {
            return PathResolution::unavailable(role, "no path is known for this runtime");
        };
        match self.filesystem.inspect(&host) {
            Ok(entry)
                if (role == StudioPathRole::McpServer && entry.is_file)
                    || (role != StudioPathRole::McpServer && entry.is_directory) =>
            {
                PathResolution {
                    role,
                    availability: PathAvailability::Available,
                    value: Some(ResolvedPath { host: Some(host), guest }),
                    origin,
                    detail: None,
                }
            }
            Ok(_) => PathResolution {
                role,
                availability: PathAvailability::Unavailable,
                value: Some(ResolvedPath { host: Some(host), guest }),
                origin,
                detail: Some(if role == StudioPathRole::McpServer {
                    "expected a file but found a different filesystem object".to_owned()
                } else {
                    "expected a directory but found a different filesystem object".to_owned()
                }),
            },
            Err(error) => PathResolution {
                role,
                availability: self.access_error_availability(error.kind),
                value: Some(ResolvedPath { host: Some(host), guest }),
                origin,
                detail: Some(error.message),
            },
        }
    }

    fn access_error_availability(&self, kind: FileSystemErrorKind) -> PathAvailability {
        match kind {
            FileSystemErrorKind::Missing => PathAvailability::Missing,
            FileSystemErrorKind::PermissionDenied if self.context.sandboxed => {
                PathAvailability::SandboxDenied
            }
            FileSystemErrorKind::PermissionDenied => PathAvailability::PermissionDenied,
            FileSystemErrorKind::Other => PathAvailability::Unavailable,
        }
    }

    fn validate_override(&self, name: &str, path: &HostPath) -> Result<(), PlatformError> {
        match self.filesystem.inspect(path) {
            Ok(entry) if entry.is_directory => Ok(()),
            Ok(_) => Err(PlatformError::InvalidOverride {
                name: name.to_owned(),
                reason: format!("{path} is not a directory"),
            }),
            Err(error) => {
                Err(PlatformError::InvalidOverride { name: name.to_owned(), reason: error.message })
            }
        }
    }

    fn validate_file_override(&self, name: &str, path: &HostPath) -> Result<(), PlatformError> {
        match self.filesystem.inspect(path) {
            Ok(entry) if entry.is_file => Ok(()),
            Ok(_) => Err(PlatformError::InvalidOverride {
                name: name.to_owned(),
                reason: format!("{path} is not a file"),
            }),
            Err(error) => {
                Err(PlatformError::InvalidOverride { name: name.to_owned(), reason: error.message })
            }
        }
    }

    fn any_detected<'a>(&self, paths: impl IntoIterator<Item = &'a HostPath>) -> bool {
        paths.into_iter().any(|path| self.is_detected(path))
    }

    fn first_detected(&self, paths: impl IntoIterator<Item = HostPath>) -> Option<HostPath> {
        let mut first = None;
        for path in paths {
            if first.is_none() {
                first = Some(path.clone());
            }
            if self.is_detected(&path) {
                return Some(path);
            }
        }
        first
    }

    fn is_detected(&self, path: &HostPath) -> bool {
        self.filesystem.inspect(path).is_ok_and(|entry| entry.is_directory)
    }

    fn is_file(&self, path: &HostPath) -> bool {
        self.filesystem.inspect(path).is_ok_and(|entry| entry.is_file)
    }
}

impl<F, P> StudioEnvironmentResolver for PlatformResolver<F, P>
where
    F: PlatformFileSystem,
    P: ProcessSource,
{
    fn detect_all(&self) -> Result<Vec<StudioEnvironmentCandidate>, PlatformError> {
        if let Some(command) = self.context.studio_mcp_command.as_ref() {
            self.validate_file_override("studio_mcp_command", command)?;
        }
        let processes = self.process_source.snapshot();
        match self.context.host_platform {
            HostPlatform::Linux => self.detect_linux(&processes),
            HostPlatform::Windows => Ok(self.detect_windows(&processes)),
            HostPlatform::MacOS => Ok(self.detect_macos(&processes)),
            HostPlatform::Unknown => Err(PlatformError::UnsupportedHost),
        }
    }

    fn resolve(
        &self,
        candidate: &StudioEnvironmentCandidate,
    ) -> Result<StudioEnvironment, PlatformError> {
        self.resolve_candidate(candidate, &self.process_source.snapshot())
    }
}

#[derive(Clone, Debug)]
struct CandidateRoots {
    runtime: StudioRuntime,
    config: HostPath,
    data: HostPath,
    cache: HostPath,
    origin: ResolutionOrigin,
}

enum DeploymentSelection {
    Selected(HostPath),
    Ambiguous(usize),
    Missing(Option<HostPath>),
}

fn select_deployment(
    filesystem: &dyn PlatformFileSystem,
    candidates: &[HostPath],
    process: Option<&ProcessSnapshot>,
) -> DeploymentSelection {
    if candidates.is_empty() {
        return DeploymentSelection::Missing(None);
    }
    if let Some(process) = process {
        let matched = candidates
            .iter()
            .filter(|candidate| process_mentions_path(process, candidate))
            .collect::<Vec<_>>();
        if matched.len() == 1 {
            return DeploymentSelection::Selected((*matched[0]).clone());
        }
    }
    if candidates.len() == 1 {
        return DeploymentSelection::Selected(candidates[0].clone());
    }
    let mut modified = candidates
        .iter()
        .filter_map(|path| filesystem.modified_unix_seconds(path).ok().map(|time| (time, path)))
        .collect::<Vec<_>>();
    modified.sort_by_key(|(time, _)| *time);
    if let [.., (previous, _), (latest, path)] = modified.as_slice()
        && latest > previous
    {
        return DeploymentSelection::Selected((*path).clone());
    }
    DeploymentSelection::Ambiguous(candidates.len())
}

fn make_deployment(path: HostPath, runtime: StudioRuntime) -> StudioDeployment {
    let build_identifier =
        path.as_path().file_name().map(|name| name.to_string_lossy().into_owned());
    let fingerprint = format!("{runtime:?}:{path}");
    StudioDeployment {
        id: StudioDeploymentId::from_fingerprint(fingerprint.as_bytes()),
        path,
        build_identifier,
        runtime,
    }
}

fn process_info(
    process: &ProcessSnapshot,
    runtime: StudioRuntime,
    deployment: Option<StudioDeploymentId>,
) -> StudioProcessInfo {
    StudioProcessInfo {
        pid: process.pid,
        executable: process.executable.clone(),
        parent_pid: process.parent_pid,
        runtime,
        deployment,
        start_time_unix_seconds: process.start_time_unix_seconds,
    }
}

fn is_studio_process(process: &ProcessSnapshot) -> bool {
    let name = process.name.to_ascii_lowercase();
    name.contains("robloxstudiobeta")
        || name == "robloxstudio"
        || process.command.first().is_some_and(|argument| {
            let executable = argument.replace('\\', "/").to_ascii_lowercase();
            executable.ends_with("/robloxstudiobeta.exe")
                || executable.ends_with("/robloxstudio")
                || executable.ends_with("/robloxstudio.app")
        })
}

fn process_mentions_path(process: &ProcessSnapshot, path: &HostPath) -> bool {
    process.executable.as_ref().is_some_and(|executable| executable.starts_with(path))
        || process.command.iter().any(|argument| HostPath::new(argument).starts_with(path))
}

fn process_mentions_resolved_path(
    process: &ProcessSnapshot,
    host: &HostPath,
    guest: Option<&WinePath>,
) -> bool {
    let host_text = host.to_string();
    let guest_text = guest.map(ToString::to_string);
    process.command.iter().any(|argument| {
        argument.contains(&host_text)
            || guest_text.as_ref().is_some_and(|guest| {
                argument.eq_ignore_ascii_case(guest)
                    || argument.to_ascii_lowercase().contains(&guest.to_ascii_lowercase())
            })
    })
}

fn is_ancestor(candidate: u32, process: u32, parents: &BTreeMap<u32, Option<u32>>) -> bool {
    let mut current = Some(process);
    for _ in 0..64 {
        let Some(pid) = current else {
            return false;
        };
        if pid == candidate {
            return true;
        }
        current = parents.get(&pid).copied().flatten();
    }
    false
}

fn is_wine_shared_user(name: &str) -> bool {
    ["Public", "Default", "Default User", "All Users"]
        .iter()
        .any(|shared| name.eq_ignore_ascii_case(shared))
}

fn guest_for_host(path: &HostPath, mappings: &BTreeMap<char, HostPath>) -> Option<WinePath> {
    mappings
        .iter()
        .filter_map(|(drive, root)| {
            path.strip_prefix(root).map(|relative| {
                let components = relative
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy().into_owned())
                    .collect();
                (root.as_path().components().count(), WinePath::from_parts(*drive, components))
            })
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, path)| path)
}

fn available_host(resolution: &PathResolution) -> Option<HostPath> {
    (resolution.availability == PathAvailability::Available)
        .then(|| resolution.value.as_ref()?.host.clone())
        .flatten()
}

fn available_resolved(resolution: &PathResolution) -> Option<ResolvedPath> {
    (resolution.availability == PathAvailability::Available)
        .then(|| resolution.value.clone())
        .flatten()
}

fn diagnostic(code: impl Into<String>, message: impl Into<String>) -> ResolutionDiagnostic {
    ResolutionDiagnostic { code: code.into(), message: message.into() }
}

fn environment_path(name: &str) -> Option<HostPath> {
    std::env::var_os(name).map(HostPath::new)
}
