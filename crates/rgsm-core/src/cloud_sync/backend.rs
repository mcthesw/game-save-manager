use std::time::Duration;

use opendal::Operator;
use opendal::layers::RetryLayer;
use opendal::services;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::CloudSettings;
use crate::config::get_config;
use crate::preclude::*;

/// How the S3 client addresses buckets.
///
/// - `PathStyle`: `https://endpoint/bucket/key` (default, works for most generic S3-compatible services)
/// - `VirtualHostedStyle`: `https://bucket.endpoint/key` (required by Tencent COS, Alibaba OSS, etc.)
/// - `Auto`: detect by endpoint host suffix; falls back to virtual-hosted-style for known providers
#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq, Default)]
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

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(tag = "type")]
pub enum Backend {
    Disabled,
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

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct CloudSyncSessionConfig {
    pub root_path: String,
    pub max_concurrency: usize,
    pub backend: Backend,
}

impl Backend {
    fn retry_layer() -> RetryLayer {
        RetryLayer::new()
            .with_jitter()
            .with_min_delay(Duration::from_millis(200))
            .with_max_delay(Duration::from_secs(2))
            .with_max_times(3)
    }

    pub fn get_op_with_root(&self, root: &str) -> Result<Operator, BackendError> {
        match self {
            Backend::Disabled => Err(BackendError::Disabled),
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
                Ok(Operator::new(builder)?.layer(Self::retry_layer()).finish())
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
                Ok(Operator::new(builder)?.layer(Self::retry_layer()).finish())
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

    pub fn normalized_max_concurrency(&self) -> usize {
        self.max_concurrency.max(1)
    }

    pub async fn check(&self) -> Result<(), BackendError> {
        const TEST_FILENAME: &str = "test.txt";
        const TEST_CONTENT: &str = "Hello from game save manager";

        let op = self.get_op()?;
        op.list(".")
            .await
            .map_err(|_| BackendError::OperatorCheck("Failed to list files.".into()))?;
        op.write(TEST_FILENAME, TEST_CONTENT)
            .await
            .map_err(|_| BackendError::OperatorCheck("Failed to create test file.".into()))?;
        let text = op
            .read(TEST_FILENAME)
            .await
            .map_err(|_| BackendError::OperatorCheck("Failed to read test file.".into()))?;
        let text = String::from_utf8(text.to_vec()).map_err(|_| {
            BackendError::OperatorCheck("Failed to convert test file to string.".into())
        })?;
        if text != TEST_CONTENT {
            return Err(BackendError::OperatorCheck(
                "Test file content does not match.".into(),
            ));
        }
        op.delete(TEST_FILENAME)
            .await
            .map_err(|_| BackendError::OperatorCheck("Failed to delete test file.".into()))?;
        Ok(())
    }

    pub fn fingerprint(&self) -> String {
        format!(
            "{}|{}",
            self.root_path,
            serde_json::to_string(&self.backend.clone().sanitize()).unwrap_or_default()
        )
    }
}

impl From<&CloudSettings> for CloudSyncSessionConfig {
    fn from(value: &CloudSettings) -> Self {
        Self {
            root_path: value.root_path.clone(),
            max_concurrency: value.max_concurrency.max(1),
            backend: value.backend.clone(),
        }
    }
}

impl Sanitizable for Backend {
    fn sanitize(self) -> Self {
        match self {
            Backend::Disabled => Backend::Disabled,
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
    use super::{S3AddressingStyle, normalize_virtual_host_endpoint};

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
}
