use std::sync::Arc;

use anyhow::Result;
use rgsm_core::cloud_sync::ConflictResolution;
use tokio_util::sync::CancellationToken;

use crate::{
    model::{ListSort, Modal, ModalKind, PendingAction, Screen, SettingsItem},
    operations::{
        Operation, cloud_settings_draft, parse_cloud_settings_draft, selected_extra_backup_count,
        submit_operation,
    },
};

use super::App;

impl App {
    pub(super) fn prompt(
        &mut self,
        action: PendingAction,
        title: &str,
        message: &str,
        initial: &str,
    ) {
        self.modal = Some(Modal {
            kind: ModalKind::Prompt,
            title: title.to_string(),
            message: message.to_string(),
            input: initial.to_string(),
            action,
        });
    }

    fn confirm(&mut self, action: PendingAction, title: &str, message: String) {
        self.modal = Some(Modal {
            kind: ModalKind::Confirm,
            title: title.to_string(),
            message,
            input: String::new(),
            action,
        });
    }

    pub(super) fn show_help(&mut self) {
        self.modal = Some(Modal {
            kind: ModalKind::Help,
            title: rust_i18n::t!("tui.dialog.help_title").to_string(),
            message: crate::ui::help_text(),
            input: String::new(),
            action: PendingAction::None,
        });
    }

    pub(super) fn message(&mut self, title: &str, message: &str) {
        self.modal = Some(Modal {
            kind: ModalKind::Message,
            title: title.to_string(),
            message: message.to_string(),
            input: String::new(),
            action: PendingAction::None,
        });
    }

    pub(super) fn prompt_search(&mut self) {
        match self.screen {
            Screen::Ludusavi => self.prompt(
                PendingAction::SearchImportableGames,
                rust_i18n::t!("tui.search.importable_title").as_ref(),
                rust_i18n::t!("misc.search").as_ref(),
                &self.import_filter.clone(),
            ),
            Screen::Home | Screen::GameEditor | Screen::Cloud => self.prompt(
                PendingAction::SearchGames,
                rust_i18n::t!("tui.search.games_title").as_ref(),
                rust_i18n::t!("misc.search").as_ref(),
                &self.game_filter.clone(),
            ),
            _ => self.status = rust_i18n::t!("tui.status.nothing_to_do").to_string(),
        }
    }

    pub(super) fn clear_filter(&mut self) {
        match self.screen {
            Screen::Ludusavi => self.import_filter.clear(),
            Screen::Home | Screen::GameEditor | Screen::Cloud => self.game_filter.clear(),
            _ => {}
        }
        self.selection.game = 0;
        self.selection.importable = 0;
        if matches!(
            self.screen,
            Screen::Home | Screen::GameEditor | Screen::Cloud
        ) {
            self.refresh_selected_snapshots();
        } else {
            self.clamp_selection();
        }
        self.status = rust_i18n::t!("tui.status.filter_cleared").to_string();
    }

    pub(super) fn toggle_sort(&mut self) {
        let sort = match self.screen {
            Screen::Ludusavi => {
                self.import_sort = self.import_sort.next();
                self.import_sort
            }
            Screen::Home | Screen::GameEditor | Screen::Cloud => {
                self.game_sort = self.game_sort.next();
                self.game_sort
            }
            _ => {
                self.status = rust_i18n::t!("tui.status.nothing_to_do").to_string();
                return;
            }
        };
        self.status = format!("{}: {}", rust_i18n::t!("tui.search.sort"), sort_label(sort));
        if matches!(
            self.screen,
            Screen::Home | Screen::GameEditor | Screen::Cloud
        ) {
            self.refresh_selected_snapshots();
        } else {
            self.clamp_selection();
        }
    }

    pub(super) fn prompt_context_add(&mut self) {
        match self.screen {
            Screen::GameEditor => self.prompt(
                PendingAction::AddSaveUnitPath,
                rust_i18n::t!("tui.dialog.add_save_unit").as_ref(),
                rust_i18n::t!("tui.dialog.path_for_current_device").as_ref(),
                "",
            ),
            _ => self.prompt_add_game(),
        }
    }

