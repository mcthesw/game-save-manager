use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use specta::Type;

use crate::preclude::*;

use super::Backend;

#[derive(Debug, Serialize, Deserialize, Clone, Type, SmartDefault)]
#[serde(default)]
pub struct CloudSettings {
    /// 是否启用跟随云同步（用户添加、删除时自动同步）
    pub always_sync: bool,
    /// 同步间隔，单位分钟，为0则不自动同步
    pub auto_sync_interval: u64,
    /// 云同步根目录
    #[default = "\"/game-save-manager\".to_string()"]
    pub root_path: String,
    /// 云同步后端设置
    #[default = "Backend::Disabled"]
    pub backend: Backend,
}

impl Sanitizable for CloudSettings {
    fn sanitize(self) -> Self {
        CloudSettings {
            backend: self.backend.sanitize(),
            ..self
        }
    }
}
