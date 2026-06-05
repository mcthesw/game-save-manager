use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind, poll, read,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use rgsm_core::{backup::GameDraft, cloud_sync::CloudSyncTaskManager, device::Device};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_util::sync::CancellationToken;

use crate::logging::SessionLog;
use crate::model::{
    AppData, ImportCandidate, ListSort, Modal, ModalKind, OperationEvent, Pane, PendingAction,
    Screen, Selection, SettingsItem,
};
use crate::operations::{Operation, load_data};
use crate::terminal::TuiTerminal;
use crate::tui_settings::TuiSettings;

mod actions;

pub struct App {
    pub data_dir: PathBuf,
    pub settings: TuiSettings,
    pub data: AppData,
    pub screen: Screen,
    pub pane: Pane,
    pub selection: Selection,
    pub status: String,
    pub game_filter: String,
    pub game_sort: ListSort,
    pub import_filter: String,
    pub import_sort: ListSort,
    pub import_path_overrides: HashMap<String, Vec<String>>,
    pub vn_scan_results: Vec<GameDraft>,
    pub modal: Option<Modal>,
    pub log: Arc<Mutex<SessionLog>>,
    cloud_sync_manager: Arc<CloudSyncTaskManager>,
    pub operation_running: bool,
    cancel_token: Option<CancellationToken>,
    should_quit: bool,
    op_tx: UnboundedSender<OperationEvent>,
    op_rx: UnboundedReceiver<OperationEvent>,
}

impl App {
    pub async fn new(data_dir: PathBuf, settings: TuiSettings) -> Result<Self> {
        let log = Arc::new(Mutex::new(SessionLog::new(&data_dir)?));
        let sync_emitter = Arc::new(crate::hooks::TuiSyncEmitter::new(Arc::clone(&log)));
        let cloud_sync_manager = CloudSyncTaskManager::new(sync_emitter);
        let cloud_sync_worker = Arc::clone(&cloud_sync_manager);
        tokio::spawn(async move {
            cloud_sync_worker.run().await;
        });
        let data = load_data(&settings).await?;
        let (op_tx, op_rx) = unbounded_channel();
        let app = Self {
            data_dir,
            settings,
            data,
            screen: Screen::Home,
            pane: Pane::Left,
            selection: Selection::default(),
            status: rust_i18n::t!("tui.status.experimental_notice").to_string(),
            game_filter: String::new(),
            game_sort: ListSort::Natural,
            import_filter: String::new(),
            import_sort: ListSort::Natural,
            import_path_overrides: HashMap::new(),
            vn_scan_results: Vec::new(),
            modal: Some(Self::experimental_warning_modal()),
            log,
            cloud_sync_manager,
            operation_running: false,
            cancel_token: None,
            should_quit: false,
            op_tx,
            op_rx,
        };
        app.log_info(format!(
            "TUI started: profile={}, games={}, importable={}, manifest_source={}",
            app.data_dir.display(),
            app.data.games.len(),
            app.data.importable_games.len(),
            app.data.manifest_status.source
        ));
        Ok(app)
    }

