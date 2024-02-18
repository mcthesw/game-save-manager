use std::{io, path::PathBuf, string::FromUtf8Error};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackupFileError {
    #[error("Cannot create file: {0:#?}")]
    CreateFileFailed(#[from] std::io::Error),
    #[error("File to backup not exists: {0:#?}")]
    NotExists(Vec<PathBuf>),
    #[error("Cannot write zip file: {0:#?}")]
    ZipError(#[from] zip::result::ZipError),
    #[error("Fs_extra error: {0:#?}")]
    FsError(#[from] fs_extra::error::Error),
    #[error("Cannot convert path to string")]
    NonePathError,
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("Backend is disabled")]
    Disabled,
    #[error("IO error: {0:#?}")]
    IoError(#[from] io::Error),
    #[error("Opendal error: {0:#?}")]
    CloudError(#[from] opendal::Error),
    #[error("Cannot read cloud file: {0:#?}")]
    ReadCloudInfoError(#[from] FromUtf8Error),
    #[error("Deserialize error: {0:#?}")]
    DeserializeError(#[from] serde_json::Error),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}
impl From<ConfigError> for BackendError {
    fn from(e: ConfigError) -> Self {
        match e {
            ConfigError::IoError(e) => Self::IoError(e),
            ConfigError::DeserializeError(e) => Self::DeserializeError(e),
            other => Self::Unexpected(other.into()),
        }
    }
}
impl From<BackupError> for BackendError {
    fn from(e: BackupError) -> Self {
        match e {
            BackupError::IoError(e) => Self::IoError(e),
            BackupError::DeserializeError(e) => Self::DeserializeError(e),
            other => Self::Unexpected(other.into()),
        }
    }
}

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("Backend error: {0:#?}")]
    BackendError(#[from] BackendError),
    #[error("Backup file error: {0:#?}")]
    BackupFileError(#[from] BackupFileError),
    #[error("Deserialize error: {0:#?}")]
    DeserializeError(#[from] serde_json::Error),
    #[error("Cannot convert path to string")]
    NonePathError,
    #[error("IO error: {0:#?}")]
    IoError(#[from] io::Error),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}
impl From<opendal::Error> for BackupError {
    fn from(e: opendal::Error) -> Self {
        Self::BackendError(BackendError::CloudError(e))
    }
}
impl From<ConfigError> for BackupError {
    fn from(e: ConfigError) -> Self {
        match e {
            ConfigError::IoError(e) => Self::IoError(e),
            ConfigError::DeserializeError(e) => Self::DeserializeError(e),
            other => Self::Unexpected(other.into()),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Deserialize error: {0:#?}")]
    DeserializeError(#[from] serde_json::Error),
    #[error("IO error: {0:#?}")]
    IoError(#[from] io::Error),
    #[error("Backend error: {0:#?}")]
    BackendError(#[from] BackendError)
}
