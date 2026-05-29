mod app_config;
pub mod backup;
mod quick_actions_settings;
mod settings;
mod utils;

pub use app_config::{Config, FavoriteTreeNode};
pub use quick_actions_settings::{
    GameAutomationSettings, GameAutomationSettingsDraft, QuickActionSoundPreferences,
    QuickActionSoundSlots, QuickActionSoundSource, QuickActionsSettings,
};
pub use settings::{AppearanceSettings, SaveListExpandBehavior, Settings};
pub use utils::*;
