use std::{io, path::PathBuf};

use serde::{Deserialize, Serialize};
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


#[derive(Debug, Serialize, Deserialize)]
pub enum BackendError {
    /// 未选择后端
    Disabled,
    IoError(String),
    Unexpected(String),
}

impl From<io::Error> for BackendError {
    fn from(err: io::Error) -> Self {
        Self::IoError(err.to_string())
    }
    
}
impl From<opendal::Error> for BackendError {
    // TODO:完成更完善的错误处理
    fn from(err: opendal::Error) -> Self {
        Self::Unexpected(err.to_string())
    }
}