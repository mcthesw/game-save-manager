use std::io;
use std::path::Path;

use opendal::Operator;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};

use crate::preclude::*;

pub trait TransferHook: Send + Sync {
    fn on_upload_start(&self, _local_path: &Path, _remote_path: &str) {}

    fn on_upload_done(&self, _local_path: &Path, _remote_path: &str) {}

    fn on_download_start(&self, _remote_path: &str, _local_path: &Path) {}

    fn on_download_done(&self, _remote_path: &str, _local_path: &Path) {}
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopTransferHook;

impl TransferHook for NoopTransferHook {}

pub struct CloudTransfer<'a, H: TransferHook = NoopTransferHook> {
    op: &'a Operator,
    hook: H,
}

impl<'a> CloudTransfer<'a, NoopTransferHook> {
    pub fn new(op: &'a Operator) -> Self {
        Self::with_hook(op, NoopTransferHook)
    }
}

impl<'a, H: TransferHook> CloudTransfer<'a, H> {
    pub fn with_hook(op: &'a Operator, hook: H) -> Self {
        Self { op, hook }
    }

    pub async fn upload_file_streaming(
        &self,
        local_path: &Path,
        remote_path: &str,
    ) -> Result<(), BackendError> {
        self.hook.on_upload_start(local_path, remote_path);

        let mut source = fs::File::open(local_path).await?;
        let mut target = self
            .op
            .writer(remote_path)
            .await?
            .into_futures_async_write()
            .compat_write();
        tokio::io::copy(&mut source, &mut target).await?;
        target.shutdown().await?;

        self.hook.on_upload_done(local_path, remote_path);
        Ok(())
    }

    pub async fn download_file_streaming(
        &self,
        remote_path: &str,
        local_path: &Path,
    ) -> Result<(), BackendError> {
        self.hook.on_download_start(remote_path, local_path);

        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut source = self
            .op
            .reader(remote_path)
            .await?
            .into_futures_async_read(..)
            .await?
            .compat();
        let mut target = fs::File::create(local_path).await?;
        tokio::io::copy(&mut source, &mut target).await?;
        target.flush().await?;

        self.hook.on_download_done(remote_path, local_path);
        Ok(())
    }

    pub async fn upload_bytes_streaming(
        &self,
        data: &[u8],
        remote_path: &str,
    ) -> Result<(), BackendError> {
        let mut target = self
            .op
            .writer(remote_path)
            .await?
            .into_futures_async_write()
            .compat_write();
        target.write_all(data).await?;
        target.shutdown().await?;
        Ok(())
    }

    pub async fn download_bytes_streaming(&self, remote_path: &str) -> Result<Vec<u8>, BackendError> {
        let mut source = self
            .op
            .reader(remote_path)
            .await?
            .into_futures_async_read(..)
            .await?
            .compat();
        let mut data = Vec::new();
        source.read_to_end(&mut data).await?;
        Ok(data)
    }
}

pub fn path_to_remote_key(path: &Path) -> Result<String, BackendError> {
    let parts = path
        .iter()
        .map(|segment| {
            segment.to_str().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Path contains non-utf8 segment: {}", path.display()),
                )
            })
        })
        .collect::<Result<Vec<&str>, io::Error>>()?;
    Ok(parts.join("/"))
}
