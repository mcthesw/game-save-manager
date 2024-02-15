use opendal::services;
use opendal::Operator;
use serde::{Deserialize, Serialize};

use crate::errors::BackendError;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Backend {
    // TODO:增加更多后端支持
    Disabled,
    WebDAV {
        endpoint: String,
        username: String,
        password: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CloudSettings {
    /// 是否启用跟随云同步（用户添加、删除时自动同步）
    pub always_sync: bool,
    /// 同步间隔，单位分钟，为0则不自动同步
    pub auto_sync_interval: u64,
    /// 云同步后端设置
    pub backend: Backend,
}

impl Backend {
    /// 获取 Operator 实例
    fn get_op(&self) -> Result<Operator, BackendError> {
        let builder = match self {
            Backend::Disabled => {
                return Err(BackendError::Disabled);
            }
            Backend::WebDAV {
                endpoint,
                username,
                password,
            } => {
                let mut builder = services::Webdav::default();
                builder.endpoint(endpoint);
                builder.username(username);
                builder.password(password);
                builder
            }
        };
        Ok(Operator::new(builder)?.finish())
    }

    /// 检查后端是否可用
    pub async fn check(&self) -> Result<(), BackendError> {
        self.get_op()?.check().await?;
        Ok(())
    }

    /// 上传文件
    pub async fn upload(&self, cloud_path: &str, local_path: &str) -> Result<(), BackendError> {
        let data = std::fs::read(local_path)?;
        self.get_op()?.write(cloud_path, data).await?;
        Ok(())
    }

    /// 删除文件
    pub async fn delete(&self, cloud_path: &str) -> Result<(), BackendError> {
        self.get_op()?.delete(cloud_path).await?;
        Ok(())
    }
}

// async fn sync() -> Result<(), BackendError> {
//     // TODO:重做错误处理
//     // 获取设置，检查是否启用
//     let settings = get_config().unwrap().settings.cloud_settings;
//     match settings.backend {
//         Backend::Disabled => {
//             return Err(BackendError::Disabled);
//         }
//         _ => {}
//     }

//     let op = settings.backend.get_op()?;
//     // 获取配置文件，检查是否需要同步
//     let new_config = get_config().unwrap();
//     if op.is_exist("GameSaveManager.config.json").await? {
//         let local_modified = get_config_metadata().unwrap().modified().unwrap();
//         let remote_modified = op
//             .stat("GameSaveManager.config.json")
//             .await?
//             .last_modified()
//             .unwrap();
//         println!("Local: {:?}, Remote: {:?}", local_modified, remote_modified);
//         // 本地配置文件与远程版本比较并合并
//     }
//     // 上传合并后的配置文件
//     op.write(
//         "GameSaveManager.config.json",
//         serde_json::to_string_pretty(&new_config).unwrap(),
//     );

//     // 同步存档文件
//     todo!()
// }

// fn merge_config(remote: Config) -> Result<Config, BackendError> {
//     // TODO:现在只实现上传功能，后面需要补全合并功能
//     // TODO:做错误处理
//     let local = get_config().unwrap();
//     Ok(local)
// }