    fn experimental_warning_modal() -> Modal {
        Modal {
            kind: ModalKind::Confirm,
            title: rust_i18n::t!("tui.warning.experimental_title").to_string(),
            message: rust_i18n::t!("tui.warning.experimental_body").to_string(),
            input: String::new(),
            action: PendingAction::AcknowledgeExperimentalWarning,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub async fn enqueue_config_migration_sync(&self) -> Result<()> {
        if !self.settings.auto_enqueue_cloud_on_change {
            return Ok(());
        }

        let config = rgsm_core::config::get_config()?;
        self.cloud_sync_manager
            .enqueue_config_upload_if_enabled(&config, "config_migration")
            .await;
        Ok(())
    }

    pub fn selected_game(&self) -> Option<rgsm_core::backup::Game> {
        let indices = self.visible_game_indices();
        indices
            .get(self.selection.game)
            .and_then(|index| self.data.games.get(*index))
            .cloned()
    }

    pub fn selected_snapshot_date(&self) -> Option<String> {
        self.data
            .selected_snapshots
            .as_ref()
            .and_then(|snapshots| snapshots.backups.get(self.selection.snapshot))
            .map(|snapshot| snapshot.date.clone())
    }

    pub fn refresh_selected_snapshots(&mut self) {
        self.selection.game = clamp(self.selection.game, self.visible_game_indices().len());
        self.data.selected_snapshots = self
            .selected_game()
            .and_then(|game| game.get_game_snapshots_info().ok());
        self.clamp_selection();
    }

    pub fn device_rows(&self) -> Vec<(String, Device)> {
        let current_id = rgsm_core::device::get_current_device_id();
        let mut rows = Vec::new();
        if let Some(device) = self.data.config.devices.get(current_id) {
            rows.push((current_id.clone(), device.clone()));
        }
        let mut others = self
            .data
            .config
            .devices
            .iter()
            .filter(|(id, _)| *id != current_id)
            .map(|(id, device)| (id.clone(), device.clone()))
            .collect::<Vec<_>>();
        others.sort_by(|a, b| a.1.name.cmp(&b.1.name).then_with(|| a.0.cmp(&b.0)));
        rows.extend(others);
        rows
    }

    pub fn poll_input(&mut self) -> Result<Option<Event>> {
        if poll(Duration::from_millis(50))? {
            return Ok(Some(read()?));
        }
        Ok(None)
    }

    pub fn drain_operation_events(&mut self) {
        while let Ok(event) = self.op_rx.try_recv() {
            match event {
                OperationEvent::Started(message) => {
                    self.operation_running = true;
                    self.status = message.clone();
                    self.log_info(format!("started: {message}"));
                }
                OperationEvent::Finished { status, detail } => {
                    self.operation_running = false;
                    self.cancel_token = None;
                    self.status = status;
                    self.log_info(detail);
                }
                OperationEvent::Failed(message) => {
                    self.operation_running = false;
                    self.cancel_token = None;
                    self.status = rust_i18n::t!("tui.status.operation_failed").to_string();
                    self.log_error(message.clone());
                    self.message(rust_i18n::t!("misc.error").as_ref(), &message);
                }
                OperationEvent::DataReloaded(data) => {
                    self.data = *data;
                    self.refresh_selected_snapshots();
                    self.log_info(format!(
                        "data reloaded: games={}, importable={}, manifest_source={}",
                        self.data.games.len(),
                        self.data.importable_games.len(),
                        self.data.manifest_status.source
                    ));
                }
            }
        }
    }

    pub fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key)?,
            Event::Key(_) => {}
            Event::Mouse(mouse) if self.modal.is_none() => self.handle_mouse(mouse),
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            _ => {}
        }
        Ok(())
    }

    pub fn visible_game_indices(&self) -> Vec<usize> {
        let filter = self.game_filter.to_ascii_lowercase();
        let mut indices = self
            .data
            .games
            .iter()
            .enumerate()
            .filter(|(_, game)| {
                filter.is_empty() || game.name.to_ascii_lowercase().contains(&filter)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        match self.game_sort {
            ListSort::Natural => {}
            ListSort::NameAsc => {
                indices.sort_by_key(|index| self.data.games[*index].name.to_ascii_lowercase())
            }
            ListSort::NameDesc => indices.sort_by_key(|index| {
                std::cmp::Reverse(self.data.games[*index].name.to_ascii_lowercase())
            }),
        }
        indices
    }

    pub fn visible_importable_indices(&self) -> Vec<usize> {
        let filter = self.import_filter.to_ascii_lowercase();
        let mut indices = self
            .data
            .importable_games
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                filter.is_empty() || candidate.game.name.to_ascii_lowercase().contains(&filter)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        match self.import_sort {
            ListSort::Natural => {}
            ListSort::NameAsc => indices.sort_by_key(|index| {
                self.data.importable_games[*index]
                    .game
                    .name
                    .to_ascii_lowercase()
            }),
            ListSort::NameDesc => indices.sort_by_key(|index| {
                std::cmp::Reverse(
                    self.data.importable_games[*index]
                        .game
                        .name
                        .to_ascii_lowercase(),
                )
            }),
        }
        indices
    }

    pub fn selected_import_candidate(&self) -> Option<&ImportCandidate> {
        let indices = self.visible_importable_indices();
        indices
            .get(self.selection.importable)
            .and_then(|index| self.data.importable_games.get(*index))
    }

    pub fn selected_import_paths(&self) -> Vec<String> {
        self.selected_import_candidate()
            .map(|candidate| {
                self.import_path_overrides
                    .get(&candidate.game.name)
                    .cloned()
                    .unwrap_or_else(|| candidate.save_paths.clone())
            })
            .unwrap_or_default()
    }

    pub fn draw(&mut self, terminal: &mut TuiTerminal) -> Result<()> {
        terminal.draw(|frame| crate::ui::draw(frame, self))?;
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.modal.is_some() {
            return self.handle_modal_key(key);
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help(),
            KeyCode::Char('/') => self.prompt_search(),
            KeyCode::Backspace | KeyCode::Char('F') => self.clear_filter(),
            KeyCode::Char('=') | KeyCode::Char('S') => self.toggle_sort(),
            KeyCode::Char('1') => self.switch_screen(Screen::Home),
            KeyCode::Char('2') => self.switch_screen(Screen::GameEditor),
            KeyCode::Char('3') => self.switch_screen(Screen::Ludusavi),
            KeyCode::Char('4') => self.switch_screen(Screen::Cloud),
            KeyCode::Char('5') => self.switch_screen(Screen::Settings),
            KeyCode::Char('6') => self.switch_screen(Screen::Logs),
            KeyCode::Tab => self.next_pane(),
            KeyCode::BackTab => self.prev_pane(),
            KeyCode::Esc => self.switch_screen(Screen::Home),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Left | KeyCode::Char('h') => self.prev_pane(),
            KeyCode::Right | KeyCode::Char('l') => self.next_pane(),
            KeyCode::Enter => self.default_action(),
            _ => self.handle_screen_key(key.code),
        }
        Ok(())
    }

    fn handle_screen_key(&mut self, code: KeyCode) {
        match self.screen {
            Screen::Home => self.handle_home_key(code),
            Screen::GameEditor => self.handle_game_editor_key(code),
            Screen::Ludusavi => self.handle_ludusavi_key(code),
            Screen::Cloud => self.handle_cloud_key(code),
            Screen::Settings => self.handle_settings_key(code),
            Screen::Logs => {}
        }
    }

    fn handle_home_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('n') => self.prompt_add_game(),
            KeyCode::Char('b') => self.prompt_create_snapshot(None),
            KeyCode::Char('B') => self.prompt_create_snapshot(self.selected_snapshot_date()),
            KeyCode::Char('r') => self.confirm_selected(PendingAction::RestoreSnapshot),
            KeyCode::Char('d') => self.confirm_selected(PendingAction::DeleteSnapshot),
            KeyCode::Char('x') => self.confirm_selected(PendingAction::DeleteGame),
            KeyCode::Char('X') => self.confirm_selected(PendingAction::BatchDeleteSnapshots),
            KeyCode::Char('e') => self.prompt_edit_description(),
            KeyCode::Char('p') => self.confirm_selected(PendingAction::SetCurrentPosition),
            KeyCode::Char('D') => self.confirm_selected(PendingAction::DetachSnapshot),
            KeyCode::Char('g') => self.switch_screen(Screen::GameEditor),
            KeyCode::Char('i') => self.switch_screen(Screen::Ludusavi),
            KeyCode::Char('c') => self.switch_screen(Screen::Cloud),
            KeyCode::Char('s') => self.switch_screen(Screen::Settings),
            _ => {}
        }
    }

    fn handle_game_editor_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('n') => self.prompt_add_game(),
            KeyCode::Char('a') => self.prompt_context_add(),
            KeyCode::Char('e') | KeyCode::Char('P') => self.prompt_edit_selected_path(),
            KeyCode::Char('w') => self.prompt_rename_game(),
            KeyCode::Char('v') => self.scan_vns(),
            _ => {}
        }
    }

    fn handle_ludusavi_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('f') => self.toggle_ludusavi_filter(),
            KeyCode::Char('u') => self.submit(Operation::UpdateManifest),
            KeyCode::Char('!') => self.submit(Operation::ResetManifest),
            KeyCode::Char('i') => self.import_selected(),
            KeyCode::Char('e') => self.prompt_edit_import_path(),
            _ => {}
        }
    }

    fn handle_cloud_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('c') => self.cloud_action(PendingAction::CheckCloud),
            KeyCode::Char('e') | KeyCode::Char('E') => {
                self.cloud_action(PendingAction::EditCloudSettings)
            }
            KeyCode::Char('u') => self.cloud_action(PendingAction::UploadAll),
            KeyCode::Char('o') => self.cloud_action(PendingAction::DownloadAll),
            KeyCode::Char('y') => self.cloud_action(PendingAction::SyncSelected),
            KeyCode::Char('a') | KeyCode::Char('Y') => self.cloud_action(PendingAction::SyncAll),
            KeyCode::Char('t') => self.toggle_setting(),
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.cloud_action(PendingAction::ResolveKeepLocal)
            }
            KeyCode::Char('p') | KeyCode::Char('U') => {
                self.cloud_action(PendingAction::ResolveUseCloud)
            }
            KeyCode::Char('z') => self.cancel_operation(),
            _ => {}
        }
    }

    fn handle_settings_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('n') => self.prompt_edit_current_device_name(),
            KeyCode::Char('r') => self.prompt_add_current_device_root(),
            KeyCode::Char('g') => self.prompt_import_gui_profile(),
            KeyCode::Char('v') => self.scan_vns(),
            KeyCode::Char('t') => self.toggle_setting(),
            _ => {}
        }
    }

    fn handle_modal_key(&mut self, key: KeyEvent) -> Result<()> {
        let Some(mut modal) = self.modal.take() else {
            return Ok(());
        };
        match modal.kind {
            ModalKind::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter) {
                    self.modal = None;
                } else {
                    self.modal = Some(modal);
                }
            }
            ModalKind::Message => match key.code {
                KeyCode::Esc | KeyCode::Enter => self.modal = None,
                KeyCode::Char('L') | KeyCode::Char('l') => {
                    self.modal = None;
                    self.switch_screen(Screen::Logs);
                }
                _ => self.modal = Some(modal),
            },
            ModalKind::Confirm => match key.code {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let action = modal.action.clone();
                    self.run_pending_action(action, modal.input)?;
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    if matches!(modal.action, PendingAction::AcknowledgeExperimentalWarning) {
                        self.should_quit = true;
                    } else {
                        self.status = rust_i18n::t!("tui.status.cancelled").to_string();
                    }
                }
                _ => self.modal = Some(modal),
            },
            ModalKind::Prompt => match key.code {
                KeyCode::Enter => {
                    let input = modal.input.trim().to_string();
                    let action = modal.action.clone();
                    self.run_pending_action(action, input)?;
                }
                KeyCode::Esc => self.status = rust_i18n::t!("tui.status.cancelled").to_string(),
                KeyCode::Backspace => {
                    modal.input.pop();
                    self.modal = Some(modal);
                }
                KeyCode::Tab => {
                    if let Some(completion) = crate::completion::complete_path(&modal.input).first()
                    {
                        modal.input = completion.clone();
                    }
                    self.modal = Some(modal);
                }
                KeyCode::Char(value) => {
                    modal.input.push(value);
                    self.modal = Some(modal);
                }
                _ => self.modal = Some(modal),
            },
        }
        Ok(())
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.move_selection(-1),
            MouseEventKind::ScrollDown => self.move_selection(1),
            MouseEventKind::Down(_) => {
                let (width, height) = crossterm::terminal::size().unwrap_or_else(|_| {
                    (mouse.column.saturating_add(1), mouse.row.saturating_add(1))
                });
                self.pane = pane_at_column(self.screen, mouse.column, width);
                self.select_visible_row(mouse.row.saturating_sub(4) as usize, height);
            }
            _ => {}
        }
    }

    fn switch_screen(&mut self, screen: Screen) {
        self.screen = screen;
        self.pane = Pane::Left;
        self.clamp_selection();
    }

    fn next_pane(&mut self) {
        self.pane = next_pane_for_screen(self.screen, self.pane);
    }

    fn prev_pane(&mut self) {
        self.pane = prev_pane_for_screen(self.screen, self.pane);
    }

    fn move_selection(&mut self, delta: isize) {
        if self.screen == Screen::Logs {
            if delta < 0 {
                self.selection.log_scroll = self
                    .selection
                    .log_scroll
                    .saturating_sub(delta.unsigned_abs() as u16);
            } else {
                self.selection.log_scroll = self.selection.log_scroll.saturating_add(delta as u16);
            }
            return;
        }

        let selected = match (self.screen, self.pane) {
            (Screen::Home | Screen::GameEditor | Screen::Cloud, Pane::Left) => {
                &mut self.selection.game
            }
            (Screen::Home | Screen::Cloud, _) => &mut self.selection.snapshot,
            (Screen::GameEditor, Pane::Middle) => &mut self.selection.save_unit,
            (Screen::GameEditor, Pane::Right) => &mut self.selection.device,
            (Screen::Ludusavi, Pane::Left) => &mut self.selection.importable,
            (Screen::Ludusavi, _) => &mut self.selection.import_path,
            (Screen::Settings, _) => &mut self.selection.settings,
            (Screen::Logs, _) => unreachable!("Logs selection is handled before this match"),
        };
        if delta < 0 {
            *selected = selected.saturating_sub(delta.unsigned_abs());
        } else {
            *selected = selected.saturating_add(delta as usize);
        }
        self.clamp_selection();
        self.refresh_selected_snapshots();
    }

    fn select_visible_row(&mut self, row: usize, terminal_height: u16) {
        let visible_rows = list_visible_rows_for_terminal(terminal_height);
        match (self.screen, self.pane) {
            (Screen::Home | Screen::GameEditor, Pane::Left) => {
                self.selection.game = clicked_list_index(
                    row,
                    self.selection.game,
                    self.visible_game_indices().len(),
                    visible_rows,
                );
            }
            (Screen::Cloud, Pane::Left) => {
                if let Some(game_row) = cloud_game_row(
                    row,
                    self.selection.game,
                    self.visible_game_indices().len(),
                    table_visible_rows_for_terminal(terminal_height),
                ) {
                    self.selection.game = game_row;
                }
            }
            (Screen::Home, Pane::Middle) => {
                let snapshot_count = self
                    .data
                    .selected_snapshots
                    .as_ref()
                    .map(|snapshots| snapshots.backups.len())
                    .unwrap_or_default();
                self.selection.snapshot =
                    clicked_list_index(row, self.selection.snapshot, snapshot_count, visible_rows);
            }
            (Screen::GameEditor, Pane::Middle) => {
                let save_unit_count = self
                    .selected_game()
                    .map(|game| game.save_paths.len())
                    .unwrap_or_default();
                self.selection.save_unit = clicked_list_index(
                    row,
                    self.selection.save_unit,
                    save_unit_count,
                    visible_rows,
                );
            }
            (Screen::GameEditor, Pane::Right) => self.selection.device = row,
            (Screen::Ludusavi, Pane::Left) => {
                self.selection.importable = clicked_list_index(
                    row,
                    self.selection.importable,
                    self.visible_importable_indices().len(),
                    visible_rows,
                );
            }
            (Screen::Ludusavi, _) => {
                self.selection.import_path = clicked_list_index(
                    row,
                    self.selection.import_path,
                    self.selected_import_paths().len(),
                    visible_rows,
                );
            }
            (Screen::Settings, _) => self.selection.settings = row,
            (Screen::Logs, _) => self.selection.log_scroll = row as u16,
            _ => {}
        }
        self.clamp_selection();
        self.refresh_selected_snapshots();
    }

    fn clamp_selection(&mut self) {
        self.selection.game = clamp(self.selection.game, self.visible_game_indices().len());
        if let Some(snapshots) = &self.data.selected_snapshots {
            self.selection.snapshot = clamp(self.selection.snapshot, snapshots.backups.len());
        } else {
            self.selection.snapshot = 0;
        }
        if let Some(game) = self.selected_game() {
            self.selection.save_unit = clamp(self.selection.save_unit, game.save_paths.len());
            self.selection.device = clamp(self.selection.device, self.device_rows().len());
        } else {
            self.selection.save_unit = 0;
            self.selection.device = 0;
        }
        self.selection.importable = clamp(
            self.selection.importable,
            self.visible_importable_indices().len(),
        );
        self.selection.import_path = clamp(
            self.selection.import_path,
            self.selected_import_paths().len(),
        );
        self.selection.settings = clamp(self.selection.settings, SettingsItem::ALL.len());
    }

    fn default_action(&mut self) {
        match self.screen {
            Screen::Home => self.prompt_create_snapshot(None),
            Screen::GameEditor if self.pane == Pane::Left => self.next_pane(),
            Screen::GameEditor if self.pane == Pane::Right => self.prompt_edit_selected_path(),
            Screen::GameEditor => self.prompt_context_add(),
            Screen::Ludusavi => self.import_selected(),
            Screen::Cloud => self.cloud_action(PendingAction::SyncSelected),
            Screen::Settings => self.toggle_setting(),
            Screen::Logs => self.show_help(),
        }
    }
}

