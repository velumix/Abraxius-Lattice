use std::{collections::BTreeMap, fs};

use lattice_platform::{
    FileSystemEntry, FileSystemError, FileSystemErrorKind, HostPath, HostPlatform,
    InspectionStatus, PathAvailability, PathError, PathTranslator, PlatformContext,
    PlatformFileSystem, PlatformResolver, ProcessSnapshot, ProcessSource, RealFileSystem,
    ResolutionOrigin, ResolverOverrides, StudioEnvironmentResolver, StudioPathRole, StudioRuntime,
    WinePath, WinePathTranslator,
};
use tempfile::TempDir;

#[derive(Clone, Debug, Default)]
struct StaticProcesses(Vec<ProcessSnapshot>);

impl ProcessSource for StaticProcesses {
    fn snapshot(&self) -> Vec<ProcessSnapshot> {
        self.0.clone()
    }
}

fn context(host_platform: HostPlatform, home: &HostPath) -> PlatformContext {
    PlatformContext {
        host_platform,
        home_dir: Some(home.clone()),
        config_dir: Some(home.join(".config")),
        data_local_dir: Some(home.join(".local/share")),
        cache_dir: Some(home.join(".cache")),
        sandboxed: false,
        studio_mcp_command: None,
        overrides: ResolverOverrides::default(),
    }
}

fn directory(path: &HostPath) -> Result<(), std::io::Error> {
    fs::create_dir_all(path.as_path())
}

fn file(path: &HostPath) -> Result<(), std::io::Error> {
    if let Some(parent) = path.as_path().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path.as_path(), [])
}

fn native_vinegar_fixture(home: &HostPath) -> Result<(HostPath, HostPath), std::io::Error> {
    let data = home.join(".local/share/vinegar");
    let prefix = data.join("prefixes/studio");
    directory(&home.join(".config/vinegar"))?;
    directory(&home.join(".cache/vinegar/logs"))?;
    directory(&prefix.join("drive_c/users/player/AppData/Local/Roblox/Profiler"))?;
    directory(&prefix.join("dosdevices"))?;
    file(&data.join("versions/version-a/RobloxStudioBeta.exe"))?;
    Ok((data, prefix))
}

fn flatpak_vinegar_fixture(home: &HostPath) -> Result<(HostPath, HostPath), std::io::Error> {
    let root = home.join(".var/app/org.vinegarhq.Vinegar");
    let data = root.join("data/vinegar");
    let prefix = data.join("prefixes/studio");
    directory(&root.join("config/vinegar"))?;
    directory(&root.join("cache/vinegar/logs"))?;
    directory(&prefix.join("drive_c/users/flatpak/AppData/Local/Roblox"))?;
    directory(&prefix.join("dosdevices"))?;
    file(&data.join("versions/version-flatpak/RobloxStudioBeta.exe"))?;
    Ok((data, prefix))
}

fn studio_process(pid: u32, prefix: &HostPath, deployment: &HostPath) -> ProcessSnapshot {
    ProcessSnapshot {
        pid,
        parent_pid: Some(pid.saturating_sub(1)),
        name: "RobloxStudioBeta.exe".to_owned(),
        executable: Some(HostPath::new("/usr/bin/wine64")),
        command: vec![deployment.join("RobloxStudioBeta.exe").to_string()],
        environment: BTreeMap::from([
            ("WINEPREFIX".to_owned(), prefix.to_string()),
            ("WINEUSERNAME".to_owned(), "player".to_owned()),
        ]),
        start_time_unix_seconds: 1_000 + u64::from(pid),
    }
}

#[test]
fn linux_native_default_xdg_resolves_all_core_paths() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let (data, prefix) = native_vinegar_fixture(&home)?;
    let deployment = data.join("versions/version-a");
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        RealFileSystem,
        StaticProcesses(vec![studio_process(41, &prefix, &deployment)]),
    );
    let inspection = resolver.inspect()?;
    assert_eq!(inspection.status, InspectionStatus::Resolved);
    let environment = &inspection.environments[0];
    assert_eq!(environment.runtime, StudioRuntime::VinegarNative);
    assert_eq!(environment.studio_prefix.as_ref(), Some(&prefix));
    assert_eq!(environment.studio_process.as_ref().map(|process| process.pid), Some(41));
    assert_eq!(
        environment
            .roblox_appdata
            .as_ref()
            .and_then(|path| path.guest.as_ref())
            .map(ToString::to_string),
        Some(r"C:\users\player\AppData\Local\Roblox".to_owned())
    );
    assert!(environment.capabilities.wine_path_translation);
    Ok(())
}

