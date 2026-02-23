use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use opendal::Operator;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};

use crate::preclude::*;

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const TEMP_FILE_CREATE_RETRY_LIMIT: usize = 16;

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

/// Build a temp file path next to the destination so rename stays in the same directory.
fn build_scoped_temp_path(local_path: &Path, suffix: &str) -> Result<PathBuf, BackendError> {
    let parent = local_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Path has no parent: {}", local_path.display()),
        )
    })?;
    let file_name = local_path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Path has no file name: {}", local_path.display()),
        )
    })?;

    let seq = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(file_name);
    temp_name.push(format!(".{suffix}.{seq}.tmp"));
    Ok(parent.join(temp_name))
}

/// Create a temp file with `create_new(true)` and retry on rare name collisions.
async fn create_temp_file_with_retry(
    local_path: &Path,
    suffix: &str,
) -> Result<(PathBuf, fs::File), BackendError> {
    let mut last_already_exists_err = None;
    for _ in 0..TEMP_FILE_CREATE_RETRY_LIMIT {
        let temp_path = build_scoped_temp_path(local_path, suffix)?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .await
        {
            Ok(file) => return Ok((temp_path, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                last_already_exists_err = Some(err);
            }
            Err(err) => return Err(err.into()),
        }
    }

    Err(last_already_exists_err
        .unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "Failed to create a unique temp file for {} after {} attempts",
                    local_path.display(),
                    TEMP_FILE_CREATE_RETRY_LIMIT
                ),
            )
        })
        .into())
}

/// Replace destination with temp file while preserving the old file on failure.
///
/// On platforms where direct rename-overwrite can fail (for example Windows),
/// this falls back to: move old file to backup -> move temp to destination -> rollback if needed.
async fn replace_path_preserving_existing(
    temp_path: &Path,
    local_path: &Path,
) -> Result<(), BackendError> {
    if fs::rename(temp_path, local_path).await.is_ok() {
        return Ok(());
    }

    if !fs::try_exists(local_path).await? {
        fs::rename(temp_path, local_path).await?;
        return Ok(());
    }

    let backup_path = build_scoped_temp_path(local_path, "backup")?;
    fs::rename(local_path, &backup_path).await?;

    match fs::rename(temp_path, local_path).await {
        Ok(_) => {
            let _ = fs::remove_file(&backup_path).await;
            Ok(())
        }
        Err(err) => {
            let _ = fs::rename(&backup_path, local_path).await;
            let _ = fs::remove_file(temp_path).await;
            Err(err.into())
        }
    }
}

/// Write stream data into a temp file and commit via atomic-ish rename semantics.
///
/// If the stream fails midway, the destination file remains untouched.
async fn write_stream_atomically<R: tokio::io::AsyncRead + Unpin>(
    mut source: R,
    local_path: &Path,
) -> Result<(), BackendError> {
    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let (temp_path, mut target) = create_temp_file_with_retry(local_path, "download").await?;

    let copy_result = tokio::io::copy(&mut source, &mut target).await;
    if let Err(err) = copy_result {
        let _ = fs::remove_file(&temp_path).await;
        return Err(err.into());
    }

    target.flush().await?;
    target.sync_all().await?;
    drop(target);

    replace_path_preserving_existing(&temp_path, local_path).await
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

        let mut source = self
            .op
            .reader(remote_path)
            .await?
            .into_futures_async_read(..)
            .await?
            .compat();

        // Keep existing snapshot intact if download fails midway.
        write_stream_atomically(&mut source, local_path).await?;

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

    pub async fn write_local_bytes_atomically(
        &self,
        local_path: &Path,
        data: &[u8],
    ) -> Result<(), BackendError> {
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let (temp_path, mut target) = create_temp_file_with_retry(local_path, "bytes").await?;
        target.write_all(data).await?;
        target.flush().await?;
        target.sync_all().await?;
        drop(target);

        replace_path_preserving_existing(&temp_path, local_path).await
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
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll};

    use opendal::Operator;
    use opendal::services;
    use tokio::io::{AsyncRead, ReadBuf};

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

    struct FailAfterReader {
        data: Vec<u8>,
        pos: usize,
        fail_after: usize,
    }

    impl AsyncRead for FailAfterReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.pos >= self.fail_after {
                return Poll::Ready(Err(io::Error::other("injected read failure")));
            }

            if self.pos >= self.data.len() {
                return Poll::Ready(Ok(()));
            }

            let remaining_data = self.data.len() - self.pos;
            let remaining_before_fail = self.fail_after - self.pos;
            let to_copy = buf
                .remaining()
                .min(remaining_data)
                .min(remaining_before_fail);
            let start = self.pos;
            let end = start + to_copy;
            buf.put_slice(&self.data[start..end]);
            self.pos = end;
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn test_stream_download_keeps_existing_file_on_failure() -> Result<(), BackendError> {
        let temp_dir = temp_dir::TempDir::new().expect("temp dir should be created");
        let local_path = temp_dir.path().join("snapshot.zip");
        fs::write(&local_path, b"existing-valid-backup").await?;

        let reader = FailAfterReader {
            data: b"new-backup-content".to_vec(),
            pos: 0,
            fail_after: 5,
        };
        let result = write_stream_atomically(reader, &local_path).await;
        assert!(result.is_err());

        let persisted = fs::read(&local_path).await?;
        assert_eq!(persisted, b"existing-valid-backup");
        Ok(())
    }

    #[tokio::test]
    async fn test_write_local_bytes_atomically_replaces_existing_file() -> Result<(), BackendError>
    {
        let op = build_memory_operator();
        let transfer = CloudTransfer::new(&op);
        let temp_dir = temp_dir::TempDir::new().expect("temp dir should be created");
        let local_path = temp_dir.path().join("Backups.json");
        fs::write(&local_path, b"old-json").await?;

        transfer
            .write_local_bytes_atomically(&local_path, b"new-json")
            .await?;

        let persisted = fs::read(&local_path).await?;
        assert_eq!(persisted, b"new-json");
        Ok(())
    }
}
