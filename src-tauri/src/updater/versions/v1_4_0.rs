use serde::Deserialize;
use std::collections::HashMap;

use crate::{
    backup::{Game as CurrentGame, SaveUnit, SaveUnitType},
    config::{Config as CurrentConfig, FavoriteTreeNode, QuickActionsSettings, Settings},
    device::Device,
};

/// Version constant for 1.4.0
pub const VERSION: &str = "1.4.0";

/// Config structure for version 1.4.0
#[derive(Deserialize)]
pub struct Config {
    version: String,
    backup_path: String,
    games: Vec<Game>,
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    favorites: Vec<FavoriteTreeNode>,
    #[serde(default)]
    quick_action: QuickActionsSettings,
}

/// Game structure for version 1.4.0
#[derive(Deserialize)]
pub struct Game {
    name: String,
    save_paths: Vec<SaveUnit1_4_0>,
    game_path: Option<String>,
}

/// SaveUnit structure for version 1.4.0
#[derive(Deserialize)]
pub struct SaveUnit1_4_0 {
    unit_type: SaveUnitType,
    path: String,
    #[serde(default)]
    delete_before_apply: bool,
}

impl From<Config> for CurrentConfig {
    fn from(old: Config) -> Self {
        let current_device = Device::default();
        let current_device_id = current_device.id.clone();

        let games = old
            .games
            .into_iter()
            .map(|g| {
                let mut game_paths = HashMap::new();
                if let Some(p) = g.game_path {
                    game_paths.insert(current_device_id.clone(), p);
                }
                let save_paths = g
                    .save_paths
                    .into_iter()
                    .map(|su| {
                        let mut paths = HashMap::new();
                        paths.insert(current_device_id.clone(), su.path);
                        SaveUnit {
                            unit_type: su.unit_type,
                            paths,
                            delete_before_apply: su.delete_before_apply,
                        }
                    })
                    .collect();
                CurrentGame {
                    name: g.name,
                    save_paths,
                    game_paths,
                }
            })
            .collect();

        let mut devices = HashMap::new();
        devices.insert(current_device_id, current_device);

        CurrentConfig {
            version: env!("CARGO_PKG_VERSION").to_string(),
            backup_path: old.backup_path,
            games,
            settings: old.settings,
            favorites: old.favorites,
            quick_action: old.quick_action,
            devices,
        }
    }
}
