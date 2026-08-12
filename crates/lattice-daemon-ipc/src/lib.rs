//! Local, process-scoped IPC discovery for the authoritative Lattice daemon.
//!
//! The endpoint is deliberately discovered from a user runtime directory and
//! contains a dynamically allocated loopback transport on platforms without a
//! Unix-domain socket. Consumers never embed a socket path or port.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::net::TcpStream;

#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

const ENDPOINT_FILE: &str = "daemon.endpoint.json";
const SOCKET_FILE: &str = "daemon.sock";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DaemonEndpointInfo {
    pub transport: String,
    pub address: String,
    pub pid: u32,
}

#[derive(Debug, Error)]
pub enum DaemonIpcError {
    #[error("DAEMON_NOT_RUNNING: no Lattice daemon endpoint is registered")]
    NotRunning,
    #[error("DAEMON_ALREADY_RUNNING: an authoritative Lattice daemon is already reachable")]
    AlreadyRunning,
    #[error("DAEMON_ENDPOINT_INVALID: {0}")]
    InvalidEndpoint(String),
    #[error("DAEMON_IPC_IO: {0}")]
    Io(#[from] io::Error),
    #[error("DAEMON_ENDPOINT_JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(unix)]
pub type DaemonStream = UnixStream;
#[cfg(not(unix))]
pub type DaemonStream = TcpStream;

#[cfg(unix)]
pub struct DaemonListener {
    listener: UnixListener,
    socket_path: PathBuf,
    endpoint_path: PathBuf,
}

#[cfg(not(unix))]
pub struct DaemonListener {
    listener: tokio::net::TcpListener,
    endpoint_path: PathBuf,
}

impl DaemonListener {
    /// Binds the single daemon endpoint. A reachable existing endpoint is
    /// never overwritten, which prevents a second daemon from becoming the
    /// authority accidentally.
    pub async fn bind() -> Result<Self, DaemonIpcError> {
        let paths = endpoint_paths()?;
        if endpoint_is_reachable(&paths.endpoint).await {
            return Err(DaemonIpcError::AlreadyRunning);
        }

        #[cfg(unix)]
        {
            if paths.socket.exists() {
                fs::remove_file(&paths.socket)?;
            }
            let listener = UnixListener::bind(&paths.socket)?;
            restrict_socket_permissions(&paths.socket)?;
            write_endpoint(
                &paths.endpoint,
                &DaemonEndpointInfo {
                    transport: "unix".to_owned(),
                    address: paths.socket.to_string_lossy().into_owned(),
                    pid: std::process::id(),
                },
            )?;
            Ok(Self { listener, socket_path: paths.socket, endpoint_path: paths.endpoint })
        }

        #[cfg(not(unix))]
        {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
            let address = listener.local_addr()?.to_string();
            write_endpoint(
                &paths.endpoint,
                &DaemonEndpointInfo {
                    transport: "loopback_tcp".to_owned(),
                    address,
                    pid: std::process::id(),
                },
            )?;
            Ok(Self { listener, endpoint_path: paths.endpoint })
        }
    }

    pub async fn accept(&self) -> Result<DaemonStream, DaemonIpcError> {
        #[cfg(unix)]
        {
            Ok(self.listener.accept().await?.0)
        }
        #[cfg(not(unix))]
        {
            Ok(self.listener.accept().await?.0)
        }
    }

    pub fn endpoint_info(&self) -> Result<DaemonEndpointInfo, DaemonIpcError> {
        read_endpoint(&self.endpoint_path)
    }
}

impl Drop for DaemonListener {
    fn drop(&mut self) {
        #[cfg(unix)]
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_file(&self.endpoint_path);
    }
}

pub async fn connect() -> Result<DaemonStream, DaemonIpcError> {
    let paths = endpoint_paths()?;
    let info = read_endpoint(&paths.endpoint)?;
    match info.transport.as_str() {
        #[cfg(unix)]
        "unix" => Ok(UnixStream::connect(info.address).await?),
        #[cfg(not(unix))]
        "loopback_tcp" => Ok(TcpStream::connect(info.address).await?),
        other => Err(DaemonIpcError::InvalidEndpoint(format!("unsupported transport {other:?}"))),
    }
}

pub fn inspect() -> Result<Option<DaemonEndpointInfo>, DaemonIpcError> {
    let paths = endpoint_paths()?;
    if !paths.endpoint.exists() {
        return Ok(None);
    }
    Ok(Some(read_endpoint(&paths.endpoint)?))
}

async fn endpoint_is_reachable(endpoint: &Path) -> bool {
    let Ok(info) = read_endpoint(endpoint) else {
        return false;
    };
    match info.transport.as_str() {
        #[cfg(unix)]
        "unix" => {
            tokio::time::timeout(Duration::from_millis(250), UnixStream::connect(info.address))
                .await
                .is_ok_and(|result| result.is_ok())
        }
        "loopback_tcp" => {
            tokio::time::timeout(Duration::from_millis(250), TcpStream::connect(info.address))
                .await
                .is_ok_and(|result| result.is_ok())
        }
        _ => false,
    }
}

struct EndpointPaths {
    endpoint: PathBuf,
    #[cfg(unix)]
    socket: PathBuf,
}

fn endpoint_paths() -> Result<EndpointPaths, DaemonIpcError> {
    let runtime = std::env::var_os("LATTICE_RUNTIME_DIR")
        .or_else(|| std::env::var_os("XDG_RUNTIME_DIR"))
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from))
        })
        .unwrap_or_else(|| std::env::temp_dir().join("lattice-runtime"));
    let root = runtime.join("lattice");
    fs::create_dir_all(&root)?;
    Ok(EndpointPaths {
        endpoint: root.join(ENDPOINT_FILE),
        #[cfg(unix)]
        socket: root.join(SOCKET_FILE),
    })
}

fn read_endpoint(path: &Path) -> Result<DaemonEndpointInfo, DaemonIpcError> {
    let bytes = fs::read(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            DaemonIpcError::NotRunning
        } else {
            DaemonIpcError::Io(error)
        }
    })?;
    let info: DaemonEndpointInfo = serde_json::from_slice(&bytes)?;
    if info.address.trim().is_empty() || info.transport.trim().is_empty() {
        return Err(DaemonIpcError::InvalidEndpoint("missing transport/address".to_owned()));
    }
    Ok(info)
}

fn write_endpoint(path: &Path, info: &DaemonEndpointInfo) -> Result<(), DaemonIpcError> {
    let temporary = path.with_extension("tmp");
    let bytes = serde_json::to_vec(info)?;
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)?;
    #[cfg(unix)]
    restrict_socket_permissions(path)?;
    Ok(())
}

#[cfg(unix)]
fn restrict_socket_permissions(path: &Path) -> Result<(), DaemonIpcError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}