#[test]
fn custom_xdg_roots_are_used_independently() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path().join("home"));
    let config = HostPath::new(temporary.path().join("custom-config"));
    let data = HostPath::new(temporary.path().join("custom-data"));
    let cache = HostPath::new(temporary.path().join("custom-cache"));
    directory(&config.join("vinegar"))?;
    directory(&data.join("vinegar"))?;
    directory(&cache.join("vinegar"))?;
    let mut custom = context(HostPlatform::Linux, &home);
    custom.config_dir = Some(config);
    custom.data_local_dir = Some(data.clone());
    custom.cache_dir = Some(cache);
    let resolver = PlatformResolver::new(custom, RealFileSystem, StaticProcesses::default());
    let candidates = resolver.detect_all()?;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].data_root, Some(data.join("vinegar")));
    Ok(())
}

#[test]
fn custom_xdg_config_home_does_not_change_data_or_cache() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path().join("home"));
    let custom = HostPath::new(temporary.path().join("config-only"));
    directory(&custom.join("vinegar"))?;
    let mut configured = context(HostPlatform::Linux, &home);
    configured.config_dir = Some(custom.clone());
    let candidate = PlatformResolver::new(configured, RealFileSystem, StaticProcesses::default())
        .detect_all()?
        .remove(0);
    assert_eq!(candidate.config_root, Some(custom.join("vinegar")));
    assert_eq!(candidate.data_root, Some(home.join(".local/share/vinegar")));
    assert_eq!(candidate.cache_root, Some(home.join(".cache/vinegar")));
    Ok(())
}

#[test]
fn custom_xdg_data_home_does_not_change_config_or_cache() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path().join("home"));
    let custom = HostPath::new(temporary.path().join("data-only"));
    directory(&custom.join("vinegar"))?;
    let mut configured = context(HostPlatform::Linux, &home);
    configured.data_local_dir = Some(custom.clone());
    let candidate = PlatformResolver::new(configured, RealFileSystem, StaticProcesses::default())
        .detect_all()?
        .remove(0);
    assert_eq!(candidate.config_root, Some(home.join(".config/vinegar")));
    assert_eq!(candidate.data_root, Some(custom.join("vinegar")));
    assert_eq!(candidate.cache_root, Some(home.join(".cache/vinegar")));
    Ok(())
}

#[test]
fn custom_xdg_cache_home_does_not_change_config_or_data() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path().join("home"));
    let custom = HostPath::new(temporary.path().join("cache-only"));
    directory(&custom.join("vinegar"))?;
    let mut configured = context(HostPlatform::Linux, &home);
    configured.cache_dir = Some(custom.clone());
    let candidate = PlatformResolver::new(configured, RealFileSystem, StaticProcesses::default())
        .detect_all()?
        .remove(0);
    assert_eq!(candidate.config_root, Some(home.join(".config/vinegar")));
    assert_eq!(candidate.data_root, Some(home.join(".local/share/vinegar")));
    assert_eq!(candidate.cache_root, Some(custom.join("vinegar")));
    Ok(())
}

#[test]
fn flatpak_is_a_distinct_first_class_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let _ = flatpak_vinegar_fixture(&home)?;
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        RealFileSystem,
        StaticProcesses::default(),
    );
    let inspection = resolver.inspect()?;
    assert_eq!(inspection.status, InspectionStatus::Resolved);
    assert_eq!(inspection.environments[0].runtime, StudioRuntime::VinegarFlatpak);
    Ok(())
}

