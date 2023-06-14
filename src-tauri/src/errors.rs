use std::path::PathBuf;

use zip::result::ZipError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackupZipError {
    #[error("Cannot create file")]
    CreateFileFailed(#[from] std::io::Error),
    #[error("File to backup not exists")]
    NotExists(Vec<PathBuf>),
    #[error("Cannot write zip file")]
    ZipError(#[from] ZipError),
    #[error("Unknown error")]
    Others(#[from] anyhow::Error),
}