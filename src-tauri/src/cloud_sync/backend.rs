use opendal::Operator;
use opendal::services;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::config::get_config;
use crate::preclude::*;

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(tag = "type")]
pub enum Backend {
    // TODO:增加更多后端支持
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

impl Backend {
    /// 获取 Operator 实例
    pub fn get_op(&self) -> Result<Operator, BackendError> {
        let root = get_config()?.settings.cloud_settings.root_path;
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
                    .root(&root);
                Ok(Operator::new(builder)?.finish())
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
                    .root(&root);
                Ok(Operator::new(builder)?.finish())
            }
        }
    }

    /// 检查后端是否可用
    pub async fn check(&self) -> Result<(), BackendError> {
        const TEST_FILENAME: &str = "test.txt";
        const TEST_CONTENT: &str = "Hello from game save manager";
        const TEST_DIR: &str = "test_dir";

        let op = self.get_op()?;
        // Step1: 检查是否可以列出文件
        op.list(".")
            .await
            .map_err(|_| BackendError::OperatorCheck("Failed to list files.".into()))?;
        // Step2: 检查是否可以创建文件
        op.write(TEST_FILENAME, TEST_CONTENT)
            .await
            .map_err(|_| BackendError::OperatorCheck("Failed to create test file.".into()))?;
        // Step3: 检查是否可以读取文件
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
        // Step4: 检查是否可以删除文件
        op.delete(TEST_FILENAME)
            .await
            .map_err(|_| BackendError::OperatorCheck("Failed to delete test file.".into()))?;
        // Step5: 检查是否可以创建目录
        op.create_dir(TEST_DIR)
            .await
            .map_err(|_| BackendError::OperatorCheck("Failed to create test directory.".into()))?;
        // Step6: 检查是否可以删除目录
        op.delete(TEST_DIR)
            .await
            .map_err(|_| BackendError::OperatorCheck("Failed to delete test directory.".into()))?;
        Ok(())
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
                endpoint: endpoint.clone(),
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