fn clamp(value: usize, len: usize) -> usize {
    if len == 0 { 0 } else { value.min(len - 1) }
}

fn cloud_game_row(
    row: usize,
    selected_game: usize,
    game_count: usize,
    visible_rows: usize,
) -> Option<usize> {
    let visible_body_row = row.checked_sub(1)?;
    let selected_body_row = selected_game.saturating_add(1);
    let body_row_count = game_count.saturating_add(1);
    let body_offset = list_scroll_offset(selected_body_row, body_row_count, visible_rows);
    body_offset
        .saturating_add(visible_body_row)
        .checked_sub(1)
        .filter(|game_row| *game_row < game_count)
}

pub(crate) fn list_scroll_offset(selected: usize, item_count: usize, visible_rows: usize) -> usize {
    if item_count == 0 || visible_rows == 0 {
        return 0;
    }
    clamp(selected, item_count)
        .saturating_add(1)
        .saturating_sub(visible_rows.min(item_count))
}

pub(crate) fn list_visible_rows(area: Rect) -> usize {
    usize::from(area.height.saturating_sub(2)).max(1)
}

pub(crate) fn table_visible_rows(area: Rect) -> usize {
    usize::from(area.height.saturating_sub(3)).max(1)
}

fn list_visible_rows_for_terminal(height: u16) -> usize {
    usize::from(height.saturating_sub(8)).max(1)
}

