use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};
use std::sync::{Arc, Mutex};

use rgsm_core::backup::ArchiveFormat;
use rgsm_core::cloud_sync::Backend;
use rgsm_core::cloud_sync::v2::{
    CloudLibraryBootstrap, CloudLibraryCutover, CloudLibraryCutoverError, CloudLibraryJoin,
    CloudNamespaceClassification, CloudNamespaceDescriptor, CloudNamespaceError,
    DeviceProfileRepository, V2_NAMESPACE_DESCRIPTOR_PATH, cloud_archive_path,
};
use rgsm_core::config::{Config, ConfigurationOwners};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

const ANCESTORS_NOT_FOUND: &str = r#"<?xml version="1.0"?><d:error xmlns:d="DAV:" xmlns:s="http://ns.jianguoyun.com"><s:exception>AncestorsNotFound</s:exception></d:error>"#;

struct State {
    directories: BTreeSet<String>,
    files: BTreeMap<String, Vec<u8>>,
    mutations: Vec<(String, String)>,
}

struct WebDav {
    endpoint: String,
    state: Arc<Mutex<State>>,
    task: JoinHandle<()>,
}

impl Drop for WebDav {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl WebDav {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State {
            directories: BTreeSet::from(["/".into()]),
            files: BTreeMap::new(),
            mutations: Vec::new(),
        }));
        let server_state = state.clone();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let state = server_state.clone();
                tokio::spawn(serve(stream, state));
            }
        });
        Self {
            endpoint,
            state,
            task,
        }
    }

    fn operator(&self) -> opendal::Operator {
        Backend::WebDAV {
            endpoint: self.endpoint.clone(),
            username: "test".into(),
            password: "test".into(),
        }
        .get_op_with_root("fresh-root")
        .unwrap()
    }
}

async fn serve(mut stream: TcpStream, state: Arc<Mutex<State>>) {
    let mut request = Vec::new();
    let header_end = loop {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).await.unwrap();
        if count == 0 {
            return;
        }
        request.extend_from_slice(&chunk[..count]);
        if let Some(index) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
    let mut first = headers.lines().next().unwrap().split_whitespace();
    let method = first.next().unwrap();
    let path = first.next().unwrap();
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while request.len() < header_end + length {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0);
        request.extend_from_slice(&chunk[..count]);
    }
    let (status, body) = respond(
        &mut state.lock().unwrap(),
        method,
        path,
        &request[header_end..],
        &headers,
    );
    let response = format!(
        "HTTP/1.1 {status} Response\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(response.as_bytes()).await.unwrap();
    stream.write_all(&body).await.unwrap();
}

fn parent(path: &str) -> &str {
    &path[..path.trim_end_matches('/').rfind('/').unwrap() + 1]
}

fn respond(
    state: &mut State,
    method: &str,
    path: &str,
    body: &[u8],
    headers: &str,
) -> (u16, Vec<u8>) {
    let directory = format!("{}/", path.trim_end_matches('/'));
    match method {
        "GET" => match state.files.get(path) {
            Some(bytes) => (200, bytes.clone()),
            None if !state.directories.contains("/fresh-root/") => {
                (409, ANCESTORS_NOT_FOUND.as_bytes().to_vec())
            }
            None => (404, Vec::new()),
        },
        "PROPFIND" => {
            if !state.directories.contains(&directory) && !state.files.contains_key(path) {
                return if !state.directories.contains("/fresh-root/") {
                    (409, ANCESTORS_NOT_FOUND.as_bytes().to_vec())
                } else {
                    (404, Vec::new())
                };
            }
            let mut entries = vec![if state.directories.contains(&directory) {
                directory.clone()
            } else {
                path.into()
            }];
            let depth_zero = headers
                .lines()
                .any(|line| line.eq_ignore_ascii_case("depth: 0"));
            if state.directories.contains(&directory) && !depth_zero {
                entries.extend(
                    state
                        .directories
                        .iter()
                        .chain(state.files.keys())
                        .filter(|entry| {
                            entry.as_str() != directory
                                && entry.starts_with(&directory)
                                && !entry[directory.len()..].trim_end_matches('/').contains('/')
                        })
                        .cloned(),
                );
            }
            let mut xml = String::from("<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\">");
            for entry in entries {
                let collection = if state.directories.contains(&entry) {
                    "<D:collection/>"
                } else {
                    ""
                };
                xml.push_str(&format!("<D:response><D:href>{entry}</D:href><D:propstat><D:prop><D:resourcetype>{collection}</D:resourcetype><D:getcontentlength>{}</D:getcontentlength><D:getlastmodified>Mon, 05 Oct 2026 00:00:00 GMT</D:getlastmodified></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>", state.files.get(&entry).map_or(0, Vec::len)));
            }
            xml.push_str("</D:multistatus>");
            (207, xml.into_bytes())
        }
        "MKCOL" => {
            if state.directories.contains(&directory) {
                return (405, Vec::new());
            }
            if !state.directories.contains(parent(&directory)) {
                return (409, Vec::new());
            }
            state.directories.insert(directory);
            state.mutations.push((method.into(), path.into()));
            (201, Vec::new())
        }
        "PUT" => {
            if !state.directories.contains(parent(path)) {
                return (409, Vec::new());
            }
            state.files.insert(path.into(), body.to_vec());
            state.mutations.push((method.into(), path.into()));
            (201, Vec::new())
        }
        _ => (405, Vec::new()),
    }
}

