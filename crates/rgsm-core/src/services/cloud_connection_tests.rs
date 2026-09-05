use std::sync::Arc;

use super::*;
use crate::cloud_sync::v2::{
    CloudNamespaceDescriptor, DeletionRegistryRepository, JoinGameAction, SharedLibraryRepository,
    device_profile_path,
};
use crate::cloud_sync::{Backend, CloudSettings};
use crate::config::{Config, ConfigTestStateGuard, get_config, set_config_local};
use crate::hooks::HookPipeline;

struct Fixture {
    _config: ConfigTestStateGuard,
    cloud: temp_dir::TempDir,
    archives: temp_dir::TempDir,
    operator: opendal::Operator,
    service: ServiceContext,
}

impl Fixture {
    async fn new() -> Self {
        let cloud = temp_dir::TempDir::new().unwrap();
        let archives = temp_dir::TempDir::new().unwrap();
        let mut config = Config {
            backup_path: archives.path().to_string_lossy().into_owned(),
            games: serde_json::from_value(serde_json::json!([
                {"name": "Local version", "storage_key": "pending", "save_paths": []},
                {"name": "Ready", "storage_key": "ready", "save_paths": []}
            ]))
            .unwrap(),
            ..Config::default()
        };
        config.settings.cloud_settings = CloudSettings {
            backend: Backend::Fs,
            root_path: cloud.path().to_string_lossy().into_owned(),
            ..CloudSettings::default()
        };
        let guard = ConfigTestStateGuard::replace_with(&config).unwrap();
        set_config_local(&config).unwrap();
        let (mut remote, mut other, _) = cloud_bootstrap_inputs().unwrap();
        remote.games[0].name = "Cloud version".into();
        other.device.id = "other-device".into();
        let operator = CloudSyncSessionConfig::from(&config.settings.cloud_settings)
            .get_op()
            .unwrap();
        CloudLibraryBootstrap::new(operator.clone(), 2)
            .create_empty(&CloudNamespaceDescriptor::default(), &remote, &other)
            .await
            .unwrap();
        std::fs::create_dir_all(archives.path().join("pending")).unwrap();
        std::fs::write(
            archives.path().join("pending/snapshot.zip"),
            b"local archive",
        )
        .unwrap();
        Self {
            _config: guard,
            cloud,
            archives,
            operator,
            service: ServiceContext::new(Arc::new(HookPipeline::new(vec![]))),
        }
    }

    fn profile_path(&self) -> std::path::PathBuf {
        let (_, _, state) = cloud_bootstrap_inputs().unwrap();
        self.cloud
            .path()
            .join(device_profile_path(&state.current_device_id))
    }

    fn assert_local_protected(&self) {
        let (_, _, state) = cloud_bootstrap_inputs().unwrap();
        assert!(state.is_local_game("pending"));
        assert_eq!(get_config().unwrap().games[0].name, "Local version");
        assert_eq!(
            std::fs::read(self.archives.path().join("pending/snapshot.zip")).unwrap(),
            b"local archive"
        );
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn failed_initial_profile_publication_can_retry_connection() {
    let _lock = crate::config::lock_config_test_file();
    runtime().block_on(async {
        let fixture = Fixture::new().await;
        let profile_path = fixture.profile_path();
        // An empty directory at the exact file target simulates a failed write.
        std::fs::create_dir(&profile_path).unwrap();
        assert!(fixture.service.connect_cloud_library().await.is_err());
        assert_eq!(
            cloud_bootstrap_inputs()
                .unwrap()
                .2
                .cloud_namespace_generation,
            CloudNamespaceGeneration::LegacyV1
        );
        std::fs::remove_dir(&profile_path).unwrap();
        assert!(matches!(
            fixture.service.connect_cloud_library().await.unwrap(),
            CloudLibraryStatus::Active { .. }
        ));
        fixture.assert_local_protected();
        let profile: crate::config::DeviceProfile =
            serde_json::from_slice(&std::fs::read(profile_path).unwrap()).unwrap();
        assert!(!profile.games.contains_key("pending"));
        assert!(profile.games.contains_key("ready"));
    });
}

#[test]
fn failed_choice_publication_keeps_the_local_definition_for_retry() {
    let _lock = crate::config::lock_config_test_file();
    runtime().block_on(async {
        let fixture = Fixture::new().await;
        fixture.service.connect_cloud_library().await.unwrap();
        let item = fixture
            .service
            .review_pending_definitions()
            .await
            .unwrap()
            .items
            .remove(0);
        let decisions = [JoinGameDecision {
            local_game_id: item.local_game_id,
            local_fingerprint: item.local_fingerprint,
            cloud_fingerprint: item.cloud_fingerprint,
            action: JoinGameAction::KeepCloud,
        }];
        let path = fixture.profile_path();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            fixture
                .service
                .resolve_pending_definitions(&decisions, false)
                .await
                .is_err()
        );
        fixture.assert_local_protected();
        std::fs::remove_dir(&path).unwrap();
        fixture
            .service
            .resolve_pending_definitions(&decisions, false)
            .await
            .unwrap();
        assert!(!cloud_bootstrap_inputs().unwrap().2.is_local_game("pending"));
        assert_eq!(get_config().unwrap().games[0].name, "Cloud version");
    });
}

#[test]
fn deleted_cloud_identity_cannot_remove_pending_local_protection() {
    let _lock = crate::config::lock_config_test_file();
    runtime().block_on(async {
        let fixture = Fixture::new().await;
        fixture.service.connect_cloud_library().await.unwrap();
        let item = fixture
            .service
            .review_pending_definitions()
            .await
            .unwrap()
            .items
            .remove(0);
        let library = SharedLibraryRepository::new(fixture.operator.clone(), 2);
        let before = library.load().await.unwrap();
        let mut after = before.clone();
        after.games.retain(|game| game.storage_key != "pending");
        DeletionRegistryRepository::new(fixture.operator.clone(), 2)
            .mark_game_deleted("pending", "Cloud version", "other-device")
            .await
            .unwrap();
        library.compare_replace(&before, &after).await.unwrap();
        // Both an open selection and a stale AddLocal request must reload the
        // review without resurrecting the deleted identity.
        for action in [JoinGameAction::KeepCloud, JoinGameAction::AddLocal] {
            let decision = JoinGameDecision {
                local_game_id: item.local_game_id.clone(),
                local_fingerprint: item.local_fingerprint.clone(),
                cloud_fingerprint: item.cloud_fingerprint.clone(),
                action,
            };
            assert!(matches!(
                fixture
                    .service
                    .resolve_pending_definitions(&[decision], false)
                    .await
                    .unwrap(),
                CloudLibraryJoinOutcome::ReviewChanged { .. }
            ));
            fixture.assert_local_protected();
        }
        super::super::game_deletion::converge_local_deleted_games()
            .await
            .unwrap();
        fixture.assert_local_protected();
        assert_eq!(library.load().await.unwrap(), after);
        assert!(
            fixture
                .service
                .review_pending_definitions()
                .await
                .unwrap()
                .items
                .is_empty()
        );
    });
}