#[cfg(unix)]
#[test]
fn flatpak_wine_runtime_produces_an_experimental_stdio_launcher_from_resolved_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let (data, prefix) = flatpak_vinegar_fixture(&home)?;
    let deployment = data.join("versions/version-flatpak");
    file(&deployment.join("StudioMCP.exe"))?;
    std::os::unix::fs::symlink(temporary.path(), prefix.join("dosdevices/z:").as_path())?;
    let runtime = data.join("kombucha-test");
    file(&runtime.join("bin/wine"))?;
    file(&runtime.join("lib/wine/x86_64-unix/wine-preloader"))?;
    let mut process = studio_process(51, &prefix, &deployment);
    process.executable = Some(runtime.join("lib/wine/x86_64-unix/wine-preloader"));
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        RealFileSystem,
        StaticProcesses(vec![process]),
    );
    let inspection = resolver.inspect()?;
    let environment = &inspection.environments[0];
    let wine = environment.wine_runtime.as_ref().ok_or("Wine runtime not resolved")?;
    assert_eq!(wine.executable, runtime.join("bin/wine"));
    let launcher = environment.mcp_launcher.as_ref().ok_or("MCP launcher not resolved")?;
    assert_eq!(launcher.executable, wine.executable);
    assert!(launcher.experimental);
    assert_eq!(launcher.environment.get("WINEPREFIX"), Some(&prefix.to_string()));
    assert_eq!(launcher.arguments.len(), 1);
    assert!(launcher.arguments[0].ends_with("StudioMCP.exe"));
    Ok(())
}

#[test]
fn native_and_flatpak_remnants_are_ambiguous_without_process_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let _ = native_vinegar_fixture(&home)?;
    let _ = flatpak_vinegar_fixture(&home)?;
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        RealFileSystem,
        StaticProcesses::default(),
    );
    assert_eq!(resolver.inspect()?.status, InspectionStatus::Ambiguous);
    Ok(())
}

#[test]
fn running_process_disambiguates_native_from_flatpak() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let (native_data, native_prefix) = native_vinegar_fixture(&home)?;
    let _ = flatpak_vinegar_fixture(&home)?;
    let process = studio_process(52, &native_prefix, &native_data.join("versions/version-a"));
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        RealFileSystem,
        StaticProcesses(vec![process]),
    );
    let inspection = resolver.inspect()?;
    assert_eq!(inspection.status, InspectionStatus::Resolved);
    let selected = inspection.selected_environment;
    assert!(inspection.environments.iter().any(|environment| Some(environment.id) == selected
        && environment.runtime == StudioRuntime::VinegarNative));
    Ok(())
}

#[test]
fn missing_prefix_is_explicit_not_found_state() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    directory(&home.join(".local/share/vinegar"))?;
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        RealFileSystem,
        StaticProcesses::default(),
    );
    let inspection = resolver.inspect()?;
    let prefix = inspection.environments[0].path(StudioPathRole::Prefix);
    assert_eq!(prefix.map(|path| path.availability), Some(PathAvailability::Missing));
    assert!(inspection.environments[0].studio_prefix.is_none());
    Ok(())
}

#[test]
fn multiple_wine_users_return_ambiguity() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let (_, prefix) = native_vinegar_fixture(&home)?;
    directory(&prefix.join("drive_c/users/second/AppData/Local/Roblox"))?;
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        RealFileSystem,
        StaticProcesses::default(),
    );
    let inspection = resolver.inspect()?;
    assert_eq!(
        inspection.environments[0]
            .path(StudioPathRole::RobloxAppData)
            .map(|path| path.availability),
        Some(PathAvailability::Ambiguous)
    );
    Ok(())
}

#[test]
fn multiple_deployments_without_distinguishing_evidence_are_ambiguous()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let (data, _) = native_vinegar_fixture(&home)?;
    file(&data.join("versions/version-b/RobloxStudioBeta.exe"))?;
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        RealFileSystem,
        StaticProcesses::default(),
    );
    let inspection = resolver.inspect()?;
    assert_eq!(
        inspection.environments[0].path(StudioPathRole::Deployment).map(|path| path.availability),
        Some(PathAvailability::Ambiguous)
    );
    Ok(())
}

#[test]
fn multiple_studio_processes_receive_distinct_environment_ids()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let (data, prefix) = native_vinegar_fixture(&home)?;
    let deployment = data.join("versions/version-a");
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        RealFileSystem,
        StaticProcesses(vec![
            studio_process(61, &prefix, &deployment),
            studio_process(62, &prefix, &deployment),
        ]),
    );
    let inspection = resolver.inspect()?;
    assert_eq!(inspection.status, InspectionStatus::Ambiguous);
    assert_eq!(inspection.environments.len(), 2);
    assert_ne!(inspection.environments[0].id, inspection.environments[1].id);
    Ok(())
}