#[tokio::test]
async fn missing_jianguoyun_root_is_read_only_then_bootstraps_and_transfers() {
    let server = WebDav::start().await;
    let operator = server.operator();
    let bootstrap = CloudLibraryBootstrap::new(operator.clone(), 1);
    assert!(matches!(
        bootstrap.inspect().await.unwrap(),
        CloudNamespaceClassification::Empty
    ));
    assert!(server.state.lock().unwrap().mutations.is_empty());

    let owners = ConfigurationOwners::from_legacy(&Config::default(), &"device".into());
    let descriptor = CloudNamespaceDescriptor::default();
    bootstrap
        .create_empty(
            &descriptor,
            &owners.shared_library,
            &owners.device_profiles["device"],
        )
        .await
        .unwrap();
    assert!(matches!(
        bootstrap.inspect().await.unwrap(),
        CloudNamespaceClassification::SupportedV2 { .. }
    ));
    assert_eq!(
        server.state.lock().unwrap().mutations.last().unwrap().1,
        format!("/fresh-root/{V2_NAMESPACE_DESCRIPTOR_PATH}")
    );

    let archive = vec![42; 128 * 1024];
    operator
        .write("v2/archives/game/snapshot.zip", archive.clone())
        .await
        .unwrap();
    assert_eq!(
        operator
            .read("v2/archives/game/snapshot.zip")
            .await
            .unwrap()
            .to_vec(),
        archive
    );
}

#[tokio::test]
async fn existing_empty_partial_and_unknown_roots_are_distinct_and_read_only() {
    let server = WebDav::start().await;
    server
        .state
        .lock()
        .unwrap()
        .directories
        .insert("/fresh-root/".into());
    let bootstrap = CloudLibraryBootstrap::new(server.operator(), 1);
    assert!(matches!(
        bootstrap.inspect().await.unwrap(),
        CloudNamespaceClassification::Empty
    ));
    server
        .state
        .lock()
        .unwrap()
        .files
        .insert("/fresh-root/foreign.txt".into(), b"keep".to_vec());
    assert!(matches!(
        bootstrap.inspect().await,
        Err(
            rgsm_core::cloud_sync::v2::CloudLibraryBootstrapError::Namespace(
                CloudNamespaceError::UnrecognizedRoot(_)
            )
        )
    ));
    {
        let mut state = server.state.lock().unwrap();
        state.directories.insert("/fresh-root/v2/".into());
        state
            .files
            .insert("/fresh-root/v2/shared-library.json".into(), b"{}".to_vec());
    }
    assert!(matches!(
        bootstrap.inspect().await,
        Err(
            rgsm_core::cloud_sync::v2::CloudLibraryBootstrapError::Namespace(
                CloudNamespaceError::PartialV2(_)
            )
        )
    ));
    assert!(server.state.lock().unwrap().mutations.is_empty());
}

