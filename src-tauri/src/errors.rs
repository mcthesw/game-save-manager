use std::{io, path::PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackupFileError {
    #[error("Cannot create file: {0:#?}")]
    CreateFileFailed(#[from] std::io::Error),
    #[error("File to backup not exists: {0:#?}")]
    NotExists(Vec<PathBuf>),
    #[error("Cannot write zip file: {0:#?}")]
    ZipError(#[from] zip::result::ZipError),
    #[error(transparent)]
    Others(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("Backend is disabled")]
    Disabled,
    #[error("IO error: {0:#?}")]
    IoError(#[from] io::Error),
    #[error("Opendal error: {0:#?}")]
    CloudError(#[from] opendal::Error),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("Backend error: {0:#?}")]
    BackendError(#[from] BackendError),
    #[error("Backup file error: {0:#?}")]
    BackupFileError(#[from] BackupFileError),
    #[error("Deserialize error: {0:#?}")]
    DeserializeError(#[from] serde_json::Error),
    #[error("IO error: {0:#?}")]
    IoError(#[from] io::Error),
    #[error(transparent)]
    Others(#[from] anyhow::Error),
}

impl From<opendal::Error> for BackupError {
    fn from(e: opendal::Error) -> Self {
        Self::BackendError(BackendError::CloudError(e))
    }
}