    pub(super) fn prompt_add_game(&mut self) {
        self.prompt(
            PendingAction::AddGameName,
            rust_i18n::t!("tui.dialog.add_game").as_ref(),
            rust_i18n::t!("addgame.game_name").as_ref(),
            "",
        );
    }

    pub(super) fn prompt_create_snapshot(&mut self, parent: Option<String>) {
        if self.selected_game().is_none() {
            self.status = rust_i18n::t!("tui.status.select_game_first").to_string();
            return;
        }
        self.modal = Some(Modal {
            kind: ModalKind::Prompt,
            title: rust_i18n::t!("tui.dialog.create_snapshot").to_string(),
            message: parent
                .as_ref()
                .map(|date| {
                    format!(
                        "{}: {date}",
                        rust_i18n::t!("tui.dialog.description_for_snapshot_from")
                    )
                })
                .unwrap_or_else(|| rust_i18n::t!("manage.description").to_string()),
            input: String::new(),
            action: if parent.is_some() {
                PendingAction::CreateSnapshotFromSelected
            } else {
                PendingAction::CreateSnapshot
            },
        });
    }

    pub(super) fn prompt_edit_description(&mut self) {
        let Some(date) = self.selected_snapshot_date() else {
            self.status = rust_i18n::t!("tui.status.select_snapshot_first").to_string();
            return;
        };
        self.prompt(
            PendingAction::EditSnapshotDescription,
            rust_i18n::t!("tui.dialog.edit_snapshot_description").as_ref(),
            &format!(
                "{}: {date}",
                rust_i18n::t!("tui.dialog.new_description_for")
            ),
            "",
        );
    }

    pub(super) fn prompt_rename_game(&mut self) {
        let Some(game) = self.selected_game() else {
            self.status = rust_i18n::t!("tui.status.select_game_first").to_string();
            return;
        };
        self.prompt(
            PendingAction::RenameGame,
            rust_i18n::t!("tui.dialog.rename_game").as_ref(),
            rust_i18n::t!("tui.dialog.new_game_name").as_ref(),
            &game.name,
        );
    }

    pub(super) fn prompt_edit_selected_path(&mut self) {
        let Some(game) = self.selected_game() else {
            self.status = rust_i18n::t!("tui.status.select_game_first").to_string();
            return;
        };
        let current_id = rgsm_core::device::get_current_device_id();
        let initial = game
            .save_paths
            .get(self.selection.save_unit)
            .and_then(|unit| unit.paths.get(current_id))
            .cloned()
            .unwrap_or_default();
        self.prompt(
            PendingAction::EditSelectedPath,
            rust_i18n::t!("tui.dialog.edit_save_path").as_ref(),
            rust_i18n::t!("tui.dialog.path_for_current_device").as_ref(),
            &initial,
        );
    }

    pub(super) fn confirm_selected(&mut self, action: PendingAction) {
        let message = match action {
            PendingAction::RestoreSnapshot => self
                .selected_snapshot_date()
                .map(|date| format!("{}: {date}", rust_i18n::t!("tui.dialog.restore_snapshot"))),
            PendingAction::DeleteSnapshot => self
                .selected_snapshot_date()
                .map(|date| format!("{}: {date}", rust_i18n::t!("tui.dialog.delete_snapshot"))),
            PendingAction::DeleteGame => self
                .selected_game()
                .map(|game| format!("{}: {}", rust_i18n::t!("tui.dialog.delete_game"), game.name)),
            PendingAction::SetCurrentPosition => self.selected_snapshot_date().map(|date| {
                format!(
                    "{}: {date}",
                    rust_i18n::t!("tui.dialog.set_current_position")
                )
            }),
            PendingAction::DetachSnapshot => self
                .selected_snapshot_date()
                .map(|date| format!("{}: {date}", rust_i18n::t!("tui.dialog.detach_snapshot"))),
            _ => Some(rust_i18n::t!("tui.dialog.confirm_action").to_string()),
        };
        if let Some(message) = message {
            self.confirm(
                action,
                rust_i18n::t!("tui.dialog.confirm_title").as_ref(),
                message,
            );
        } else {
            self.status = rust_i18n::t!("tui.status.select_item_first").to_string();
        }
    }

