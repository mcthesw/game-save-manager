use serde::{Deserialize, Serialize};

/// A backup is a zip file that contains
/// all the file that the save unit has declared.
/// The date is the unique indicator for a backup
#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub date: String,
    pub describe: String,
    pub path: String, // like "D:\\SaveManager\save_data\Game1\date.zip"
}