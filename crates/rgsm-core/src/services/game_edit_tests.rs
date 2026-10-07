use super::*;
use crate::config::{ConfigTestStateGuard, get_config, set_config_local};
use serde_json::json;

#[test]
fn game_and_device_variables_commit_together_and_reject_stale_edits() -> Result<()> {
    let _lock = crate::config::lock_config_test_file();
    let temp = temp_dir::TempDir::new()?;
    let id = get_current_device_id();
    let mut config = Config {
        backup_path: temp.path().join("archives").to_string_lossy().into_owned(),
        ..Config::default()
    };
    config.devices.insert(
        id.clone(),
        serde_json::from_value(
            json!({"id": id, "name": "Test", "path_variables": {"account": "old"}}),
        )?,
    );
    let _guard = ConfigTestStateGuard::replace_with(&config)?;
    set_config_local(&config)?;
    let service = ServiceContext::new(std::sync::Arc::new(crate::hooks::HookPipeline::new(vec![])));
    let edits = [(
        "account".into(),
        DeviceVariableEdit {
            previous_value: Some("old".into()),
            value: "new".into(),
        },
    )]
    .into_iter()
    .collect();
    let mut draft: GameDraft = serde_json::from_value(
        json!({"name": "New game", "save_paths": [{"source": {"type": "manifestPattern", "pattern": "<var:missing>/save"}}]}),
    )?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        assert!(
            service
                .save_game_edit(None, &draft, &edits, HookSource::UserManual)
                .await
                .is_err()
        );
        assert_eq!(get_config()?.devices[id].path_variables["account"], "old");
        assert!(get_config()?.games.is_empty());
        draft.save_paths.clear();
        let game = service
            .save_game_edit(None, &draft, &edits, HookSource::UserManual)
            .await?;
        assert_eq!(get_config()?.devices[id].path_variables["account"], "new");
        draft.name = "Must not be saved".into();
        assert!(
            service
                .save_game_edit(
                    Some(&game.storage_key),
                    &draft,
                    &edits,
                    HookSource::UserManual
                )
                .await
                .is_err()
        );
        assert_eq!(get_config()?.games[0].name, "New game");
        Ok(())
    })
}

#[test]
fn preview_uses_pending_defaults_without_persisting_them() {
    let id = get_current_device_id();
    let temp = temp_dir::TempDir::new().unwrap();
    std::fs::write(temp.path().join("save.sav"), b"save").unwrap();
    let mut config = Config::default();
    config.devices.insert(
        id.clone(),
        serde_json::from_value(json!({"id": id, "name": "Test"})).unwrap(),
    );
    let edits = [(
        "root".into(),
        DeviceVariableEdit {
            previous_value: None,
            value: temp.path().to_string_lossy().into_owned(),
        },
    )]
    .into_iter()
    .collect();
    let preview = crate::services::PathPreviewContext {
        device_variables: edits,
        ..Default::default()
    };
    let service = ServiceContext::new(std::sync::Arc::new(crate::hooks::HookPipeline::new(vec![])));
    let result = service.check_ad_hoc_paths(&config, &["<var:root>/save.sav".into()], &preview);
    assert!(matches!(
        result[0],
        crate::path_resolver::PathCheckResult::Ok { .. }
    ));
    assert!(config.devices[id].path_variables.is_empty());
}