fn table_visible_rows_for_terminal(height: u16) -> usize {
    usize::from(height.saturating_sub(9)).max(1)
}

fn clicked_list_index(
    row: usize,
    selected: usize,
    item_count: usize,
    visible_rows: usize,
) -> usize {
    clamp(
        list_scroll_offset(selected, item_count, visible_rows).saturating_add(row),
        item_count,
    )
}

fn next_pane_for_screen(screen: Screen, pane: Pane) -> Pane {
    if is_two_pane_screen(screen) {
        return toggle_two_pane(pane);
    }
    match pane {
        Pane::Left => Pane::Middle,
        Pane::Middle => Pane::Right,
        Pane::Right => Pane::Left,
    }
}

fn prev_pane_for_screen(screen: Screen, pane: Pane) -> Pane {
    if is_two_pane_screen(screen) {
        return toggle_two_pane(pane);
    }
    match pane {
        Pane::Left => Pane::Right,
        Pane::Middle => Pane::Left,
        Pane::Right => Pane::Middle,
    }
}

fn is_two_pane_screen(screen: Screen) -> bool {
    matches!(screen, Screen::Ludusavi | Screen::Cloud | Screen::Settings)
}

fn toggle_two_pane(pane: Pane) -> Pane {
    if pane == Pane::Left {
        Pane::Right
    } else {
        Pane::Left
    }
}

