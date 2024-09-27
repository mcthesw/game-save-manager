use serde::{Deserialize, Serialize};

use crate::default_value;

/// A save unit should be a file or a folder
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SaveUnitType {
    File,
    Folder,
}

/// A save unit declares one of the files/folders
/// that should be backup for a game
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SaveUnit {
    pub unit_type: SaveUnitType,
    pub path: String,
    #[serde(default = "default_value::default_false")]
    pub delete_before_apply: bool,
}
