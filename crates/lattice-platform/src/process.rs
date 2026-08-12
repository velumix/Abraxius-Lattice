use std::collections::BTreeMap;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::HostPath;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessSnapshot {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub executable: Option<HostPath>,
    pub command: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub start_time_unix_seconds: u64,
}

impl ProcessSnapshot {
    #[must_use]
    pub fn environment_value(&self, name: &str) -> Option<&str> {
        self.environment.get(name).map(String::as_str)
    }
}

pub trait ProcessSource: Send + Sync {
    fn snapshot(&self) -> Vec<ProcessSnapshot>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SysinfoProcessSource;

impl ProcessSource for SysinfoProcessSource {
    fn snapshot(&self) -> Vec<ProcessSnapshot> {
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_exe(UpdateKind::Always)
                .with_cmd(UpdateKind::Always)
                .with_environ(UpdateKind::Always)
                .without_tasks(),
        );

        let mut snapshots = system
            .processes()
            .values()
            .map(|process| ProcessSnapshot {
                pid: process.pid().as_u32(),
                parent_pid: process.parent().map(sysinfo::Pid::as_u32),
                name: process.name().to_string_lossy().into_owned(),
                executable: process.exe().map(|path| HostPath::new(path.to_path_buf())),
                command: process.cmd().iter().map(os_to_string).collect(),
                environment: process.environ().iter().filter_map(parse_environment).collect(),
                start_time_unix_seconds: process.start_time(),
            })
            .collect::<Vec<_>>();
        snapshots.sort_by_key(|process| process.pid);
        snapshots
    }
}

fn os_to_string(value: &std::ffi::OsString) -> String {
    value.to_string_lossy().into_owned()
}

fn parse_environment(value: &std::ffi::OsString) -> Option<(String, String)> {
    let value = value.to_string_lossy();
    let (name, value) = value.split_once('=')?;
    ["WINEPREFIX", "WINEUSERNAME", "USERNAME", "FLATPAK_ID"]
        .contains(&name)
        .then(|| (name.to_owned(), value.to_owned()))
}
