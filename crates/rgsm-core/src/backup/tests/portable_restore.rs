use std::{collections::HashMap, fs, path::Path, sync::Arc};

use crate::{
    backup::{Game, GameDraft, SaveUnit, SaveUnitSource, SaveUnitType},
    config::Config,
    device::get_current_device_id,
    hooks::{HookPipeline, HookSource},
    services::ServiceContext,
};

use super::utils::{ConfigFileGuard, lock_config_file};

fn source(path: &Path) -> SaveUnitSource {
    SaveUnitSource::Concrete {
        unit_type: SaveUnitType::File,
        paths: HashMap::from([(
            get_current_device_id().clone(),
            path.to_string_lossy().into_owned(),
        )]),
    }
}

#[test]
fn same_name_entries_restore_by_identity_after_game_and_path_edits()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = lock_config_file();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let temp = temp_dir::TempDir::new()?;
        let old_paths = [temp.path().join("one/save.sav"), temp.path().join("two/save.sav")];
        for (path, contents) in old_paths.iter().zip(["first-slot", "second-slot"]) {
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, contents)?;
        }
        let game: Game = serde_json::from_value(serde_json::json!({
            "name": "Original title", "storage_key": "stable-game",
            "save_paths": old_paths.iter().enumerate().map(|(index, path)| SaveUnit {
                id: index as u32 + 1, source: source(path), enabled: true, delete_before_apply: false,
            }).collect::<Vec<_>>(),
            "next_save_unit_id": 3,
        }))?;
        let config = Config {
            backup_path: temp.path().join("backups").to_string_lossy().into_owned(),
            games: vec![game.clone()],
            ..Config::default()
        };
        let _guard = ConfigFileGuard::write_config(&config)?;
        let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));
        service.create_snapshot(&game, "original note", HookSource::UserManual, None).await?;
        let snapshot = game.get_game_snapshots_info()?.backups.remove(0);
        let extracted = temp.path().join("extracted");
        sevenz_rust2::decompress_file(&snapshot.path, &extracted)?;
        assert_eq!(fs::read(extracted.join("save.sav"))?, b"first-slot");
        assert_eq!(fs::read(extracted.join("save (2).sav"))?, b"second-slot");

        let destinations = [temp.path().join("renamed-one.sav"), temp.path().join("renamed-two.sav")];
        let mut draft: GameDraft = serde_json::from_value(serde_json::to_value(&game)?)?;
        draft.name = "Renamed game".into();
        for (unit, path) in draft.save_paths.iter_mut().zip(&destinations) {
            unit.source = source(path);
        }
        draft.save_paths.reverse();
        let edited = service.save_game_edit(Some(&game.storage_key), &draft, &Default::default(), HookSource::UserManual).await?;
        assert_eq!(edited.storage_key, game.storage_key);
        assert_eq!(edited.save_paths.iter().map(|unit| unit.id).collect::<Vec<_>>(), [2, 1]);
        service.restore_snapshot(&edited, &snapshot.date, HookSource::UserManual, None).await?;
        assert_eq!(fs::read(&destinations[0])?, b"first-slot");
        assert_eq!(fs::read(&destinations[1])?, b"second-slot");
        assert_eq!(edited.get_game_snapshots_info()?.backups[0].date, snapshot.date);
        // Editing the live configuration must not rewrite captured identity or notes.
        let manifest: serde_json::Value = serde_json::from_slice(&fs::read(extracted.join("_rgsm/manifest.json"))?)?;
        assert_eq!(manifest["identity"]["gameName"], "Original title");
        assert_eq!(manifest["identity"]["description"], "original note");
        Ok(())
    })
}
