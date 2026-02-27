use super::{Backend, download_all, upload_all};
use crate::config::get_config;
use crate::preclude::BackendError;

pub async fn upload_all_from_backend(backend: &Backend) -> Result<(), BackendError> {
    let op = backend.get_op()?;
    let max_concurrency = get_config()
        .map(|c| c.settings.cloud_settings.max_concurrency)
        .unwrap_or(1);
    upload_all(&op, max_concurrency).await
}

pub async fn download_all_from_backend(backend: &Backend) -> Result<(), BackendError> {
    let op = backend.get_op()?;
    let max_concurrency = get_config()
        .map(|c| c.settings.cloud_settings.max_concurrency)
        .unwrap_or(1);
    download_all(&op, max_concurrency).await
}
