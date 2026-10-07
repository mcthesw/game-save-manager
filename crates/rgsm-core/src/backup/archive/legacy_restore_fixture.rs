//! Route historical fixture tests through the production restore service.
use std::path::Path;

use crate::{
    backup::{Game, RestoreNotifier, SaveUnit, ZipBackend},
    config::get_config,
    hooks::HookPipeline,
    preclude::BackupError,
    services::ServiceContext,
};

pub fn decompress_from_file(
    save_paths: &[SaveUnit],
    backup_path: &Path,
    date: &str,
    notifier: Option<&dyn RestoreNotifier>,
) -> Result<(), BackupError> {
    let game: Game = serde_json::from_value(serde_json::json!({
        "name": "historical-fixture",
        "save_paths": save_paths,
    }))
    .expect("fixture game");
    ServiceContext::new(HookPipeline::new(Vec::new()).into()).restore_capture_archive(
        &get_config()?,
        &game,
        &backup_path.join(format!("{date}.zip")),
        &ZipBackend,
        notifier,
    )
}