    pub(super) fn cloud_action(&mut self, action: PendingAction) {
        match action {
            PendingAction::UploadAll => self.confirm(
                action,
                rust_i18n::t!("tui.dialog.cloud_upload_title").as_ref(),
                rust_i18n::t!("tui.dialog.cloud_upload_message").to_string(),
            ),
            PendingAction::DownloadAll => self.confirm(
                action,
                rust_i18n::t!("tui.dialog.cloud_download_title").as_ref(),
                rust_i18n::t!("tui.dialog.cloud_download_message").to_string(),
            ),
            PendingAction::SyncSelected => {
                if let Some(game) = self.selected_game() {
                    self.submit(Operation::SyncGame(game.name));
                }
            }
            PendingAction::SyncAll => self.submit(Operation::SyncAll),
            PendingAction::CheckCloud => self.submit(Operation::CheckCloud),
            PendingAction::EditCloudSettings => self.prompt_cloud_settings(),
            PendingAction::ResolveKeepLocal => {
                if let Some(game) = self.selected_game() {
                    self.confirm(
                        action,
                        rust_i18n::t!("tui.dialog.resolve_conflict_title").as_ref(),
                        format!(
                            "{}: {}",
                            rust_i18n::t!("tui.dialog.keep_local_progress"),
                            game.name
                        ),
                    );
                }
            }
            PendingAction::ResolveUseCloud => {
                if let Some(game) = self.selected_game() {
                    self.confirm(
                        action,
                        rust_i18n::t!("tui.dialog.resolve_conflict_title").as_ref(),
                        format!(
                            "{}: {}",
                            rust_i18n::t!("tui.dialog.use_cloud_progress"),
                            game.name
                        ),
                    );
                }
            }
            _ => {}
        }
    }

    pub(super) fn import_selected(&mut self) {
        if let Some(candidate) = self.selected_import_candidate() {
            self.confirm(
                PendingAction::ImportSelectedGame,
                rust_i18n::t!("tui.dialog.import_game_title").as_ref(),
                format!(
                    "{}: {}\n{}: {}",
                    rust_i18n::t!("addgame.game_name"),
                    candidate.game.name,
                    rust_i18n::t!("game_import.save_paths_count"),
                    self.selected_import_paths().len()
                ),
            );
        }
    }

    pub(super) fn prompt_edit_import_path(&mut self) {
        let Some(candidate) = self.selected_import_candidate() else {
            self.status = rust_i18n::t!("tui.empty.select_importable").to_string();
            return;
        };
        let name = candidate.game.name.clone();
        let paths = self.selected_import_paths();
        let Some(path) = paths.get(self.selection.import_path).cloned() else {
            self.status = rust_i18n::t!("tui.status.select_item_first").to_string();
            return;
        };
        self.prompt(
            PendingAction::EditImportPath,
            rust_i18n::t!("tui.import.edit_path").as_ref(),
            &name,
            &path,
        );
    }

    pub(super) fn toggle_ludusavi_filter(&mut self) {
        self.settings.ludusavi_local_only = !self.settings.ludusavi_local_only;
        if self.save_tui_settings() {
            self.submit(Operation::ReloadData);
        }
    }

    pub(super) fn prompt_edit_current_device_name(&mut self) {
        let current_id = rgsm_core::device::get_current_device_id();
        let initial = self
            .data
            .config
            .devices
            .get(current_id)
            .map(|device| device.name.as_str())
            .unwrap_or_default()
            .to_string();
        self.prompt(
            PendingAction::EditCurrentDeviceName,
            rust_i18n::t!("tui.settings.device_name_action").as_ref(),
            rust_i18n::t!("settings.device_name").as_ref(),
            &initial,
        );
    }

    pub(super) fn prompt_add_current_device_root(&mut self) {
        self.prompt(
            PendingAction::AddCurrentDeviceRoot,
            rust_i18n::t!("tui.settings.add_game_root_action").as_ref(),
            rust_i18n::t!("settings.game_roots_path_placeholder").as_ref(),
            "",
        );
    }

