use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use opendal::Operator;
use opendal::layers::{RetryLayer, TimeoutLayer};
use opendal::services;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::CloudSettings;
use crate::config::get_config;
use crate::preclude::*;

/// Staging directory for atomic writes under a local-folder root. Namespace
/// classification must ignore it so an empty root stays empty.
pub(crate) const FS_ATOMIC_WRITE_DIR: &str = ".rgsm-atomic-writes";

/// How the S3 client addresses buckets.
///
/// - `PathStyle`: `https://endpoint/bucket/key` (default, works for most generic S3-compatible services)
/// - `VirtualHostedStyle`: `https://bucket.endpoint/key` (required by Tencent COS, Alibaba OSS, etc.)
/// - `Auto`: detect by endpoint host suffix; falls back to virtual-hosted-style for known providers
#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema, PartialEq, Eq, Default)]
pub enum S3AddressingStyle {
    #[default]
    PathStyle,
    VirtualHostedStyle,
    Auto,
}

impl S3AddressingStyle {
    /// Return whether virtual-hosted-style should be used for the given endpoint+bucket combination.
    fn resolve_virtual_host(&self, endpoint: &str, bucket: &str) -> (bool, String) {
        match self {
            S3AddressingStyle::PathStyle => (false, endpoint.to_string()),
            S3AddressingStyle::VirtualHostedStyle => {
                (true, normalize_virtual_host_endpoint(endpoint, bucket))
            }
            S3AddressingStyle::Auto => {
                let use_vh = auto_requires_virtual_host_style(endpoint);
                let effective = if use_vh {
                    normalize_virtual_host_endpoint(endpoint, bucket)
                } else {
                    endpoint.to_string()
                };
                (use_vh, effective)
            }
        }
    }
}

/// Heuristic used by `Auto` mode: returns true for known providers that require virtual-hosted-style.
fn auto_requires_virtual_host_style(endpoint: &str) -> bool {
    let Some(host) = extract_endpoint_host(endpoint) else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    host.ends_with(".myqcloud.com") || host.ends_with(".aliyuncs.com")
}

/// Strip a `bucket.` prefix from the endpoint hostname if present.
/// This normalises user-supplied virtual-hosted endpoints (e.g. `bucket.cos.region.myqcloud.com`)
/// into bare base endpoints (`cos.region.myqcloud.com`) so that OpenDAL can re-attach them
/// correctly when `enable_virtual_host_style()` is set.
fn normalize_virtual_host_endpoint(endpoint: &str, bucket: &str) -> String {
    let bucket = bucket.trim();
    if bucket.is_empty() {
        return endpoint.to_string();
    }

    let Some(host) = extract_endpoint_host(endpoint) else {
        return endpoint.to_string();
    };

    let bucket_prefix = format!("{}.", bucket.to_ascii_lowercase());
    if !host.to_ascii_lowercase().starts_with(&bucket_prefix) {
        return endpoint.to_string();
    }

    replace_endpoint_host(endpoint, &host[bucket.len() + 1..])
}

fn extract_endpoint_host(endpoint: &str) -> Option<&str> {
    let (start, end) = endpoint_host_range(endpoint)?;
    Some(&endpoint[start..end])
}

fn replace_endpoint_host(endpoint: &str, new_host: &str) -> String {
    let Some((start, end)) = endpoint_host_range(endpoint) else {
        return endpoint.to_string();
    };

    format!("{}{}{}", &endpoint[..start], new_host, &endpoint[end..])
}

fn endpoint_host_range(endpoint: &str) -> Option<(usize, usize)> {
    let start = endpoint.find("://").map(|index| index + 3).unwrap_or(0);
    let remainder = &endpoint[start..];
    let authority_len = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_len];

    if authority.is_empty() {
        return None;
    }

    let host_len = if authority.starts_with('[') {
        authority.find(']')? + 1
    } else {
        authority.find(':').unwrap_or(authority.len())
    };

    if host_len == 0 {
        return None;
    }

    Some((start, start + host_len))
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type, utoipa::ToSchema)]
#[serde(tag = "type")]
pub enum Backend {
    Disabled,
    /// Local filesystem backend powered by OpenDAL.
    ///
    /// The physical root is stored in [`CloudSettings::root_path`] so all
    /// backends keep using the same operator-construction contract.
    Fs,
    /// WebDAV 后端
    /// 参考：https://docs.rs/opendal/latest/opendal/services/struct.Webdav.html
    /// 不支持 blocking
    WebDAV {
        endpoint: String,
        username: String,
        password: String,
    },
    /// Amazon S3 后端
    /// 参考：https://docs.rs/opendal/latest/opendal/services/struct.S3.html
    /// 不支持 rename 和 blocking
    S3 {
        endpoint: String,
        bucket: String,
        region: String,
        access_key_id: String,
        secret_access_key: String,
        /// How buckets are addressed. Defaults to `PathStyle` for backward compatibility.
        #[serde(default)]
        addressing_style: S3AddressingStyle,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema)]
