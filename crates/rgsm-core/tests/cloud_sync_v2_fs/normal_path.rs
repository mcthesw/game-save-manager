use rgsm_core::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudLibraryBootstrap, CloudNamespaceClassification,
    DELETION_REGISTRY_PATH, SHARED_LIBRARY_PATH, V2_NAMESPACE_DESCRIPTOR_PATH, device_profile_path,
};

use crate::support::{DeviceFixture, FsCloudFixture, MAX_ATTEMPTS};

#[tokio::test]
async fn bootstrap_persists_complete_v2_namespace_across_fresh_operators() {
    let cloud = FsCloudFixture::new();
    let device = DeviceFixture::new("device-a");
    let (shared_library, device_profile) = device.empty_library_and_profile();

    CloudLibraryBootstrap::new(cloud.new_operator(), MAX_ATTEMPTS)
        .create_empty(&shared_library, &device_profile)
        .await
        .expect("empty Fs root should bootstrap");

    let fresh_operator = cloud.new_operator();
    let classification = CloudLibraryBootstrap::new(fresh_operator.clone(), MAX_ATTEMPTS)
        .inspect()
        .await
        .expect("persisted namespace should classify");

    let CloudNamespaceClassification::SupportedV2 {
        descriptor,
        shared_library: stored_library,
        manifest,
    } = classification
    else {
        panic!("bootstrapped Fs root should classify as V2");
    };
    assert_eq!(descriptor, Default::default());
    assert_eq!(stored_library, shared_library);
    assert_eq!(manifest, Default::default());

    for path in [
        V2_NAMESPACE_DESCRIPTOR_PATH,
        SHARED_LIBRARY_PATH,
        CLOUD_MANIFEST_PATH,
        DELETION_REGISTRY_PATH,
        &device_profile_path(&device.id),
    ] {
        assert!(
            fresh_operator
                .exists(path)
                .await
                .expect("required object existence should be readable"),
            "required V2 object should persist: {path}"
        );
    }

    assert!(!cloud.root().join("game-save-manager").exists());
}