#[test]
fn webview_helpers_are_related_but_never_promoted_to_studio_sessions()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let (data, prefix) = native_vinegar_fixture(&home)?;
    let deployment = data.join("versions/version-a");
    let main = studio_process(71, &prefix, &deployment);
    let helper = ProcessSnapshot {
        pid: 72,
        parent_pid: Some(70),
        name: "CrRendererMain".to_owned(),
        executable: Some(HostPath::new("/usr/bin/wine-preloader")),
        command: vec![
            r"C:\Program Files\WebView2\msedgewebview2.exe".to_owned(),
            "--webview-exe-name=RobloxStudioBeta.exe".to_owned(),
        ],
        environment: BTreeMap::from([("WINEPREFIX".to_owned(), prefix.to_string())]),
        start_time_unix_seconds: 2_000,
    };
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        RealFileSystem,
        StaticProcesses(vec![main, helper]),
    );
    let inspection = resolver.inspect()?;
    assert_eq!(inspection.environments.len(), 1);
    assert_eq!(inspection.environments[0].studio_process.as_ref().map(|value| value.pid), Some(71));
    assert!(inspection.environments[0].related_processes.iter().any(|value| value.pid == 72));
    Ok(())
}

#[test]
fn windows_native_fixture_uses_semantic_roots() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path().join("profile"));
    let local = HostPath::new(temporary.path().join("LocalAppData"));
    directory(&local.join("Roblox/logs"))?;
    file(&local.join("Roblox/Versions/version-win/RobloxStudioBeta.exe"))?;
    let mut windows = context(HostPlatform::Windows, &home);
    windows.data_local_dir = Some(local.clone());
    let resolver = PlatformResolver::new(windows, RealFileSystem, StaticProcesses::default());
    let environment = &resolver.inspect()?.environments[0];
    assert_eq!(environment.runtime, StudioRuntime::RobloxNative);
    assert_eq!(
        environment.roblox_appdata.as_ref().and_then(|path| path.host.as_ref()),
        Some(&local.join("Roblox"))
    );
    Ok(())
}

#[test]
fn macos_native_fixture_uses_mac_semantics() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    directory(&home.join("Applications/RobloxStudio.app"))?;
    directory(&home.join("Library/Roblox"))?;
    directory(&home.join("Library/Logs/Roblox"))?;
    let resolver = PlatformResolver::new(
        context(HostPlatform::MacOS, &home),
        RealFileSystem,
        StaticProcesses::default(),
    );
    let environment = &resolver.inspect()?.environments[0];
    assert_eq!(environment.host_platform, HostPlatform::MacOS);
    assert_eq!(
        environment.path(StudioPathRole::Logs).map(|path| path.availability),
        Some(PathAvailability::Available)
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn wine_c_and_z_translation_round_trip_and_reject_symlink_escape()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;
    use std::str::FromStr;

    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let (_, prefix) = native_vinegar_fixture(&home)?;
    let project = home.join("projects/game");
    directory(&project)?;
    symlink(home.as_path(), prefix.join("dosdevices/z:").as_path())?;
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        RealFileSystem,
        StaticProcesses::default(),
    );
    let environment = &resolver.inspect()?.environments[0];
    let translator = WinePathTranslator::new(&RealFileSystem);
    let guest = WinePath::from_str(r"Z:\projects\game")?;
    assert_eq!(translator.guest_to_host(&guest, environment)?, project);
    assert_eq!(translator.host_to_guest(&project, environment)?, guest);

    let outside = HostPath::new(temporary.path().join("outside"));
    directory(&outside)?;
    symlink(outside.as_path(), prefix.join("drive_c/users/escape").as_path())?;
    let escaping = WinePath::from_str(r"C:\users\escape\secret.txt")?;
    assert_eq!(translator.guest_to_host(&escaping, environment), Err(PathError::PrefixEscape));
    Ok(())
}

#[test]
fn missing_roblox_appdata_preserves_the_expected_host_and_guest_namespaces()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let data = home.join(".local/share/vinegar");
    directory(&home.join(".config/vinegar"))?;
    directory(&data.join("prefixes/studio/drive_c/users/only-user"))?;
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        RealFileSystem,
        StaticProcesses::default(),
    );
    let environment = &resolver.inspect()?.environments[0];
    let path = environment.path(StudioPathRole::RobloxAppData);
    assert_eq!(path.map(|value| value.availability), Some(PathAvailability::Missing));
    assert!(
        path.and_then(|value| value.value.as_ref())
            .is_some_and(|value| { value.host.is_some() && value.guest.is_some() })
    );
    Ok(())
}

