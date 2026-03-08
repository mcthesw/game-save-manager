use std::time::Duration;

use opendal::Operator;
use opendal::layers::RetryLayer;
use opendal::services;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::CloudSettings;
use crate::config::get_config;
use crate::preclude::*;

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
            } => {
                let builder = services::S3::default()
                    .endpoint(endpoint)
                    .bucket(bucket)
                    .region(region)
                    .access_key_id(access_key_id)
                    .secret_access_key(secret_access_key)
                    .root(root);
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
            } => Backend::S3 {
                endpoint: "*endpoint*".to_string(),
                bucket: "*bucket*".to_string(),
                region: "*region*".to_string(),
                access_key_id: "*access_key_id*".to_string(),
                secret_access_key: "*secret_access_key*".to_string(),
            },
        }
    }
}
