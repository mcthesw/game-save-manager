use super::*;
use crate::cloud_sync::v2::{CloudNamespaceDescriptor, V2_NAMESPACE_DESCRIPTOR_PATH};
use crate::config::{
    CloudNamespaceGeneration, Config, ConfigTestStateGuard, ConfigurationOwners, get_config,
};
use crate::hooks::HookPipeline;
use std::sync::Arc;

#[test]
fn joins_existing_library_without_replacing_local_data_or_device_identity() {
    let _lock = crate::config::lock_config_test_file();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let root = temp_dir::TempDir::new().unwrap();
        let local_config = Config {
            games: serde_json::from_value(serde_json::json!([
                {"name":"Private", "storage_key":"private", "save_paths":[]}
            ]))
            .unwrap(),
            ..Config::default()
        };
        let _guard = ConfigTestStateGuard::replace_with(&local_config).unwrap();
        crate::config::set_config_local(&local_config).unwrap();
        let (_, original_profile, state) = cloud_bootstrap_inputs().unwrap();
        let cloud_config = Config {
            games: serde_json::from_value(serde_json::json!([
                {"name":"Shared", "storage_key":"shared", "save_paths":[]}
            ]))
            .unwrap(),
            ..Config::default()
        };
        let remote = ConfigurationOwners::from_legacy(&cloud_config, &"other-device".into());
        let settings = CloudSettings {
            backend: Backend::Fs,
            root_path: root.path().to_string_lossy().into_owned(),
            ..Default::default()
        };
        let operator = CloudSyncSessionConfig::from(&settings).get_op().unwrap();
        let descriptor = CloudNamespaceDescriptor::default();
        CloudLibraryBootstrap::new(operator.clone(), 3)
            .create_empty(
                &descriptor,
                &remote.shared_library,
                &remote.device_profiles["other-device"],
            )
            .await
            .unwrap();
        let before = operator
            .read(super::super::super::cloud_sync::v2::CLOUD_MANIFEST_PATH)
            .await
            .unwrap()
            .to_vec();
        let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));
        inspect_target(&settings, &descriptor.library_id)
            .await
            .unwrap();
        assert_eq!(
            get_config().unwrap().settings.cloud_settings,
            local_config.settings.cloud_settings
        );
        assert!(matches!(
            service
                .import_cloud_join_code("never log this secret", false)
                .await,
            Err(CloudJoinError::ConfirmationRequired)
        ));
        service
            .connect_join_target(&Backend::Fs, &settings.root_path, &descriptor.library_id)
            .await
            .unwrap();
        let (library, profile, joined) = cloud_bootstrap_inputs().unwrap();
        assert_eq!(joined.current_device_id, state.current_device_id);
        assert_eq!(profile.device, original_profile.device);
        assert_eq!(
            profile.local_archive_root,
            original_profile.local_archive_root
        );
        assert!(joined.is_local_game("private"));
        assert!(
            library
                .games
                .iter()
                .any(|game| game.storage_key == "shared")
        );
        assert_eq!(
            joined.cloud_namespace_generation,
            CloudNamespaceGeneration::V2
        );
        assert_eq!(
            joined.cloud_library_id.as_deref(),
            Some(descriptor.library_id.as_str())
        );
        assert_eq!(
            operator
                .read(crate::cloud_sync::v2::CLOUD_MANIFEST_PATH)
                .await
                .unwrap()
                .to_vec(),
            before
        );
    });
}

#[tokio::test]
async fn empty_and_replaced_targets_are_rejected_without_creating_metadata() {
    let root = temp_dir::TempDir::new().unwrap();
    let settings = CloudSettings {
        backend: Backend::Fs,
        root_path: root.path().to_string_lossy().into_owned(),
        ..Default::default()
    };
    let expected = CloudNamespaceDescriptor::default();
    assert!(
        inspect_target(&settings, &expected.library_id)
            .await
            .is_err()
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    let operator = CloudSyncSessionConfig::from(&settings).get_op().unwrap();
    let replacement = serde_json::to_vec(&CloudNamespaceDescriptor::default()).unwrap();
    operator
        .write(V2_NAMESPACE_DESCRIPTOR_PATH, replacement.clone())
        .await
        .unwrap();
    assert!(
        inspect_target(&settings, &expected.library_id)
            .await
            .is_err()
    );
    assert_eq!(
        operator
            .read(V2_NAMESPACE_DESCRIPTOR_PATH)
            .await
            .unwrap()
            .to_vec(),
        replacement
    );
}
