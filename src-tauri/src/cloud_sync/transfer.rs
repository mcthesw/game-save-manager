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

    pub async fn download_bytes_streaming(
        &self,
        remote_path: &str,
    ) -> Result<Vec<u8>, BackendError> {
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use opendal::Operator;
    use opendal::services;

    use super::*;

    fn build_memory_operator() -> Operator {
        Operator::new(services::Memory::default())
            .expect("memory backend should initialize")
            .finish()
    }

    #[test]
    fn test_path_to_remote_key() -> Result<(), BackendError> {
        let input = PathBuf::from("save_data")
            .join("My Game")
            .join("2026-01-01_00-00-00.zip");
        let remote = path_to_remote_key(&input)?;
        assert_eq!(remote, "save_data/My Game/2026-01-01_00-00-00.zip");
        Ok(())
    }

    #[tokio::test]
    async fn test_upload_file_streaming() -> Result<(), BackendError> {
        let op = build_memory_operator();
        let transfer = CloudTransfer::new(&op);

        let temp_dir = temp_dir::TempDir::new().expect("temp dir should be created");
        let local_path = temp_dir.path().join("snapshot.zip");
        let payload = vec![7_u8; 1024 * 1024 + 17];
        fs::write(&local_path, &payload).await?;

        transfer
            .upload_file_streaming(&local_path, "save_data/demo/2026.zip")
            .await?;

        let remote = op.read("save_data/demo/2026.zip").await?;
        assert_eq!(remote.to_vec(), payload);
        Ok(())
    }

    #[tokio::test]
    async fn test_download_file_streaming() -> Result<(), BackendError> {
        let op = build_memory_operator();
        let transfer = CloudTransfer::new(&op);
        let payload = b"download-streaming".to_vec();
        op.write("save_data/demo/2026.zip", payload.clone()).await?;

        let temp_dir = temp_dir::TempDir::new().expect("temp dir should be created");
        let local_path = temp_dir.path().join("nested").join("snapshot.zip");
        transfer
            .download_file_streaming("save_data/demo/2026.zip", &local_path)
            .await?;

        let local = fs::read(&local_path).await?;
        assert_eq!(local, payload);
        Ok(())
    }

    struct CounterHook {
        upload_start: AtomicUsize,
        upload_done: AtomicUsize,
        download_start: AtomicUsize,
        download_done: AtomicUsize,
    }

    impl Default for CounterHook {
        fn default() -> Self {
            Self {
                upload_start: AtomicUsize::new(0),
                upload_done: AtomicUsize::new(0),
                download_start: AtomicUsize::new(0),
                download_done: AtomicUsize::new(0),
            }
        }
    }

    impl TransferHook for CounterHook {
        fn on_upload_start(&self, _local_path: &Path, _remote_path: &str) {
            self.upload_start.fetch_add(1, Ordering::Relaxed);
        }

        fn on_upload_done(&self, _local_path: &Path, _remote_path: &str) {
            self.upload_done.fetch_add(1, Ordering::Relaxed);
        }

        fn on_download_start(&self, _remote_path: &str, _local_path: &Path) {
            self.download_start.fetch_add(1, Ordering::Relaxed);
        }

        fn on_download_done(&self, _remote_path: &str, _local_path: &Path) {
            self.download_done.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[tokio::test]
    async fn test_transfer_hook_callbacks() -> Result<(), BackendError> {
        let op = build_memory_operator();
        let hook = CounterHook::default();
        let transfer = CloudTransfer::with_hook(&op, hook);

        let temp_dir = temp_dir::TempDir::new().expect("temp dir should be created");
        let upload_path = temp_dir.path().join("upload.zip");
        fs::write(&upload_path, b"hook-test").await?;

        transfer
            .upload_file_streaming(&upload_path, "save_data/demo/hook.zip")
            .await?;
        transfer
            .download_file_streaming(
                "save_data/demo/hook.zip",
                &temp_dir.path().join("download.zip"),
            )
            .await?;

        assert_eq!(transfer.hook.upload_start.load(Ordering::Relaxed), 1);
        assert_eq!(transfer.hook.upload_done.load(Ordering::Relaxed), 1);
        assert_eq!(transfer.hook.download_start.load(Ordering::Relaxed), 1);
        assert_eq!(transfer.hook.download_done.load(Ordering::Relaxed), 1);
        Ok(())
    }
}