#[tokio::test]
async fn v1_8_copy_upgrades_on_a_then_b_joins_with_its_own_absolute_paths() {
    let server = WebDav::start().await;
    let local = temp_dir::TempDir::new().unwrap();
    let mut raw: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/config-upgrade/config_v1_8_0.json")).unwrap();
    raw["games"][0]["name"] = "ExampleGame".into();
    // Model the copied 1.8 package after both machines edited their concrete paths.
    raw["games"][0]["save_paths"] = serde_json::json!([{
        "id": 11, "unit_type": "Folder", "delete_before_apply": true,
        "paths": {"desktop-gamma": "C:/A/Saved", "device-b": "D:/B/Saved"}
    }]);
    raw["devices"]["device-b"] = serde_json::json!({"id": "device-b", "name": "B"});
    raw["games"][0]["cloud_sync_enabled"] = true.into();
    raw["settings"]["cloud_settings"]["backend"] = serde_json::json!({"type": "WebDAV", "endpoint": server.endpoint, "username": "test", "password": "test"});
    raw["settings"]["cloud_settings"]["root_path"] = "/fresh-root".into();
    let original_cloud_config = serde_json::to_vec(&raw).unwrap();

    let mut configs = Vec::new();
    for device in ["a", "b"] {
        let directory = local.path().join(device);
        std::fs::create_dir_all(&directory).unwrap();
        raw["backup_path"] = directory
            .join("save_data")
            .to_string_lossy()
            .into_owned()
            .into();
        let path = directory.join("GameSaveManager.config.json");
        std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(rgsm_core::updater::update_config(&path).unwrap());
        assert!(path.with_extension("json.bak").is_file());
        assert!(!rgsm_core::updater::update_config(&path).unwrap());
        let config: Config = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            config.games[0].save_paths[0]
                .get_path_for_device(&"desktop-gamma".into())
                .unwrap(),
            "C:/A/Saved"
        );
        assert_eq!(
            config.games[0].save_paths[0]
                .get_path_for_device(&"device-b".into())
                .unwrap(),
            "D:/B/Saved"
        );
        configs.push(config);
    }
    let owners_a = ConfigurationOwners::from_legacy(&configs[0], &"desktop-gamma".into());
    let owners_b = ConfigurationOwners::from_legacy(&configs[1], &"device-b".into());
    let game_id = &owners_a.shared_library.games[0].storage_key;
    assert_eq!(game_id, "ExampleGame");
    assert_eq!(owners_b.shared_library, owners_a.shared_library);
    assert_ne!(
        owners_a.local_state.current_device_id,
        owners_b.local_state.current_device_id
    );

    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(
            "11/Saved/profile.sav",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    writer.write_all(b"saved by 1.8 on A").unwrap();
    let archive = writer.finish().unwrap().into_inner();
    let snapshot_id = "2025-01-02_03-04-05";
    let metadata = serde_json::to_vec(&serde_json::json!({
        "name": "ExampleGame", "backups": [{"date": snapshot_id, "describe": "1.8 backup", "path": format!("save_data/ExampleGame/{snapshot_id}.zip"), "size": archive.len(), "device_id": "desktop-gamma"}],
        "device_heads": {"desktop-gamma": snapshot_id}, "sync_version": 3, "last_sync_device": "desktop-gamma"
    })).unwrap();
    {
        let mut state = server.state.lock().unwrap();
        state.directories.extend([
            "/fresh-root/".into(),
            "/fresh-root/save_data/".into(),
            "/fresh-root/save_data/ExampleGame/".into(),
        ]);
        state.files.insert(
            "/fresh-root/GameSaveManager.config.json".into(),
            original_cloud_config.clone(),
        );
        state.files.insert(
            "/fresh-root/save_data/ExampleGame/Backups.json".into(),
            metadata.clone(),
        );
        state.files.insert(
            format!("/fresh-root/save_data/ExampleGame/{snapshot_id}.zip"),
            archive.clone(),
        );
    }
    let operator = server.operator();
    let bootstrap = CloudLibraryBootstrap::new(operator.clone(), 1);
    assert!(matches!(
        bootstrap.inspect().await.unwrap(),
        CloudNamespaceClassification::V1Only { .. }
    ));
    let cutover = CloudLibraryCutover::new(
        operator.clone(),
        local.path().join("a/save_data"),
        local.path().join("cutover.json"),
        "desktop-gamma".into(),
        owners_a.device_profiles["desktop-gamma"].clone(),
        1,
    );
    assert_eq!(cutover.review().await.unwrap().snapshot_count, 1);
    let upgraded = cutover.execute().await.unwrap();
    assert_eq!(upgraded.snapshot_count, 1);
    assert_eq!(upgraded.unavailable_archives, 0);
    let v2_archive = cloud_archive_path(game_id, snapshot_id, ArchiveFormat::Zip, None).unwrap();
    assert_eq!(operator.read(&v2_archive).await.unwrap().to_vec(), archive);
    cutover.finish().await.unwrap();

    // B still has a locally upgraded 1.8 configuration. It must join A's V2
    // identity, rather than trying to cut over or create a second namespace.
    assert!(matches!(
        bootstrap.inspect().await.unwrap(),
        CloudNamespaceClassification::SupportedV2 { .. }
    ));
    assert!(matches!(
        cutover.review().await,
        Err(CloudLibraryCutoverError::CutoverUnavailable)
    ));
    let join = CloudLibraryJoin::new(operator.clone(), 1);
    let review = join.review(&owners_b.shared_library).await.unwrap();
    assert_eq!(review.cloud_game_count, 1);
    let joined = join
        .join(
            &owners_b.shared_library,
            &owners_b.device_profiles["device-b"],
            &[],
            false,
        )
        .await
        .unwrap();
    assert_eq!(joined.library_id, upgraded.library_id);
    assert_eq!(joined.shared_library, upgraded.shared_library);
    let profiles = DeviceProfileRepository::new(operator, 1)
        .list()
        .await
        .unwrap();
    let a = profiles
        .iter()
        .find(|profile| profile.device.id == "desktop-gamma")
        .unwrap();
    let b = profiles
        .iter()
        .find(|profile| profile.device.id == "device-b")
        .unwrap();
    assert_eq!(
        a.games[game_id].save_units[&11].path.as_deref(),
        Some("C:/A/Saved")
    );
    assert_eq!(
        b.games[game_id].save_units[&11].path.as_deref(),
        Some("D:/B/Saved")
    );
    let state = server.state.lock().unwrap();
    assert_eq!(
        state.files["/fresh-root/GameSaveManager.config.json"],
        original_cloud_config
    );
    assert_eq!(
        state.files["/fresh-root/save_data/ExampleGame/Backups.json"],
        metadata
    );
    assert_eq!(
        state.files[&format!("/fresh-root/save_data/ExampleGame/{snapshot_id}.zip")],
        archive
    );
}

