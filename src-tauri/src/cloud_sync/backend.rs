use opendal::services;
use opendal::Operator;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::config::get_config;
use crate::errors::BackendError;
use crate::traits::Sanitizable;

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
    /// TODO: 严格化检查
    pub async fn check(&self) -> Result<(), BackendError> {
        self.get_op()?.check().await?;
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
