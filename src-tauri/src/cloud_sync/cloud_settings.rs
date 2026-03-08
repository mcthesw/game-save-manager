use serde::{Deserialize, Serialize};
use specta::Type;

use crate::default_value;
use crate::preclude::*;

use super::Backend;

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct CloudSettings {
    /// Legacy field — kept only for deserialization of old configs.
    /// Per-game `cloud_sync_enabled` on `Game` replaces this.
    #[serde(default = "default_value::default_false")]
    #[deprecated(note = "Use Game.cloud_sync_enabled instead")]
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
    /// 最大并发数
    #[serde(default = "default_value::default_one_usize")]
    pub max_concurrency: usize,
}

#[allow(deprecated)]
impl Default for CloudSettings {
    fn default() -> Self {
        CloudSettings {
            always_sync: false,
            auto_sync_interval: 0,
            root_path: "/game-save-manager".to_string(),
            backend: Backend::Disabled,
            max_concurrency: 1,
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
