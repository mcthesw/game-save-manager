use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use rgsm_core::backup::{
    CreatedBy, Game, GameDraft, SaveUnitDraft, SaveUnitType, StoreGameId, list_extra_backups,
};
use rgsm_core::cloud_sync::{
    Backend, CloudBackendCheckOutcome, CloudSettings, CloudSyncSessionConfig, CloudSyncTaskManager,
    ConflictResolution, S3AddressingStyle,
};
use rgsm_core::config::{get_config, set_config_local};
use rgsm_core::device::{Device, DeviceResourceKind, DeviceResourceSource, get_current_device_id};
use rgsm_core::hooks::HookSource;
use rgsm_core::ludusavi_manifest;
use rgsm_core::path_pattern::StoreKind;
use rgsm_core::services::ServiceContext;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use crate::hooks::{TuiRestoreNotifier, build_pipeline};
use crate::logging::SessionLog;
use crate::model::{AppData, ImportCandidate, OperationEvent};
use crate::profile_import;
use crate::tui_settings::TuiSettings;

mod vn;

#[derive(Debug, Clone)]
pub enum Operation {
    AddGame(String),
    RenameGame(Game, String),
    AddSaveUnitPath(Game, String),
    EditSelectedPath(Game, usize, String),
    DeleteGame(Game),
    CreateSnapshot(Game, String, Option<String>),
    RestoreSnapshot(Game, String),
    DeleteSnapshot(Game, String),
    BatchDeleteSnapshots(Game, Vec<String>),
    EditSnapshotDescription(Game, String, String),
    SetCurrentPosition(Game, String),
    DetachSnapshot(Game, String),
    ToggleCloudSync(Game),
    CheckCloud,
    UploadAll,
    DownloadAll,
    SyncGame(String),
    SyncAll,
    ResolveConflict(String, ConflictResolution),
    SaveCloudSettings(CloudSettings),
    UpdateManifest,
    ResetManifest,
    ImportGuiProfile(String),
    ImportGame {
        name: String,
        save_paths: Vec<String>,
    },
    ReloadData,
    UpdateCurrentDeviceName(String),
    AddCurrentDeviceRoot(String),
    AddVnScanRoot(String),
    ImportVnGames(Vec<GameDraft>),
}

pub async fn load_data(settings: &TuiSettings) -> Result<AppData> {
    ensure_current_device_registered()?;
    let config = get_config()?;
    let games = config.games.clone();
    let selected_snapshots = games
        .first()
        .and_then(|game| game.get_game_snapshots_info().ok());
    let sync_state = rgsm_core::cloud_sync::sync_state::load_sync_state().unwrap_or_default();
    let manifest_status = ludusavi_manifest::get_manifest_status();
    let importable_games = load_importable_games(&config, settings.ludusavi_local_only)
        .await
        .unwrap_or_default();

    Ok(AppData {
        config,
        games,
        selected_snapshots,
        sync_state,
        manifest_status,
        importable_games,
    })
}

pub fn ensure_current_device_registered() -> Result<()> {
    let mut config = get_config()?;
    let id = get_current_device_id().clone();
    if config.devices.contains_key(&id) {
        return Ok(());
    }
    config.devices.insert(id, Device::default());
    set_config_local(&config)?;
    Ok(())
}

async fn load_importable_games(
    config: &rgsm_core::config::Config,
    local_only: bool,
) -> Result<Vec<ImportCandidate>> {
    let manifest = ludusavi_manifest::fetch_manifest().await?;
    let managed_games = config
        .games
        .iter()
        .map(|game| game.name.clone())
        .collect::<Vec<_>>();
    let games =
        ludusavi_manifest::parse_manifest_games(&manifest, &managed_games, local_only, config);
    Ok(games
        .into_iter()
        .map(|game| {
            let save_paths = manifest
                .get(&game.name)
                .and_then(|value| ludusavi_manifest::extract_save_paths(&game.name, value).ok())
                .map(|paths| paths.into_iter().map(|path| path.path).collect())
                .unwrap_or_default();
            ImportCandidate { game, save_paths }
        })
        .collect())
}