#[derive(Clone)]
struct DeniedFileSystem {
    denied: HostPath,
}

impl PlatformFileSystem for DeniedFileSystem {
    fn inspect(&self, path: &HostPath) -> Result<FileSystemEntry, FileSystemError> {
        if path == &self.denied {
            return Err(FileSystemError {
                kind: FileSystemErrorKind::PermissionDenied,
                message: "fixture access denied".to_owned(),
            });
        }
        RealFileSystem.inspect(path)
    }

    fn read_directory(&self, path: &HostPath) -> Result<Vec<FileSystemEntry>, FileSystemError> {
        RealFileSystem.read_directory(path)
    }

    fn read_link(&self, path: &HostPath) -> Result<HostPath, FileSystemError> {
        RealFileSystem.read_link(path)
    }

    fn canonicalize(&self, path: &HostPath) -> Result<HostPath, FileSystemError> {
        RealFileSystem.canonicalize(path)
    }

    fn modified_unix_seconds(&self, path: &HostPath) -> Result<u64, FileSystemError> {
        RealFileSystem.modified_unix_seconds(path)
    }
}

#[test]
fn permission_denied_is_distinct_from_missing() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let data = home.join(".local/share/vinegar");
    directory(&home.join(".config/vinegar"))?;
    directory(&data)?;
    let resolver = PlatformResolver::new(
        context(HostPlatform::Linux, &home),
        DeniedFileSystem { denied: data },
        StaticProcesses::default(),
    );
    let environment = &resolver.inspect()?.environments[0];
    assert_eq!(
        environment.path(StudioPathRole::Data).map(|path| path.availability),
        Some(PathAvailability::PermissionDenied)
    );
    Ok(())
}

#[test]
fn sandbox_denied_is_distinct_from_host_permission_denied() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let data = home.join(".local/share/vinegar");
    directory(&home.join(".config/vinegar"))?;
    directory(&data)?;
    let mut sandboxed = context(HostPlatform::Linux, &home);
    sandboxed.sandboxed = true;
    let resolver = PlatformResolver::new(
        sandboxed,
        DeniedFileSystem { denied: data },
        StaticProcesses::default(),
    );
    let environment = &resolver.inspect()?.environments[0];
    assert_eq!(
        environment.path(StudioPathRole::Data).map(|path| path.availability),
        Some(PathAvailability::SandboxDenied)
    );
    Ok(())
}

#[test]
fn configured_invalid_override_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    directory(&home.join(".config/vinegar"))?;
    let mut configured = context(HostPlatform::Linux, &home);
    configured.overrides.vinegar_data_root = Some(home.join("does-not-exist"));
    let resolver = PlatformResolver::new(configured, RealFileSystem, StaticProcesses::default());
    assert!(resolver.detect_all().is_err());
    Ok(())
}

#[test]
fn invalid_mcp_command_override_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let mut configured = context(HostPlatform::Linux, &home);
    configured.studio_mcp_command = Some(home.join("missing-mcp-command"));
    let resolver = PlatformResolver::new(configured, RealFileSystem, StaticProcesses::default());
    assert!(resolver.detect_all().is_err());
    Ok(())
}

#[test]
fn configured_root_is_marked_configured() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let home = HostPath::new(temporary.path());
    let configured_data = home.join("vinegar-custom");
    directory(&configured_data)?;
    let mut configured = context(HostPlatform::Linux, &home);
    configured.overrides.vinegar_data_root = Some(configured_data.clone());
    let resolver = PlatformResolver::new(configured, RealFileSystem, StaticProcesses::default());
    let candidates = resolver.detect_all()?;
    assert_eq!(candidates[0].origin, ResolutionOrigin::Configured);
    assert_eq!(candidates[0].data_root, Some(configured_data));
    let environment = resolver.resolve(&candidates[0])?;
    assert_eq!(
        environment.path(StudioPathRole::Data).map(|path| path.origin),
        Some(ResolutionOrigin::Configured)
    );
    assert_eq!(
        environment.path(StudioPathRole::Config).map(|path| path.origin),
        Some(ResolutionOrigin::Detected)
    );
    Ok(())
}
