mod backend;
mod cloud_settings;
pub(crate) mod join_code;
pub mod rate_limit;
mod task_manager;
pub mod transfer;
mod utils;
pub mod v2;
mod webdav;

pub const V1_CONFIG_PATH: &str = "/GameSaveManager.config.json";
pub const V1_SAVE_DATA_PREFIX: &str = "save_data/";

#[cfg(test)]
pub(crate) use backend::FS_ATOMIC_WRITE_DIR;
pub use backend::{
    Backend, CloudBackendCheckItem, CloudBackendCheckItemStatus, CloudBackendCheckOutcome,
    CloudBackendCheckReport, CloudBackendCheckStep, CloudSyncSessionConfig, S3AddressingStyle,
};
pub use cloud_settings::CloudSettings;
pub use task_manager::{
    CancelCloudSyncResult, CloudSyncJobInfo, CloudSyncJobStatus, CloudSyncStatus,
    CloudSyncTaskManager, SyncEventEmitter,
};
pub use utils::SyncOperationError;
