mod app_config;
pub mod backup;
pub(crate) mod owner_store;
mod ownership;
#[cfg(test)]
mod ownership_tests;
mod quick_actions_settings;
mod settings;
mod utils;

pub use app_config::{Config, FavoriteTreeNode};
pub use owner_store::OwnerStoreError;
pub use ownership::{
    ConfigurationOwners, DeviceBehaviorSettings, DeviceGameProfile, DeviceProfile,
    DeviceSaveUnitSettings, EffectiveConfiguration, LocalInterfaceSettings, LocalState,
    OwnershipError, SharedGame, SharedLibrary, SharedSaveUnit, SharedSaveUnitSource, SyncMode,
    V2_CONFIG_SCHEMA_VERSION,
};
pub use quick_actions_settings::{
    GameAutomationSettings, GameAutomationSettingsDraft, QuickActionSoundPreferences,
    QuickActionSoundSlots, QuickActionSoundSource, QuickActionsSettings,
};
pub use settings::{
    AppearanceSettings, SaveListExpandBehavior, SaveListSortMode, Settings, SortDirection,
};
pub use utils::*;
