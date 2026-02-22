use super::{Backend, download_all, upload_all};
use crate::preclude::BackendError;

pub async fn upload_all_from_backend(backend: &Backend) -> Result<(), BackendError> {
    let op = backend.get_op()?;
    upload_all(&op).await
}

pub async fn download_all_from_backend(backend: &Backend) -> Result<(), BackendError> {
    let op = backend.get_op()?;
    download_all(&op).await
}
