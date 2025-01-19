use serde::{Deserialize, Serialize};
use specta::Type;

use crate::default_value;
use crate::preclude::*;

use super::Backend;

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct CloudSettings {
    /// 是否启用跟随云同步（用户添加、删除时自动同步）
    #[serde(default = "default_value::default_false")]
    pub always_sync: bool,
    /// 同步间隔，单位分钟，为0则不自动同步
    #[serde(default = "default_value::default_zero")]
    pub auto_sync_interval: u64,
    /// 云同步根目录
    #[serde(default = "default_value::default_root_path")]
    pub root_path: String,
    /// 云同步后端设置
    #[serde(default = "default_value::default_backend")]
    pub backend: Backend,
}

impl Default for CloudSettings {
    fn default() -> Self {
        CloudSettings {
            always_sync: false,
            auto_sync_interval: 0,
            root_path: "/game-save-manager".to_string(),
            backend: Backend::Disabled,
        }
    }
}

impl Sanitizable for CloudSettings {
    fn sanitize(self) -> Self {
        CloudSettings {
            backend: self.backend.sanitize(),
            ..self
        }
    }
}