#[tokio::test]
async fn reset_both_devices_and_cloud_initializes_once_then_other_device_joins() {
    let server = WebDav::start().await;
    let operator = server.operator();
    let bootstrap = CloudLibraryBootstrap::new(operator.clone(), 1);
    let a = ConfigurationOwners::from_legacy(&Config::default(), &"a".into());
    let b = ConfigurationOwners::from_legacy(&Config::default(), &"b".into());
    let descriptor = CloudNamespaceDescriptor::default();
    bootstrap
        .create_empty(&descriptor, &a.shared_library, &a.device_profiles["a"])
        .await
        .unwrap();
    let join = CloudLibraryJoin::new(operator, 1);
    let joined = join
        .join(&b.shared_library, &b.device_profiles["b"], &[], false)
        .await
        .unwrap();
    assert_eq!(joined.library_id, descriptor.library_id);
    let other_descriptor = CloudNamespaceDescriptor::default();
    assert!(
        bootstrap
            .create_empty(
                &other_descriptor,
                &b.shared_library,
                &b.device_profiles["b"]
            )
            .await
            .is_err()
    );
    let CloudNamespaceClassification::SupportedV2 {
        descriptor: stored, ..
    } = bootstrap.inspect().await.unwrap()
    else {
        panic!("library must remain supported");
    };
    assert_eq!(stored, descriptor);
}
