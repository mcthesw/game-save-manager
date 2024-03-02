use crate::cloud::{Backend, CloudSettings};

pub fn default_false() -> bool {
    false
}
pub fn default_true() -> bool {
    true
}
pub fn default_zero() -> u64 {
    0
}
pub fn default_root_path() -> String {
    "/game-save-manager".to_string()
}
pub fn default_backend() -> Backend {
    Backend::Disabled
}
pub fn default_cloud_settings() -> CloudSettings {
    CloudSettings {
        always_sync: false,
        auto_sync_interval: 0,
        root_path: "/game-save-manager".to_string(),
        backend: Backend::Disabled,
    }
}
