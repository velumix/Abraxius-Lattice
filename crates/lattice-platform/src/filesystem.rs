use std::{fs, io, time::UNIX_EPOCH};

use crate::HostPath;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileSystemEntry {
    pub path: HostPath,
    pub file_name: String,
    pub is_directory: bool,
    pub is_file: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileSystemErrorKind {
    Missing,
    PermissionDenied,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileSystemError {
    pub kind: FileSystemErrorKind,
    pub message: String,
}

impl From<io::Error> for FileSystemError {
    fn from(error: io::Error) -> Self {
        let kind = match error.kind() {
            io::ErrorKind::NotFound => FileSystemErrorKind::Missing,
            io::ErrorKind::PermissionDenied => FileSystemErrorKind::PermissionDenied,
            _ => FileSystemErrorKind::Other,
        };
        Self { kind, message: error.to_string() }
    }
}

pub trait PlatformFileSystem: Send + Sync {
    fn inspect(&self, path: &HostPath) -> Result<FileSystemEntry, FileSystemError>;
    fn read_directory(&self, path: &HostPath) -> Result<Vec<FileSystemEntry>, FileSystemError>;
    fn read_link(&self, path: &HostPath) -> Result<HostPath, FileSystemError>;
    fn canonicalize(&self, path: &HostPath) -> Result<HostPath, FileSystemError>;
    fn modified_unix_seconds(&self, path: &HostPath) -> Result<u64, FileSystemError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RealFileSystem;

impl PlatformFileSystem for RealFileSystem {
    fn inspect(&self, path: &HostPath) -> Result<FileSystemEntry, FileSystemError> {
        let metadata = fs::metadata(path.as_path()).map_err(FileSystemError::from)?;
        Ok(FileSystemEntry {
            path: path.clone(),
            file_name: path
                .as_path()
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
            is_directory: metadata.is_dir(),
            is_file: metadata.is_file(),
        })
    }

    fn read_directory(&self, path: &HostPath) -> Result<Vec<FileSystemEntry>, FileSystemError> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(path.as_path()).map_err(FileSystemError::from)? {
            let entry = entry.map_err(FileSystemError::from)?;
            let file_type = entry.file_type().map_err(FileSystemError::from)?;
            entries.push(FileSystemEntry {
                path: HostPath::new(entry.path()),
                file_name: entry.file_name().to_string_lossy().into_owned(),
                is_directory: file_type.is_dir(),
                is_file: file_type.is_file(),
            });
        }
        entries.sort_by(|left, right| left.file_name.cmp(&right.file_name));
        Ok(entries)
    }

    fn read_link(&self, path: &HostPath) -> Result<HostPath, FileSystemError> {
        let target = fs::read_link(path.as_path()).map_err(FileSystemError::from)?;
        let resolved = if target.is_absolute() {
            target
        } else {
            path.as_path().parent().map_or(target.clone(), |parent| parent.join(target))
        };
        Ok(HostPath::new(resolved))
    }

    fn canonicalize(&self, path: &HostPath) -> Result<HostPath, FileSystemError> {
        fs::canonicalize(path.as_path()).map(HostPath::new).map_err(FileSystemError::from)
    }

    fn modified_unix_seconds(&self, path: &HostPath) -> Result<u64, FileSystemError> {
        fs::metadata(path.as_path())
            .and_then(|metadata| metadata.modified())
            .and_then(|modified| {
                modified
                    .duration_since(UNIX_EPOCH)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
            })
            .map(|duration| duration.as_secs())
            .map_err(FileSystemError::from)
    }
}