pub fn submit_operation(
    tx: UnboundedSender<OperationEvent>,
    settings: TuiSettings,
    log: Arc<Mutex<SessionLog>>,
    cloud_sync_manager: Arc<CloudSyncTaskManager>,
    cancel_token: CancellationToken,
    operation: Operation,
) {
    tokio::spawn(async move {
        let description = operation.description();
        let started_at = Instant::now();
        let _ = tx.send(OperationEvent::Started(description.clone()));
        let result = run_operation(
            operation,
            &settings,
            Arc::clone(&log),
            cloud_sync_manager,
            cancel_token,
        )
        .await;
        match result {
            Ok(message) => {
                let detail = format!(
                    "{description} finished in {}: {message}",
                    format_elapsed(started_at.elapsed())
                );
                let _ = tx.send(OperationEvent::Finished {
                    status: message,
                    detail,
                });
                match load_data(&settings).await {
                    Ok(data) => {
                        let _ = tx.send(OperationEvent::DataReloaded(Box::new(data)));
                    }
                    Err(err) => {
                        let _ = tx.send(OperationEvent::Failed(format!("refresh failed: {err:#}")));
                    }
                }
            }
            Err(err) => {
                let _ = tx.send(OperationEvent::Failed(format!(
                    "{description} failed in {}: {err:#}",
                    format_elapsed(started_at.elapsed())
                )));
            }
        }
    });
}

impl Operation {
    fn description(&self) -> String {
        match self {
            Operation::AddGame(name) => format!("add game {name}"),
            Operation::RenameGame(game, _) => format!("rename game {}", game.name),
            Operation::AddSaveUnitPath(game, _) | Operation::EditSelectedPath(game, _, _) => {
                format!("update save units for {}", game.name)
            }
            Operation::DeleteGame(game) => format!("delete game {}", game.name),
            Operation::CreateSnapshot(game, _, _) => format!("create snapshot for {}", game.name),
            Operation::RestoreSnapshot(game, date) => format!("restore {} / {date}", game.name),
            Operation::DeleteSnapshot(game, date) => format!("delete {} / {date}", game.name),
            Operation::BatchDeleteSnapshots(game, dates) => {
                format!("delete {} snapshots for {}", dates.len(), game.name)
            }
            Operation::EditSnapshotDescription(game, date, _) => {
                format!("edit {} / {date}", game.name)
            }
            Operation::SetCurrentPosition(game, date) => {
                format!("set {} position to {date}", game.name)
            }
            Operation::DetachSnapshot(game, date) => format!("detach {} / {date}", game.name),
            Operation::ToggleCloudSync(game) => format!("toggle cloud sync for {}", game.name),
            Operation::CheckCloud => "check cloud backend".to_string(),
            Operation::UploadAll => "upload all cloud data".to_string(),
            Operation::DownloadAll => "download all cloud data".to_string(),
            Operation::SyncGame(name) => format!("sync game {name}"),
            Operation::SyncAll => "sync all enabled games".to_string(),
            Operation::ResolveConflict(name, resolution) => {
                format!("resolve cloud conflict for {name} ({resolution:?})")
            }
            Operation::SaveCloudSettings(_) => "save cloud settings".to_string(),
            Operation::UpdateManifest => "update detection database".to_string(),
            Operation::ResetManifest => "reset detection database".to_string(),
            Operation::ImportGuiProfile(_) => "import GUI profile".to_string(),
            Operation::ImportGame { name, .. } => format!("import {name} from detection database"),
            Operation::ReloadData => "refresh RGSM data".to_string(),
            Operation::UpdateCurrentDeviceName(_) => "update current device name".to_string(),
            Operation::AddCurrentDeviceRoot(_) => "add current device game root".to_string(),
            Operation::AddVnScanRoot(_) => "add VN scan directory".to_string(),
            Operation::ImportVnGames(drafts) => {
                format!("import {} detected VN games", drafts.len())
            }
        }
    }
}

