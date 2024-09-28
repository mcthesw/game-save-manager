use std::{io, path::PathBuf, string::FromUtf8Error};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackupFileError {
    #[error("Cannot create file: {0:#?}")]
    CreateFileFailed(#[from] std::io::Error),
    #[error("File to backup not exists: {0:#?}")]
    NotExists(PathBuf),
    #[error("Cannot write zip file: {0:#?}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Fs_extra error: {0:#?}")]
    Fs(#[from] fs_extra::error::Error),
    #[error("Cannot convert path to string")]
    NonePathError,
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// 压缩或解压缩时发生的错误
#[derive(Debug, Error)]
pub enum CompressError {
    #[error(transparent)]
    Single(#[from] BackupFileError),
    #[error("Multiple errors: {0:#?}")]
    Multiple(Vec<BackupFileError>),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("Backend is disabled")]
    Disabled,
    #[error("IO error: {0:#?}")]
    Io(#[from] io::Error),
    #[error("Opendal error: {0:#?}")]
    Cloud(#[from] opendal::Error),
    #[error("Cannot read cloud file: {0:#?}")]
    ReadCloudInfo(#[from] FromUtf8Error),
    #[error("Deserialize error: {0:#?}")]
    Deserialize(#[from] serde_json::Error),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}
impl From<ConfigError> for BackendError {
    fn from(e: ConfigError) -> Self {
        match e {
            ConfigError::Io(e) => Self::Io(e),
            ConfigError::Deserialize(e) => Self::Deserialize(e),
            other => Self::Unexpected(other.into()),
        }
    }
}
impl From<BackupError> for BackendError {
    fn from(e: BackupError) -> Self {
        match e {
            BackupError::Io(e) => Self::Io(e),
            BackupError::Deserialize(e) => Self::Deserialize(e),
            other => Self::Unexpected(other.into()),
        }
    }
}

/// 备份或恢复快照时可能产生的错误
#[derive(Debug, Error)]
pub enum BackupError {
    #[error("Backup for {name} not exists: {date}")]
    BackupNotExist { name: String, date: String },
    #[error("No backups available")]
    NoBackupAvailable,
    #[error("Backend error: {0:#?}")]
    Backend(#[from] BackendError),
    #[error("Compress/Decompress error: {0:#?}")]
    Compress(#[from] CompressError),
    #[error("Deserialize error: {0:#?}")]
    Deserialize(#[from] serde_json::Error),
    #[error("Cannot convert path to string")]
    NonePathError,
    #[error("IO error: {0:#?}")]
    Io(#[from] io::Error),
    #[error("Cannot create extra backup")]
    ExtraBackupFailed,
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}
impl From<opendal::Error> for BackupError {
    fn from(e: opendal::Error) -> Self {
        Self::Backend(BackendError::Cloud(e))
    }
}
impl From<ConfigError> for BackupError {
    fn from(e: ConfigError) -> Self {
        match e {
            ConfigError::Io(e) => Self::Io(e),
            ConfigError::Deserialize(e) => Self::Deserialize(e),
            other => Self::Unexpected(other.into()),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Deserialize error: {0:#?}")]
    Deserialize(#[from] serde_json::Error),
    #[error("IO error: {0:#?}")]
    Io(#[from] io::Error),
    #[error("Backend error: {0:#?}")]
    Backend(#[from] BackendError),
    #[error("Tauri error: {0:#?}")]
    Tauri(#[from] tauri::Error),
    #[error("Semver error: {0:#?}")]
    Semver(#[from] semver::Error),
}