    pub(super) fn prompt_add_vn_scan_root(&mut self) {
        self.prompt(
            PendingAction::AddVnScanRoot,
            rust_i18n::t!("tui.settings.add_vn_scan_root_action").as_ref(),
            rust_i18n::t!("settings.game_roots_path_placeholder").as_ref(),
            "",
        );
    }

    pub(super) fn prompt_import_gui_profile(&mut self) {
        self.prompt(
            PendingAction::ImportGuiProfile,
            rust_i18n::t!("tui.settings.import_gui_profile_action").as_ref(),
            rust_i18n::t!("tui.settings.import_gui_profile_prompt").as_ref(),
            "",
        );
    }

    pub(super) fn scan_vns(&mut self) {
        let dirs = self.data.config.settings.vn_scan_dirs.clone();
        if dirs.is_empty() {
            self.status = rust_i18n::t!("addgame.scan_vns_no_dirs").to_string();
            self.message(
                rust_i18n::t!("addgame.scan_vns").as_ref(),
                rust_i18n::t!("addgame.scan_vns_no_dirs").as_ref(),
            );
            return;
        }

        self.status = rust_i18n::t!("addgame.scan_vns_scanning").to_string();
        let existing = self
            .data
            .config
            .games
            .iter()
            .map(|game| game.name.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let drafts = rgsm_core::vn_scanner::scan_games(&dirs)
            .into_iter()
            .filter(|draft| {
                !existing
                    .iter()
                    .any(|name| name == &draft.name.to_ascii_lowercase())
            })
            .collect::<Vec<_>>();
        if drafts.is_empty() {
            self.status = rust_i18n::t!("addgame.scan_vns_no_result").to_string();
            self.message(
                rust_i18n::t!("addgame.scan_vns").as_ref(),
                rust_i18n::t!("addgame.scan_vns_no_result").as_ref(),
            );
            return;
        }

        let names = drafts
            .iter()
            .take(8)
            .map(|draft| draft.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let more = drafts.len().saturating_sub(8);
        let suffix = if more == 0 {
            String::new()
        } else {
            format!(" (+{more})")
        };
        self.vn_scan_results = drafts;
        self.confirm(
            PendingAction::ImportVnScanResults,
            rust_i18n::t!("addgame.scan_vns").as_ref(),
            format!(
                "{}: {}{}\n{}",
                rust_i18n::t!("tui.vn.detected_games"),
                self.vn_scan_results.len(),
                suffix,
                names
            ),
        );
    }

    pub(super) fn toggle_setting(&mut self) {
        if self.screen == Screen::Cloud {
            if let Some(game) = self.selected_game() {
                self.submit(Operation::ToggleCloudSync(game));
            }
            return;
        }
        if self.screen != Screen::Settings {
            self.status = rust_i18n::t!("tui.status.nothing_to_do").to_string();
            return;
        }
        match SettingsItem::ALL
            .get(self.selection.settings)
            .copied()
            .unwrap_or(SettingsItem::AutoCloudEnqueue)
        {
            SettingsItem::AutoCloudEnqueue => {
                self.settings.auto_enqueue_cloud_on_change =
                    !self.settings.auto_enqueue_cloud_on_change;
                self.save_tui_settings();
            }
            SettingsItem::LudusaviLocalOnly => self.toggle_ludusavi_filter(),
            SettingsItem::ImportGuiProfile => self.prompt_import_gui_profile(),
            SettingsItem::CurrentDeviceName => self.prompt_edit_current_device_name(),
            SettingsItem::AddGameRoot => self.prompt_add_current_device_root(),
            SettingsItem::AddVnScanRoot => self.prompt_add_vn_scan_root(),
        }
    }

    fn save_tui_settings(&mut self) -> bool {
        if let Err(err) = self.settings.save(&self.data_dir) {
            self.status = format!(
                "{}: {err:#}",
                rust_i18n::t!("tui.status.save_settings_failed")
            );
            false
        } else {
            self.status = rust_i18n::t!("tui.status.settings_saved").to_string();
            true
        }
    }

    fn prompt_cloud_settings(&mut self) {
        let current = &self.data.config.settings.cloud_settings;
        let draft = cloud_settings_draft(current);
        self.prompt(
            PendingAction::EditCloudSettings,
            rust_i18n::t!("tui.dialog.cloud_backend_title").as_ref(),
            rust_i18n::t!("tui.dialog.cloud_backend_hint").as_ref(),
            &draft,
        );
    }

    pub(super) fn run_pending_action(
        &mut self,
        action: PendingAction,
        input: String,
    ) -> Result<()> {
        match action {
            PendingAction::AcknowledgeExperimentalWarning => {
                self.status = rust_i18n::t!("tui.status.experimental_notice").to_string();
            }
            PendingAction::AddGameName if !input.is_empty() => {
                self.submit(Operation::AddGame(input))
            }
            PendingAction::RenameGame if !input.is_empty() => {
                if let Some(game) = self.selected_game() {
                    self.submit(Operation::RenameGame(game, input));
                }
            }
            PendingAction::AddSaveUnitPath if !input.is_empty() => {
                if let Some(game) = self.selected_game() {
                    self.submit(Operation::AddSaveUnitPath(game, input));
                }
            }
            PendingAction::EditSelectedPath if !input.is_empty() => {
                if let Some(game) = self.selected_game() {
                    self.submit(Operation::EditSelectedPath(
                        game,
                        self.selection.save_unit,
                        input,
                    ));
                }
            }
            PendingAction::CreateSnapshot => {
                if let Some(game) = self.selected_game() {
                    self.submit(Operation::CreateSnapshot(game, input, None));
                }
            }
            PendingAction::CreateSnapshotFromSelected => {
                if let (Some(game), Some(parent)) =
                    (self.selected_game(), self.selected_snapshot_date())
                {
                    self.submit(Operation::CreateSnapshot(game, input, Some(parent)));
                }
            }
            PendingAction::RestoreSnapshot => {
                if let (Some(game), Some(date)) =
                    (self.selected_game(), self.selected_snapshot_date())
                {
                    self.submit(Operation::RestoreSnapshot(game, date));
                }
            }
            PendingAction::DeleteSnapshot => {
                if let (Some(game), Some(date)) =
                    (self.selected_game(), self.selected_snapshot_date())
                {
                    self.submit(Operation::DeleteSnapshot(game, date));
                }
            }
            PendingAction::BatchDeleteSnapshots => {
                if let (Some(game), Some(date)) =
                    (self.selected_game(), self.selected_snapshot_date())
                {
                    self.submit(Operation::BatchDeleteSnapshots(game, vec![date]));
                }
            }
            PendingAction::EditSnapshotDescription if !input.is_empty() => {
                if let (Some(game), Some(date)) =
                    (self.selected_game(), self.selected_snapshot_date())
                {
                    self.submit(Operation::EditSnapshotDescription(game, date, input));
                }
            }
            PendingAction::SetCurrentPosition => {
                if let (Some(game), Some(date)) =
                    (self.selected_game(), self.selected_snapshot_date())
                {
                    self.submit(Operation::SetCurrentPosition(game, date));
                }
            }
            PendingAction::DetachSnapshot => {
                if let (Some(game), Some(date)) =
                    (self.selected_game(), self.selected_snapshot_date())
                {
                    self.submit(Operation::DetachSnapshot(game, date));
                }
            }
            PendingAction::DeleteGame => {
                if let Some(game) = self.selected_game() {
                    self.submit(Operation::DeleteGame(game));
                }
            }
            PendingAction::UploadAll => self.submit(Operation::UploadAll),
            PendingAction::DownloadAll => self.submit(Operation::DownloadAll),
            PendingAction::EditCloudSettings if !input.is_empty() => {
                let current = &self.data.config.settings.cloud_settings;
                match parse_cloud_settings_draft(&input, current) {
                    Ok(settings) => self.submit(Operation::SaveCloudSettings(settings)),
                    Err(err) => {
                        self.status = format!(
                            "{}: {err:#}",
                            rust_i18n::t!("tui.status.invalid_cloud_settings")
                        );
                    }
                }
            }
            PendingAction::ResolveKeepLocal => {
                if let Some(game) = self.selected_game() {
                    self.submit(Operation::ResolveConflict(
                        game.name,
                        ConflictResolution::KeepLocal,
                    ));
                }
            }
            PendingAction::ResolveUseCloud => {
                if let Some(game) = self.selected_game() {
                    self.submit(Operation::ResolveConflict(
                        game.name,
                        ConflictResolution::AcceptRemote,
                    ));
                }
            }
            PendingAction::ImportSelectedGame => {
                if let Some(candidate) = self.selected_import_candidate() {
                    self.submit(Operation::ImportGame {
                        name: candidate.game.name.clone(),
                        save_paths: self.selected_import_paths(),
                    });
                }
            }
            PendingAction::SearchGames => {
                self.game_filter = input;
                self.selection.game = 0;
                self.refresh_selected_snapshots();
                self.status = rust_i18n::t!("tui.status.filter_applied").to_string();
            }
            PendingAction::SearchImportableGames => {
                self.import_filter = input;
                self.selection.importable = 0;
                self.selection.import_path = 0;
                self.clamp_selection();
                self.status = rust_i18n::t!("tui.status.filter_applied").to_string();
            }
            PendingAction::EditImportPath if !input.is_empty() => {
                if let Some(candidate) = self.selected_import_candidate() {
                    let name = candidate.game.name.clone();
                    let mut paths = self.selected_import_paths();
                    if let Some(path) = paths.get_mut(self.selection.import_path) {
                        *path = input;
                        self.import_path_overrides.insert(name, paths);
                        self.status = rust_i18n::t!("tui.status.import_path_updated").to_string();
                    }
                }
            }
            PendingAction::ImportGuiProfile if !input.is_empty() => {
                self.submit(Operation::ImportGuiProfile(input));
            }
            PendingAction::EditCurrentDeviceName if !input.is_empty() => {
                self.submit(Operation::UpdateCurrentDeviceName(input));
            }
            PendingAction::AddCurrentDeviceRoot if !input.is_empty() => {
                self.submit(Operation::AddCurrentDeviceRoot(input));
            }
            PendingAction::AddVnScanRoot if !input.is_empty() => {
                self.submit(Operation::AddVnScanRoot(input));
            }
            PendingAction::ImportVnScanResults => {
                let drafts = std::mem::take(&mut self.vn_scan_results);
                if drafts.is_empty() {
                    self.status = rust_i18n::t!("addgame.scan_vns_no_result").to_string();
                } else {
                    self.submit(Operation::ImportVnGames(drafts));
                }
            }
            _ => self.status = rust_i18n::t!("tui.status.nothing_to_do").to_string(),
        }
        Ok(())
    }

    pub(super) fn submit(&mut self, operation: Operation) {
        if self.operation_running {
            self.status = rust_i18n::t!("tui.status.operation_running").to_string();
            return;
        }
        let cancel_token = CancellationToken::new();
        self.operation_running = true;
        self.cancel_token = Some(cancel_token.clone());
        submit_operation(
            self.op_tx.clone(),
            self.settings.clone(),
            Arc::clone(&self.log),
            Arc::clone(&self.cloud_sync_manager),
            cancel_token,
            operation,
        );
    }

    pub(super) fn cancel_operation(&mut self) {
        if let Some(token) = &self.cancel_token {
            token.cancel();
            self.status = rust_i18n::t!("cloud_sync.cancelled").to_string();
        } else {
            self.status = rust_i18n::t!("tui.status.no_operation").to_string();
        }
    }

    pub(super) fn log_info(&self, message: String) {
        if let Ok(mut log) = self.log.lock() {
            log.info(message);
        }
    }

    pub(super) fn log_error(&self, message: String) {
        if let Ok(mut log) = self.log.lock() {
            log.error(message);
        }
    }

    pub fn extra_backup_count(&self) -> usize {
        self.selected_game()
            .as_ref()
            .map(selected_extra_backup_count)
            .unwrap_or(0)
    }
}

fn sort_label(sort: ListSort) -> String {
    match sort {
        ListSort::Natural => rust_i18n::t!("tui.sort.natural").to_string(),
        ListSort::NameAsc => rust_i18n::t!("tui.sort.name_asc").to_string(),
        ListSort::NameDesc => rust_i18n::t!("tui.sort.name_desc").to_string(),
    }
}