fn format_elapsed(duration: Duration) -> String {
    if duration.as_secs() > 0 {
        format!("{}.{:03}s", duration.as_secs(), duration.subsec_millis())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

async fn run_operation(
    operation: Operation,
    settings: &TuiSettings,
    log: Arc<Mutex<SessionLog>>,
    cloud_sync_manager: Arc<CloudSyncTaskManager>,
    cancel_token: CancellationToken,
) -> Result<String> {
    let mut config = get_config()?;
    let service = ServiceContext::new(build_pipeline(&config, settings, cloud_sync_manager));

    match operation {
        Operation::AddGame(name) => {
            service
                .add_game(&empty_game_draft(&name), HookSource::UserManual)
                .await?;
            Ok(format!("added game {name}"))
        }
        Operation::RenameGame(game, name) => {
            let draft = draft_from_game(&game, Some(name.clone()));
            service
                .update_game(&game.storage_key, &draft, HookSource::UserManual)
                .await?;
            Ok(format!("renamed game to {name}"))
        }
        Operation::AddSaveUnitPath(game, path) => {
            let mut draft = draft_from_game(&game, None);
            let mut paths = HashMap::new();
            paths.insert(get_current_device_id().clone(), path);
            draft.save_paths.push(SaveUnitDraft {
                id: None,
                unit_type: SaveUnitType::Folder,
                paths,
                delete_before_apply: false,
                enabled: true,
            });
            service
                .update_game(&game.storage_key, &draft, HookSource::UserManual)
                .await?;
            Ok(format!("added save unit to {}", game.name))
        }
        Operation::EditSelectedPath(game, index, path) => {
            let mut draft = draft_from_game(&game, None);
            let Some(unit) = draft.save_paths.get_mut(index) else {
                return Err(anyhow!("save unit index out of range"));
            };
            unit.paths.insert(get_current_device_id().clone(), path);
            service
                .update_game(&game.storage_key, &draft, HookSource::UserManual)
                .await?;
            Ok(format!("updated save unit for {}", game.name))
        }
        Operation::DeleteGame(game) => {
            service.delete_game(&game, HookSource::UserManual).await?;
            Ok(format!("deleted game {}", game.name))
        }
        Operation::CreateSnapshot(game, describe, parent) => {
            service
                .create_snapshot_at(
                    &game,
                    &describe,
                    parent,
                    CreatedBy::Manual,
                    HookSource::UserManual,
                )
                .await?;
            Ok(format!("created snapshot for {}", game.name))
        }
        Operation::RestoreSnapshot(game, date) => {
            let notifier = TuiRestoreNotifier::new(log);
            service
                .restore_snapshot(&game, &date, HookSource::UserManual, Some(&notifier))
                .await?;
            Ok(format!("restored {} / {date}", game.name))
        }
        Operation::DeleteSnapshot(game, date) => {
            service
                .delete_snapshot(&game, &date, HookSource::UserManual)
                .await?;
            Ok(format!("deleted {} / {date}", game.name))
        }
        Operation::BatchDeleteSnapshots(game, dates) => {
            service
                .batch_delete_snapshots(&game, &dates, HookSource::UserManual)
                .await?;
            Ok(format!("deleted {} snapshots", dates.len()))
        }
        Operation::EditSnapshotDescription(game, date, describe) => {
            service
                .set_snapshot_description(&game, &date, &describe, HookSource::UserManual)
                .await?;
            Ok(format!("updated description for {date}"))
        }
        Operation::SetCurrentPosition(game, date) => {
            service
                .set_snapshot_head(&game, &date, HookSource::UserManual)
                .await?;
            Ok(format!("set current position to {date}"))
        }
        Operation::DetachSnapshot(game, date) => {
            service
                .detach_snapshot(&game, &date, HookSource::UserManual)
                .await?;
            Ok(format!("detached {date}"))
        }
        Operation::ToggleCloudSync(mut game) => {
            game.cloud_sync_enabled = !game.cloud_sync_enabled;
            let draft = draft_from_game(&game, None);
            service
                .update_game(&game.storage_key, &draft, HookSource::UserManual)
                .await?;
            Ok(format!("cloud sync toggled for {}", game.name))
        }
        Operation::CheckCloud => {
            let session = CloudSyncSessionConfig::from(&config.settings.cloud_settings);
            let report = service.check_cloud_backend(&session).await?;
            if let Some(message) = report.blocking_error_message() {
                return Err(anyhow!(message));
            }
            let message = match report.outcome {
                CloudBackendCheckOutcome::Available => "cloud backend check passed",
                CloudBackendCheckOutcome::Degraded => "cloud backend check passed with warnings",
                CloudBackendCheckOutcome::Unavailable => "cloud backend check failed",
            };
            Ok(message.to_string())
        }
        Operation::UploadAll => {
            let session = CloudSyncSessionConfig::from(&config.settings.cloud_settings);
            let report = service
                .upload_all_from_session(&session, Some(cancel_token.clone()))
                .await?;
            Ok(format!(
                "upload complete: config={:?}, games={}",
                report.config.status,
                report.games.len()
            ))
        }
        Operation::DownloadAll => {
            let session = CloudSyncSessionConfig::from(&config.settings.cloud_settings);
            let report = service
                .download_all_from_session(&session, Some(cancel_token.clone()))
                .await?;
            Ok(format!(
                "download complete: config={:?}, games={}",
                report.config.status,
                report.games.len()
            ))
        }
        Operation::SyncGame(name) => {
            let outcome = service.sync_game(&name).await?;
            Ok(format!("sync {name}: {outcome:?}"))
        }
        Operation::SyncAll => {
            let mut count = 0usize;
            for game in config.games.iter().filter(|game| game.cloud_sync_enabled) {
                if cancel_token.is_cancelled() {
                    return Ok("cloud sync cancelled".to_string());
                }
                service.sync_game(&game.name).await?;
                count += 1;
            }
            Ok(format!("synced {count} enabled games"))
        }
        Operation::ResolveConflict(name, resolution) => {
            let outcome = service.resolve_game_conflict(&name, resolution).await?;
            Ok(format!("conflict resolved: {outcome:?}"))
        }
        Operation::SaveCloudSettings(settings) => {
            config.settings.cloud_settings = settings;
            set_config_local(&config)?;
            Ok("cloud settings saved".to_string())
        }
        Operation::UpdateManifest => {
            let status = ludusavi_manifest::update_manifest_from_remote().await?;
            Ok(format!("manifest updated: {status:?}"))
        }
        Operation::ResetManifest => {
            let status = ludusavi_manifest::reset_manifest_to_bundled()?;
            Ok(format!("manifest reset: {status:?}"))
        }
        Operation::ImportGuiProfile(path) => {
            let data_dir = rgsm_core::app_dirs::get_app_data_dir().clone();
            let report =
                profile_import::import_gui_profile(std::path::Path::new(&path), &data_dir)?;
            Ok(format!(
                "imported GUI profile: {} games, {} backup files copied, {} skipped from {}",
                report.games,
                report.copied_backup_files,
                report.skipped_backup_files,
                report.source_backup_path.display()
            ))
        }
        Operation::ImportGame { name, save_paths } => {
            import_ludusavi_game(&service, &name, save_paths).await?;
            Ok(format!("imported {name}"))
        }
        Operation::ReloadData => Ok("data refreshed".to_string()),
        Operation::UpdateCurrentDeviceName(name) => {
            let name = name.trim();
            if name.is_empty() {
                return Err(anyhow!("device name cannot be empty"));
            }
            let id = get_current_device_id().clone();
            config.devices.entry(id).or_default().name = name.to_string();
            set_config_local(&config)?;
            Ok("device name updated".to_string())
        }
        Operation::AddCurrentDeviceRoot(path) => {
            let path = path.trim();
            if path.is_empty() {
                return Err(anyhow!("game root cannot be empty"));
            }
            let id = get_current_device_id().clone();
            let device = config.devices.entry(id).or_default();
            if !device.game_root_paths().any(|root| root == path) {
                device.add_resource(
                    DeviceResourceSource::Manual,
                    DeviceResourceKind::GameRoot {
                        store: StoreKind::Other,
                        path: path.to_string(),
                    },
                );
                set_config_local(&config)?;
            }
            Ok(format!("game root added: {path}"))
        }
        Operation::AddVnScanRoot(path) => {
            let path = path.trim();
            if path.is_empty() {
                return Err(anyhow!("VN scan directory cannot be empty"));
            }
            if !config.settings.vn_scan_dirs.iter().any(|root| root == path) {
                config.settings.vn_scan_dirs.push(path.to_string());
                set_config_local(&config)?;
            }
            Ok(format!("VN scan directory added: {path}"))
        }
        Operation::ImportVnGames(drafts) => {
            let count = vn::import_vn_games(&service, drafts).await?;
            Ok(format!("imported {count} detected VN games"))
        }
    }
}

fn empty_game_draft(name: &str) -> GameDraft {
    GameDraft {
        name: name.to_string(),
        save_paths: Vec::new(),
        game_paths: HashMap::new(),
        ludusavi_meta: None,
        device_bindings: HashMap::new(),
    }
}

fn draft_from_game(game: &Game, name_override: Option<String>) -> GameDraft {
    GameDraft {
        name: name_override.unwrap_or_else(|| game.name.clone()),
        save_paths: game
            .save_paths
            .iter()
            .map(|unit| SaveUnitDraft {
                id: Some(unit.id),
                unit_type: unit.unit_type.clone(),
                paths: unit.paths.clone(),
                delete_before_apply: unit.delete_before_apply,
                enabled: unit.enabled,
            })
            .collect(),
        game_paths: game.game_paths.clone(),
        ludusavi_meta: game.ludusavi_meta.clone(),
        device_bindings: game.device_bindings.clone(),
    }
}

async fn import_ludusavi_game(
    service: &ServiceContext,
    name: &str,
    save_paths: Vec<String>,
) -> Result<()> {
    let config = get_config()?;
    if config
        .games
        .iter()
        .any(|game| game.name.eq_ignore_ascii_case(name))
    {
        return Err(anyhow!("game already managed"));
    }
    let manifest = ludusavi_manifest::fetch_manifest().await?;
    let value = manifest
        .get(name)
        .ok_or_else(|| anyhow!("manifest game not found"))?;
    let paths = if save_paths.is_empty() {
        ludusavi_manifest::extract_save_paths(name, value)?
            .into_iter()
            .map(|save_path| save_path.path)
            .collect()
    } else {
        save_paths
    };
    let save_paths = paths
        .into_iter()
        .map(|path| {
            let mut paths = HashMap::new();
            paths.insert(get_current_device_id().clone(), path.clone());
            SaveUnitDraft {
                id: None,
                unit_type: if path.starts_with("REGISTRY:") {
                    SaveUnitType::WinRegistry
                } else {
                    SaveUnitType::Folder
                },
                paths,
                delete_before_apply: config.settings.default_delete_before_apply,
                enabled: true,
            }
        })
        .collect();
    let draft = GameDraft {
        name: name.to_string(),
        save_paths,
        game_paths: HashMap::new(),
        ludusavi_meta: Some(rgsm_core::backup::LudusaviMeta {
            install_dirs: ludusavi_manifest::extract_install_dirs(value),
            store_game_ids: ludusavi_manifest::extract_steam_id(value)
                .map(|id| StoreGameId {
                    store: StoreKind::Steam,
                    id: id.to_string(),
                })
                .into_iter()
                .collect(),
        }),
        device_bindings: HashMap::new(),
    };
    service.add_game(&draft, HookSource::UserManual).await?;
    Ok(())
}

pub fn selected_extra_backup_count(game: &Game) -> usize {
    list_extra_backups(game)
        .map(|items| items.len())
        .unwrap_or_default()
}

pub fn parse_cloud_settings_draft(input: &str, current: &CloudSettings) -> Result<CloudSettings> {
    let input = input.trim();
    let Some((kind, rest)) = split_cloud_draft_kind(input) else {
        return Err(anyhow!("cloud backend draft is empty"));
    };
    let fields = parse_cloud_draft_fields(rest);

    let mut settings = current.clone();
    settings.root_path = fields
        .get("root")
        .or_else(|| fields.get("root_path"))
        .cloned()
        .unwrap_or_else(|| current.root_path.clone());
    settings.max_concurrency = fields
        .get("max")
        .or_else(|| fields.get("max_concurrency"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(current.max_concurrency)
        .max(1);

    settings.backend = match kind.to_ascii_lowercase().as_str() {
        "disabled" | "off" | "none" => Backend::Disabled,
        "webdav" => Backend::WebDAV {
            endpoint: field_or_current(
                &fields,
                "endpoint",
                match &current.backend {
                    Backend::WebDAV { endpoint, .. } => endpoint,
                    _ => "",
                },
            )?,
            username: field_or_current(
                &fields,
                "username",
                match &current.backend {
                    Backend::WebDAV { username, .. } => username,
                    _ => "",
                },
            )?,
            password: field_or_current(
                &fields,
                "password",
                match &current.backend {
                    Backend::WebDAV { password, .. } => password,
                    _ => "",
                },
            )?,
        },
        "s3" => Backend::S3 {
            endpoint: field_or_current(
                &fields,
                "endpoint",
                match &current.backend {
                    Backend::S3 { endpoint, .. } => endpoint,
                    _ => "",
                },
            )?,
            bucket: field_or_current(
                &fields,
                "bucket",
                match &current.backend {
                    Backend::S3 { bucket, .. } => bucket,
                    _ => "",
                },
            )?,
            region: field_or_current(
                &fields,
                "region",
                match &current.backend {
                    Backend::S3 { region, .. } => region,
                    _ => "",
                },
            )?,
            access_key_id: field_alias_or_current(
                &fields,
                ["access_key_id", "access_key", "ak"],
                match &current.backend {
                    Backend::S3 { access_key_id, .. } => access_key_id,
                    _ => "",
                },
            )?,
            secret_access_key: field_alias_or_current(
                &fields,
                ["secret_access_key", "secret_key", "sk"],
                match &current.backend {
                    Backend::S3 {
                        secret_access_key, ..
                    } => secret_access_key,
                    _ => "",
                },
            )?,
            addressing_style: parse_addressing_style(fields.get("addressing").map(String::as_str))
                .unwrap_or_else(|| match &current.backend {
                    Backend::S3 {
                        addressing_style, ..
                    } => addressing_style.clone(),
                    _ => S3AddressingStyle::PathStyle,
                }),
        },
        _ => return Err(anyhow!("unknown cloud backend draft: {kind}")),
    };

    Ok(settings)
}

fn split_cloud_draft_kind(input: &str) -> Option<(&str, &str)> {
    if input.is_empty() {
        return None;
    }
    let split_at = input
        .char_indices()
        .find_map(|(index, value)| value.is_whitespace().then_some(index))
        .unwrap_or(input.len());
    let kind = &input[..split_at];
    let rest = input[split_at..].trim_start();
    Some((kind, rest))
}

fn parse_cloud_draft_fields(input: &str) -> HashMap<String, String> {
    let markers = cloud_draft_field_markers(input);
    markers
        .iter()
        .enumerate()
        .map(|(index, marker)| {
            let end = markers
                .get(index + 1)
                .map(|next| next.start)
                .unwrap_or(input.len());
            (
                marker.key.clone(),
                input[marker.value_start..end].trim().to_string(),
            )
        })
        .collect()
}

struct FieldMarker {
    key: String,
    start: usize,
    value_start: usize,
}

fn cloud_draft_field_markers(input: &str) -> Vec<FieldMarker> {
    let mut markers = Vec::new();
    let mut index = 0usize;
    while index < input.len() {
        let Some((relative_start, _)) = input[index..]
            .char_indices()
            .find(|(_, value)| !value.is_whitespace())
        else {
            break;
        };
        let start = index + relative_start;
        let Some((key, value_start)) = parse_cloud_draft_field_marker(input, start) else {
            index = next_token_boundary(input, start);
            continue;
        };
        if is_cloud_draft_field_key(&key) {
            markers.push(FieldMarker {
                key,
                start,
                value_start,
            });
        }
        index = next_token_boundary(input, start);
    }
    markers
}

fn parse_cloud_draft_field_marker(input: &str, start: usize) -> Option<(String, usize)> {
    let rest = &input[start..];
    let key_len = rest
        .char_indices()
        .take_while(|(_, value)| value.is_ascii_alphanumeric() || *value == '_')
        .last()
        .map(|(index, value)| index + value.len_utf8())?;
    if key_len == 0 || !rest[key_len..].starts_with('=') {
        return None;
    }
    Some((rest[..key_len].to_ascii_lowercase(), start + key_len + 1))
}

fn next_token_boundary(input: &str, start: usize) -> usize {
    input[start..]
        .char_indices()
        .find_map(|(index, value)| value.is_whitespace().then_some(start + index))
        .unwrap_or(input.len())
}

fn is_cloud_draft_field_key(key: &str) -> bool {
    matches!(
        key,
        "root"
            | "root_path"
            | "max"
            | "max_concurrency"
            | "endpoint"
            | "username"
            | "password"
            | "bucket"
            | "region"
            | "access_key_id"
            | "access_key"
            | "ak"
            | "secret_access_key"
            | "secret_key"
            | "sk"
            | "addressing"
    )
}

pub fn cloud_settings_draft(current: &CloudSettings) -> String {
    let shared = format!("root={} max={}", current.root_path, current.max_concurrency);
    match &current.backend {
        Backend::Disabled => format!("disabled {shared}"),
        Backend::WebDAV {
            endpoint, username, ..
        } => {
            format!("webdav endpoint={endpoint} username={username} {shared}")
        }
        Backend::S3 {
            endpoint,
            bucket,
            region,
            addressing_style,
            ..
        } => format!(
            "s3 endpoint={endpoint} bucket={bucket} region={region} addressing={} {shared}",
            addressing_style_draft_value(addressing_style)
        ),
    }
}

fn addressing_style_draft_value(style: &S3AddressingStyle) -> &'static str {
    match style {
        S3AddressingStyle::PathStyle => "path",
        S3AddressingStyle::VirtualHostedStyle => "virtual",
        S3AddressingStyle::Auto => "auto",
    }
}

fn field_or_current(fields: &HashMap<String, String>, key: &str, current: &str) -> Result<String> {
    let value = fields
        .get(key)
        .map(String::as_str)
        .unwrap_or(current)
        .trim();
    if value.is_empty() {
        return Err(anyhow!("missing {key}"));
    }
    Ok(value.to_string())
}

fn field_alias_or_current<const N: usize>(
    fields: &HashMap<String, String>,
    keys: [&str; N],
    current: &str,
) -> Result<String> {
    let value = keys
        .iter()
        .find_map(|key| fields.get(*key))
        .map(String::as_str)
        .unwrap_or(current)
        .trim();
    if value.is_empty() {
        return Err(anyhow!("missing {}", keys[0]));
    }
    Ok(value.to_string())
}

fn parse_addressing_style(value: Option<&str>) -> Option<S3AddressingStyle> {
    match value?.to_ascii_lowercase().as_str() {
        "path" | "path-style" | "pathstyle" => Some(S3AddressingStyle::PathStyle),
        "virtual" | "virtual-hosted" | "virtual-hosted-style" | "virtualhostedstyle" => {
            Some(S3AddressingStyle::VirtualHostedStyle)
        }
        "auto" => Some(S3AddressingStyle::Auto),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_webdav_cloud_settings_draft() {
        let parsed = parse_cloud_settings_draft(
            "webdav endpoint=https://dav.example username=me password=secret root=/rgsm max=2",
            &CloudSettings::default(),
        )
        .unwrap();

        assert_eq!(parsed.root_path, "/rgsm");
        assert_eq!(parsed.max_concurrency, 2);
        assert!(matches!(parsed.backend, Backend::WebDAV { .. }));
    }

    #[test]
    fn parses_s3_cloud_settings_draft_aliases() {
        let parsed = parse_cloud_settings_draft(
            "s3 endpoint=https://s3.example bucket=saves region=auto ak=id sk=secret addressing=auto",
            &CloudSettings::default(),
        )
        .unwrap();

        if let Backend::S3 {
            addressing_style, ..
        } = parsed.backend
        {
            assert_eq!(addressing_style, S3AddressingStyle::Auto);
        } else {
            panic!("expected S3 backend");
        }
    }

    #[test]
    fn cloud_settings_draft_preserves_current_webdav_backend() {
        let current = CloudSettings {
            root_path: "/rgsm".to_string(),
            max_concurrency: 3,
            backend: Backend::WebDAV {
                endpoint: "https://dav.example".to_string(),
                username: "me".to_string(),
                password: "secret".to_string(),
            },
            ..Default::default()
        };

        let draft = cloud_settings_draft(&current);
        let parsed = parse_cloud_settings_draft(&draft, &current).unwrap();

        assert!(draft.starts_with("webdav "));
        assert!(!draft.contains("secret"));
        assert_eq!(parsed.root_path, "/rgsm");
        assert_eq!(parsed.max_concurrency, 3);
        assert!(matches!(parsed.backend, Backend::WebDAV { .. }));
    }

    #[test]
    fn parses_cloud_settings_draft_values_with_spaces() {
        let current = CloudSettings {
            root_path: "/old root".to_string(),
            max_concurrency: 1,
            backend: Backend::WebDAV {
                endpoint: "https://old.example".to_string(),
                username: "old user".to_string(),
                password: "old password".to_string(),
            },
            ..Default::default()
        };
        let parsed = parse_cloud_settings_draft(
            "webdav endpoint=https://dav.example/base path username=my user root=/cloud saves max=2",
            &current,
        )
        .unwrap();

        assert_eq!(parsed.root_path, "/cloud saves");
        assert_eq!(parsed.max_concurrency, 2);
        assert!(matches!(
            parsed.backend,
            Backend::WebDAV {
                ref endpoint,
                ref username,
                ref password,
            } if endpoint == "https://dav.example/base path"
                && username == "my user"
                && password == "old password"
        ));
    }

    #[test]
    fn parses_cloud_settings_draft_ignores_keys_inside_values() {
        let parsed = parse_cloud_settings_draft(
            "s3 endpoint=https://s3.example/?region=ignored&bucket=ignored bucket=real bucket region=auto ak=id sk=secret addressing=auto root=/cloud saves",
            &CloudSettings::default(),
        )
        .unwrap();

        assert_eq!(parsed.root_path, "/cloud saves");
        assert!(matches!(
            parsed.backend,
            Backend::S3 {
                ref endpoint,
                ref bucket,
                ..
            } if endpoint == "https://s3.example/?region=ignored&bucket=ignored"
                && bucket == "real bucket"
        ));
    }

    #[test]
    fn cloud_settings_draft_round_trips_spaced_values() {
        let current = CloudSettings {
            root_path: "/cloud saves/rgsm".to_string(),
            max_concurrency: 3,
            backend: Backend::WebDAV {
                endpoint: "https://dav.example/base path".to_string(),
                username: "my user".to_string(),
                password: "secret".to_string(),
            },
            ..Default::default()
        };

        let draft = cloud_settings_draft(&current);
        let parsed = parse_cloud_settings_draft(&draft, &current).unwrap();

        assert_eq!(parsed.root_path, current.root_path);
        assert!(matches!(
            parsed.backend,
            Backend::WebDAV {
                ref endpoint,
                ref username,
                ..
            } if endpoint == "https://dav.example/base path" && username == "my user"
        ));
    }

    #[test]
    fn cloud_settings_draft_preserves_current_s3_backend() {
        let current = CloudSettings {
            root_path: "/rgsm".to_string(),
            max_concurrency: 2,
            backend: Backend::S3 {
                endpoint: "https://s3.example".to_string(),
                bucket: "saves".to_string(),
                region: "auto".to_string(),
                access_key_id: "id".to_string(),
                secret_access_key: "secret".to_string(),
                addressing_style: S3AddressingStyle::Auto,
            },
            ..Default::default()
        };

        let draft = cloud_settings_draft(&current);
        let parsed = parse_cloud_settings_draft(&draft, &current).unwrap();

        assert!(draft.starts_with("s3 "));
        assert!(!draft.contains("secret"));
        assert!(matches!(
            parsed.backend,
            Backend::S3 {
                addressing_style: S3AddressingStyle::Auto,
                ..
            }
        ));
    }
}