fn pane_at_column(screen: Screen, column: u16, width: u16) -> Pane {
    let area = Rect::new(0, 0, width.max(1), 1);
    let chunks = match screen {
        Screen::Home => Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(32),
                Constraint::Percentage(38),
                Constraint::Percentage(30),
            ])
            .split(area),
        Screen::GameEditor => Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(32),
                Constraint::Percentage(34),
                Constraint::Percentage(34),
            ])
            .split(area),
        Screen::Ludusavi => Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
            .split(area),
        Screen::Cloud => Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area),
        Screen::Settings => Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(area),
        Screen::Logs => return Pane::Left,
    };
    pane_from_chunks(column, &chunks)
}

fn pane_from_chunks(column: u16, chunks: &[Rect]) -> Pane {
    let index = chunks
        .iter()
        .position(|chunk| {
            let end = chunk.x.saturating_add(chunk.width);
            column >= chunk.x && column < end
        })
        .unwrap_or_else(|| chunks.len().saturating_sub(1));
    match (chunks.len(), index) {
        (_, 0) => Pane::Left,
        (2, _) => Pane::Right,
        (_, 1) => Pane::Middle,
        _ => Pane::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_pane_uses_cloud_two_column_layout() {
        assert_eq!(pane_at_column(Screen::Cloud, 57, 100), Pane::Left);
        assert_eq!(pane_at_column(Screen::Cloud, 58, 100), Pane::Right);
    }

    #[test]
    fn mouse_pane_uses_ludusavi_two_column_layout() {
        assert_eq!(pane_at_column(Screen::Ludusavi, 43, 100), Pane::Left);
        assert_eq!(pane_at_column(Screen::Ludusavi, 44, 100), Pane::Right);
    }

    #[test]
    fn mouse_pane_uses_three_column_layout() {
        assert_eq!(pane_at_column(Screen::GameEditor, 31, 100), Pane::Left);
        assert_eq!(pane_at_column(Screen::GameEditor, 32, 100), Pane::Middle);
        assert_eq!(pane_at_column(Screen::GameEditor, 66, 100), Pane::Right);
    }

    #[test]
    fn cloud_mouse_rows_skip_config_row() {
        assert_eq!(cloud_game_row(0, 0, 2, 5), None);
        assert_eq!(cloud_game_row(1, 0, 2, 5), None);
        assert_eq!(cloud_game_row(2, 0, 2, 5), Some(0));
        assert_eq!(cloud_game_row(3, 0, 2, 5), Some(1));
    }

    #[test]
    fn cloud_mouse_rows_include_table_scroll_offset() {
        assert_eq!(cloud_game_row(1, 10, 20, 5), Some(6));
        assert_eq!(cloud_game_row(2, 10, 20, 5), Some(7));
        assert_eq!(cloud_game_row(5, 10, 20, 5), Some(10));
        assert_eq!(cloud_game_row(5, 19, 20, 5), Some(19));
    }

    #[test]
    fn cloud_keyboard_pane_switching_uses_two_column_layout() {
        assert_eq!(next_pane_for_screen(Screen::Cloud, Pane::Left), Pane::Right);
        assert_eq!(next_pane_for_screen(Screen::Cloud, Pane::Right), Pane::Left);
        assert_eq!(
            next_pane_for_screen(Screen::Cloud, Pane::Middle),
            Pane::Left
        );
        assert_eq!(prev_pane_for_screen(Screen::Cloud, Pane::Left), Pane::Right);
        assert_eq!(prev_pane_for_screen(Screen::Cloud, Pane::Right), Pane::Left);
        assert_eq!(
            prev_pane_for_screen(Screen::Cloud, Pane::Middle),
            Pane::Left
        );
    }

    #[test]
    fn clicked_list_rows_include_scroll_offset() {
        assert_eq!(clicked_list_index(0, 2, 20, 5), 0);
        assert_eq!(clicked_list_index(0, 10, 20, 5), 6);
        assert_eq!(clicked_list_index(3, 10, 20, 5), 9);
        assert_eq!(clicked_list_index(8, 19, 20, 5), 19);
    }

    #[test]
    fn list_scroll_offset_handles_empty_and_short_lists() {
        assert_eq!(list_scroll_offset(0, 0, 5), 0);
        assert_eq!(list_scroll_offset(4, 5, 10), 0);
        assert_eq!(list_scroll_offset(9, 10, 5), 5);
    }
}