pub struct CloudSyncSessionConfig {
    pub root_path: String,
    pub backend: Backend,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Type, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudBackendCheckOutcome {
    Available,
    Degraded,
    Unavailable,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Type, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudBackendCheckStep {
    PrepareBackend,
    ListFiles,
    WriteFile,
    ReadFile,
    VerifyContent,
    DeleteFile,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Type, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudBackendCheckItemStatus {
    Passed,
    Warning,
    Failed,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema, PartialEq, Eq)]
pub struct CloudBackendCheckItem {
    pub step: CloudBackendCheckStep,
    pub status: CloudBackendCheckItemStatus,
    pub critical: bool,
    pub message: Option<String>,
}

impl CloudBackendCheckItem {
    fn passed(step: CloudBackendCheckStep, critical: bool) -> Self {
        Self {
            step,
            status: CloudBackendCheckItemStatus::Passed,
            critical,
            message: None,
        }
    }

    fn warning(step: CloudBackendCheckStep, message: impl Into<String>) -> Self {
        Self {
            step,
            status: CloudBackendCheckItemStatus::Warning,
            critical: false,
            message: Some(message.into()),
        }
    }

    fn failed(step: CloudBackendCheckStep, message: impl Into<String>) -> Self {
        Self {
            step,
            status: CloudBackendCheckItemStatus::Failed,
            critical: true,
            message: Some(message.into()),
        }
    }

    fn blocks_usage(&self) -> bool {
        self.critical && self.status == CloudBackendCheckItemStatus::Failed
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema, PartialEq, Eq)]
pub struct CloudBackendCheckReport {
    pub outcome: CloudBackendCheckOutcome,
    pub items: Vec<CloudBackendCheckItem>,
}

impl CloudBackendCheckReport {
    fn from_items(items: Vec<CloudBackendCheckItem>) -> Self {
        let outcome = if items.iter().any(CloudBackendCheckItem::blocks_usage) {
            CloudBackendCheckOutcome::Unavailable
        } else if items
            .iter()
            .any(|item| item.status == CloudBackendCheckItemStatus::Warning)
        {
            CloudBackendCheckOutcome::Degraded
        } else {
            CloudBackendCheckOutcome::Available
        };

        Self { outcome, items }
    }

    pub fn is_usable(&self) -> bool {
        self.outcome != CloudBackendCheckOutcome::Unavailable
    }

    pub fn blocking_error_message(&self) -> Option<String> {
        self.items
            .iter()
            .find(|item| item.blocks_usage())
            .and_then(|item| item.message.clone())
            .or_else(|| {
                if self.is_usable() {
                    None
                } else {
                    Some("A required cloud backend check failed.".to_string())
                }
            })
    }
}

fn check_failure_message(action: &str, err: impl std::fmt::Display) -> String {
    format!("{action}: {err}")
}

impl Backend {
    /// A stalled connection would otherwise hang a sync, and the exclusive cloud
    /// operation lock with it, until the OS drops the socket. The limits apply per
    /// request or chunk, not per archive, and stay loose for slow links. WebDAV
    /// sizes its own limits to each request and does not use this layer.
    fn timeout_layer() -> TimeoutLayer {
        TimeoutLayer::new()
            .with_timeout(Duration::from_secs(120))
            .with_io_timeout(Duration::from_secs(60))
    }

    /// Timeout must sit inside retry: it drops the timed-out future, which retry
    /// then replays.
    fn harden<A: opendal::raw::Access>(builder: opendal::OperatorBuilder<A>) -> Operator {
        builder
            .layer(Self::timeout_layer())
            .layer(Self::retry_layer())
            .finish()
    }

    /// Three attempts in all.
    fn retry_layer() -> RetryLayer {
        RetryLayer::new()
            .with_jitter()
            .with_min_delay(Duration::from_millis(200))
            .with_max_delay(Duration::from_secs(2))
            .with_max_times(2)
    }

    pub fn get_op_with_root(&self, root: &str) -> Result<Operator, BackendError> {
        match self {
            Backend::Disabled => Err(BackendError::Disabled),
            Backend::Fs => {
                if !Path::new(root).is_absolute() {
                    return Err(BackendError::OperatorCheck(
                        "Local folder backend requires an absolute root path.".to_string(),
                    ));
                }
                // Without this, OpenDAL truncates the destination in place, so an
                // interrupted metadata write leaves a half-written manifest. The
                // directory must share the root's volume for the final rename.
                let builder = services::Fs::default()
                    .root(root)
                    .atomic_write_dir(&Path::new(root).join(FS_ATOMIC_WRITE_DIR).to_string_lossy());
                Ok(Self::harden(Operator::new(builder)?))
            }
            Backend::WebDAV {
                endpoint,
                username,
                password,
            } => {
                let builder = services::Webdav::default()
                    .endpoint(endpoint)
                    .username(username)
                    .password(password)
                    .root(root);
                Ok(Operator::new(builder)?
                    .layer(super::webdav::compatibility_layer()?)
                    .layer(Self::retry_layer())
                    .finish())
            }
            Backend::S3 {
                endpoint,
                bucket,
                region,
                access_key_id,
                secret_access_key,
                addressing_style,
            } => {
                let (use_virtual_host, effective_endpoint) =
                    addressing_style.resolve_virtual_host(endpoint, bucket);
                let builder = services::S3::default()
                    .endpoint(&effective_endpoint)
                    .bucket(bucket)
                    .region(region)
                    .access_key_id(access_key_id)
                    .secret_access_key(secret_access_key)
                    .root(root);
                let builder = if use_virtual_host {
                    builder.enable_virtual_host_style()
                } else {
                    builder
                };
                Ok(Self::harden(Operator::new(builder)?))
            }
        }
    }

    pub fn get_op(&self) -> Result<Operator, BackendError> {
        let root = get_config()?.settings.cloud_settings.root_path;
        self.get_op_with_root(&root)
    }
}

impl CloudSyncSessionConfig {
    pub fn get_op(&self) -> Result<Operator, BackendError> {
        self.backend.get_op_with_root(&self.root_path)
    }

    pub async fn check(&self) -> Result<(), BackendError> {
        let report = self.check_report().await;
        if let Some(message) = report.blocking_error_message() {
            return Err(BackendError::OperatorCheck(message));
        }
        Ok(())
    }

    pub async fn check_report(&self) -> CloudBackendCheckReport {
        const TEST_CONTENT: &str = "Hello from game save manager";

        let mut items = Vec::new();
        let op = match self.get_op() {
            Ok(op) => {
                items.push(CloudBackendCheckItem::passed(
                    CloudBackendCheckStep::PrepareBackend,
                    true,
                ));
                op
            }
            Err(err) => {
                items.push(CloudBackendCheckItem::failed(
                    CloudBackendCheckStep::PrepareBackend,
                    check_failure_message("Failed to prepare cloud backend", err),
                ));
                return CloudBackendCheckReport::from_items(items);
            }
        };

        use super::v2::{CloudLibraryBootstrap, CloudNamespaceClassification};
        let inspection = CloudLibraryBootstrap::new(op.clone(), 3).inspect().await;
        let error = match inspection {
            Ok(CloudNamespaceClassification::V1Only { .. }) => {
                Some(BackendError::LegacyCloudOperationUnavailable.to_string())
            }
            Ok(_) => None,
            Err(error) => Some(error.to_string()),
        };
        if let Some(error) = error {
            items.push(CloudBackendCheckItem::failed(
                CloudBackendCheckStep::ListFiles,
                error,
            ));
            return CloudBackendCheckReport::from_items(items);
        }

        match op.list(".").await {
            Ok(_) => items.push(CloudBackendCheckItem::passed(
                CloudBackendCheckStep::ListFiles,
                false,
            )),
            Err(err) => items.push(CloudBackendCheckItem::warning(
                CloudBackendCheckStep::ListFiles,
                check_failure_message("Failed to list files", err),
            )),
        }

        let test_filename = format!(
            ".rgsm-backend-check-{}-{}",
            std::process::id(),
            Utc::now().timestamp_millis()
        );

        if let Err(err) = op.write(test_filename.as_str(), TEST_CONTENT).await {
            items.push(CloudBackendCheckItem::failed(
                CloudBackendCheckStep::WriteFile,
                check_failure_message("Failed to create test file", err),
            ));
            return CloudBackendCheckReport::from_items(items);
        }
        items.push(CloudBackendCheckItem::passed(
            CloudBackendCheckStep::WriteFile,
            true,
        ));

        let text = match op.read(test_filename.as_str()).await {
            Ok(text) => {
                items.push(CloudBackendCheckItem::passed(
                    CloudBackendCheckStep::ReadFile,
                    true,
                ));
                Some(text)
            }
            Err(err) => {
                items.push(CloudBackendCheckItem::failed(
                    CloudBackendCheckStep::ReadFile,
                    check_failure_message("Failed to read test file", err),
                ));
                None
            }
        };

        if let Some(text) = text {
            match String::from_utf8(text.to_vec()) {
                Ok(text) if text == TEST_CONTENT => items.push(CloudBackendCheckItem::passed(
                    CloudBackendCheckStep::VerifyContent,
                    true,
                )),
                Ok(_) => items.push(CloudBackendCheckItem::failed(
                    CloudBackendCheckStep::VerifyContent,
                    "Test file content does not match.",
                )),
                Err(err) => items.push(CloudBackendCheckItem::failed(
                    CloudBackendCheckStep::VerifyContent,
                    check_failure_message("Failed to convert test file to string", err),
                )),
            }
        } else {
            items.push(CloudBackendCheckItem::failed(
                CloudBackendCheckStep::VerifyContent,
                "Skipped because the test file could not be read.",
            ));
        }

        match op.delete(test_filename.as_str()).await {
            Ok(_) => items.push(CloudBackendCheckItem::passed(
                CloudBackendCheckStep::DeleteFile,
                true,
            )),
            Err(err) => items.push(CloudBackendCheckItem::failed(
                CloudBackendCheckStep::DeleteFile,
                check_failure_message("Failed to delete test file", err),
            )),
        }

        CloudBackendCheckReport::from_items(items)
    }
}

impl From<&CloudSettings> for CloudSyncSessionConfig {
    fn from(value: &CloudSettings) -> Self {
        Self {
            root_path: value.root_path.clone(),
            backend: value.backend.clone(),
        }
    }
}

impl Sanitizable for Backend {
    fn sanitize(self) -> Self {
        match self {
            Backend::Disabled => Backend::Disabled,
            Backend::Fs => Backend::Fs,
            Backend::WebDAV {
                endpoint,
                username: _,
                password: _,
            } => Backend::WebDAV {
                endpoint,
                username: "*username*".to_string(),
                password: "*password*".to_string(),
            },
            Backend::S3 {
                endpoint: _,
                bucket: _,
                region: _,
                access_key_id: _,
                secret_access_key: _,
                addressing_style,
            } => Backend::S3 {
                endpoint: "*endpoint*".to_string(),
                bucket: "*bucket*".to_string(),
                region: "*region*".to_string(),
                access_key_id: "*access_key_id*".to_string(),
                secret_access_key: "*secret_access_key*".to_string(),
                addressing_style: addressing_style.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::preclude::Sanitizable;

    use super::{
        Backend, CloudBackendCheckItem, CloudBackendCheckOutcome, CloudBackendCheckReport,
        CloudBackendCheckStep, CloudSyncSessionConfig, S3AddressingStyle,
        normalize_virtual_host_endpoint,
    };

    #[test]
    fn path_style_never_enables_virtual_host() {
        let (use_vh, endpoint) = S3AddressingStyle::PathStyle
            .resolve_virtual_host("https://cos.ap-nanjing.myqcloud.com", "my-bucket-125000");
        assert!(!use_vh);
        assert_eq!(endpoint, "https://cos.ap-nanjing.myqcloud.com");
    }

    #[test]
    fn virtual_hosted_style_always_enables_virtual_host() {
        let (use_vh, endpoint) = S3AddressingStyle::VirtualHostedStyle
            .resolve_virtual_host("https://127.0.0.1:9000", "my-bucket");
        assert!(use_vh);
        assert_eq!(endpoint, "https://127.0.0.1:9000");
    }

    #[test]
    fn auto_keeps_path_style_for_generic_s3() {
        let (use_vh, endpoint) =
            S3AddressingStyle::Auto.resolve_virtual_host("https://127.0.0.1:9000", "my-bucket");
        assert!(!use_vh);
        assert_eq!(endpoint, "https://127.0.0.1:9000");
    }

    #[test]
    fn auto_enables_virtual_host_for_tencent_cos() {
        let (use_vh, endpoint) = S3AddressingStyle::Auto
            .resolve_virtual_host("https://cos.ap-nanjing.myqcloud.com", "my-bucket-125000");
        assert!(use_vh);
        assert_eq!(endpoint, "https://cos.ap-nanjing.myqcloud.com");
    }

    #[test]
    fn auto_enables_virtual_host_for_alibaba_oss() {
        let (use_vh, endpoint) = S3AddressingStyle::Auto
            .resolve_virtual_host("https://oss-cn-hangzhou.aliyuncs.com", "gsm-test");
        assert!(use_vh);
        assert_eq!(endpoint, "https://oss-cn-hangzhou.aliyuncs.com");
    }

    #[test]
    fn virtual_hosted_style_strips_bucket_prefix_from_endpoint() {
        let (use_vh, endpoint) = S3AddressingStyle::VirtualHostedStyle.resolve_virtual_host(
            "https://my-bucket-125000.cos.ap-nanjing.myqcloud.com",
            "my-bucket-125000",
        );
        assert!(use_vh);
        assert_eq!(endpoint, "https://cos.ap-nanjing.myqcloud.com");
    }

    #[test]
    fn normalize_does_not_strip_unrelated_prefix() {
        let result = normalize_virtual_host_endpoint(
            "https://other-host.cos.ap-nanjing.myqcloud.com",
            "my-bucket",
        );
        assert_eq!(result, "https://other-host.cos.ap-nanjing.myqcloud.com");
    }

    #[test]
    fn path_style_is_default_for_serde() {
        let json = r#"{"type":"S3","endpoint":"https://s3.example.com","bucket":"b","region":"us-east-1","access_key_id":"k","secret_access_key":"s"}"#;
        let backend: super::Backend = serde_json::from_str(json).unwrap();
        if let super::Backend::S3 {
            addressing_style, ..
        } = backend
        {
            assert_eq!(addressing_style, S3AddressingStyle::PathStyle);
        } else {
            panic!("expected S3 backend");
        }
    }

    #[test]
    fn fs_backend_round_trips_as_a_fieldless_variant() {
        let json = serde_json::to_string(&Backend::Fs).unwrap();
        assert_eq!(json, r#"{"type":"Fs"}"#);

        let backend: Backend = serde_json::from_str(&json).unwrap();
        assert!(matches!(backend, Backend::Fs));
        assert!(matches!(backend.sanitize(), Backend::Fs));
    }

    #[test]
    fn fs_backend_rejects_a_relative_root_before_writing() {
        let result = Backend::Fs.get_op_with_root("relative/cloud/root");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fs_backend_uses_the_selected_folder_as_its_exact_persistent_root() {
        let root = temp_dir::TempDir::new().unwrap();
        let root_path = root.path().to_string_lossy().into_owned();
        let session = CloudSyncSessionConfig {
            root_path,
            backend: Backend::Fs,
        };

        let first = session.get_op().unwrap();
        first
            .write("snapshot.bin", b"persistent bytes".as_slice())
            .await
            .unwrap();

        assert_eq!(
            std::fs::read(root.path().join("snapshot.bin")).unwrap(),
            b"persistent bytes"
        );
        assert!(!root.path().join("game-save-manager").exists());

        let second = session.get_op().unwrap();
        assert_eq!(
            second.read("snapshot.bin").await.unwrap().to_vec(),
            b"persistent bytes"
        );
    }

    fn visible_root_entries(root: &std::path::Path) -> Vec<std::ffi::OsString> {
        std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name != super::FS_ATOMIC_WRITE_DIR)
            .collect()
    }

    #[tokio::test]
    async fn a_silent_server_fails_the_operation_instead_of_hanging() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let accepted = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                held.push(stream);
            }
        });
        let operator =
            opendal::Operator::new(opendal::services::Webdav::default().endpoint(&endpoint))
                .unwrap()
                .layer(
                    crate::cloud_sync::webdav::compatibility_layer_with(
                        crate::cloud_sync::webdav::Limits {
                            request: std::time::Duration::from_millis(200),
                            upload_rate: 1024,
                        },
                    )
                    .unwrap(),
                )
                .layer(Backend::retry_layer())
                .finish();

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            operator.read("manifest.json"),
        )
        .await
        .expect("the timeout layer must end the request");

        assert!(result.is_err());
        accepted.abort();
    }

    #[tokio::test]
    async fn interrupted_fs_write_leaves_the_previous_object_intact() {
        let root = temp_dir::TempDir::new().unwrap();
        let op = Backend::Fs
            .get_op_with_root(&root.path().to_string_lossy())
            .unwrap();
        op.write("v2/cloud-manifest.json", b"{\"revision\":1}".to_vec())
            .await
            .unwrap();

        let mut writer = op.writer("v2/cloud-manifest.json").await.unwrap();
        writer.write(b"{\"revision\":2,".to_vec()).await.unwrap();
        drop(writer);

        assert_eq!(
            op.read("v2/cloud-manifest.json").await.unwrap().to_vec(),
            b"{\"revision\":1}"
        );
    }

    #[tokio::test]
    async fn fs_staging_directory_does_not_make_an_empty_root_unrecognized() {
        let root = temp_dir::TempDir::new().unwrap();
        let op = Backend::Fs
            .get_op_with_root(&root.path().to_string_lossy())
            .unwrap();
        op.write("probe", b"x".to_vec()).await.unwrap();
        op.delete("probe").await.unwrap();
        assert!(root.path().join(super::FS_ATOMIC_WRITE_DIR).is_dir());

        let classification = crate::cloud_sync::v2::CloudNamespaceClassifier::new(op)
            .classify()
            .await
            .unwrap();

        assert!(matches!(
            classification,
            crate::cloud_sync::v2::CloudNamespaceClassification::Empty
        ));
    }

    #[tokio::test]
    async fn fs_backend_health_check_succeeds_without_leaving_a_probe_file() {
        let root = temp_dir::TempDir::new().unwrap();
        let session = CloudSyncSessionConfig {
            root_path: root.path().to_string_lossy().into_owned(),
            backend: Backend::Fs,
        };

        let report = session.check_report().await;

        assert_eq!(report.outcome, CloudBackendCheckOutcome::Available);
        assert!(visible_root_entries(root.path()).is_empty());
    }

    #[tokio::test]
    async fn legacy_cloud_health_check_requires_migration() {
        let root = temp_dir::TempDir::new().unwrap();
        let config = serde_json::to_vec(&crate::config::Config::default()).unwrap();
        std::fs::write(root.path().join("GameSaveManager.config.json"), &config).unwrap();
        let session = CloudSyncSessionConfig {
            root_path: root.path().to_string_lossy().into_owned(),
            backend: Backend::Fs,
        };
        let report = session.check_report().await;
        assert!(report.blocking_error_message().is_some());
        assert!(
            !report
                .items
                .iter()
                .any(|item| item.step == CloudBackendCheckStep::WriteFile)
        );
        assert_eq!(
            std::fs::read(root.path().join("GameSaveManager.config.json")).unwrap(),
            config
        );
        assert_eq!(visible_root_entries(root.path()).len(), 1);
    }

    #[test]
    fn optional_list_failure_degrades_without_blocking_usage() {
        let report = CloudBackendCheckReport::from_items(vec![
            CloudBackendCheckItem::passed(CloudBackendCheckStep::PrepareBackend, true),
            CloudBackendCheckItem::warning(CloudBackendCheckStep::ListFiles, "502 Bad Gateway"),
            CloudBackendCheckItem::passed(CloudBackendCheckStep::WriteFile, true),
            CloudBackendCheckItem::passed(CloudBackendCheckStep::ReadFile, true),
            CloudBackendCheckItem::passed(CloudBackendCheckStep::VerifyContent, true),
            CloudBackendCheckItem::passed(CloudBackendCheckStep::DeleteFile, true),
        ]);

        assert_eq!(report.outcome, CloudBackendCheckOutcome::Degraded);
        assert!(report.is_usable());
        assert_eq!(report.blocking_error_message(), None);
    }

    #[test]
    fn critical_failure_marks_backend_unavailable() {
        let report = CloudBackendCheckReport::from_items(vec![
            CloudBackendCheckItem::passed(CloudBackendCheckStep::PrepareBackend, true),
            CloudBackendCheckItem::passed(CloudBackendCheckStep::ListFiles, false),
            CloudBackendCheckItem::failed(CloudBackendCheckStep::WriteFile, "AccessDenied"),
        ]);

        assert_eq!(report.outcome, CloudBackendCheckOutcome::Unavailable);
        assert!(!report.is_usable());
        assert_eq!(
            report.blocking_error_message(),
            Some("AccessDenied".to_string())
        );
    }
}
