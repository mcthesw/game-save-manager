use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{List, ListItem, ListState, Paragraph, Row, Table, TableState, Wrap},
};
use rgsm_core::{
    backup::{Game, SaveUnitType, list_extra_backups},
    cloud_sync::Backend,
    device::get_current_device_id,
    path_pattern::StoreKind,
};
use rust_i18n::t;

use crate::{
    app::{App, list_scroll_offset, list_visible_rows, table_visible_rows},
    logging::LogLevel,
    model::Pane,
};

use super::{
    backend_label, bool_label, created_by_label, option, panel_block, save_unit_type_label,
    selected_style, sort_label, sync_error, sync_state_label, validate_path_label,
};

pub(super) fn draw_home(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(32),
            Constraint::Percentage(38),
            Constraint::Percentage(30),
        ])
        .split(area);
    draw_games(frame, chunks[0], app);
    draw_snapshots(frame, chunks[1], app);
    draw_snapshot_detail(frame, chunks[2], app);
}

pub(super) fn draw_game_editor(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(32),
            Constraint::Percentage(34),
            Constraint::Percentage(34),
        ])
        .split(area);
    draw_games(frame, chunks[0], app);
    draw_editor_units(frame, chunks[1], app);
    draw_editor_detail(frame, chunks[2], app);
}

pub(super) fn draw_cloud(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(area);

    let game_indices = app.visible_game_indices();
    let mut rows = vec![Row::new(vec![
        t!("sync_settings.overview.config_row").to_string(),
        t!("sync_settings.overview.always_synced").to_string(),
        sync_state_label(&app.data.sync_state.config_state),
        app.data
            .sync_state
            .config_state
            .last_sync_at
            .clone()
            .unwrap_or_else(|| "-".to_string()),
    ])];
    rows.extend(game_indices.iter().map(|index| {
        let game = &app.data.games[*index];
        let state = app.data.sync_state.games.get(&game.name);
        Row::new(vec![
            game.name.clone(),
            bool_label(game.cloud_sync_enabled),
            state
                .map(sync_state_label)
                .unwrap_or_else(|| t!("sync_settings.overview.status_unknown").to_string()),
            state
                .and_then(|state| state.last_sync_at.clone())
                .unwrap_or_else(|| "-".to_string()),
        ])
    }));

    let selected_row = (!game_indices.is_empty()).then_some(app.selection.game.saturating_add(1));
    let mut state = TableState::default()
        .with_offset(
            selected_row
                .map(|row| {
                    list_scroll_offset(row, game_indices.len() + 1, table_visible_rows(chunks[0]))
                })
                .unwrap_or_default(),
        )
        .with_selected(selected_row);
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(35),
            Constraint::Percentage(12),
            Constraint::Percentage(20),
            Constraint::Percentage(33),
        ],
    )
    .header(
        Row::new(vec![
            t!("sync_settings.overview.game_name").to_string(),
            t!("sync_settings.overview.cloud_sync").to_string(),
            t!("sync_settings.overview.status").to_string(),
            t!("sync_settings.overview.last_sync").to_string(),
        ])
        .style(Style::default().fg(Color::Cyan)),
    )
    .row_highlight_style(Style::default().fg(Color::Black).bg(Color::Yellow))
    .highlight_symbol("▶ ")
    .block(panel_block(t!("sync_settings.overview.tab").as_ref(), true));
    frame.render_stateful_widget(table, chunks[0], &mut state);

    frame.render_widget(
        Paragraph::new(cloud_detail_lines(app).join("\n"))
            .wrap(Wrap { trim: false })
            .block(panel_block(
                t!("sync_settings.backend_tab.tab").as_ref(),
                false,
            )),
        chunks[1],
    );
}

pub(super) fn draw_logs(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let all_items = app
        .log
        .lock()
        .ok()
        .map(|log| {
            log.entries()
                .rev()
                .map(|entry| {
                    let style = match &entry.level {
                        LogLevel::Info => Style::default().fg(Color::White),
                        LogLevel::Warning => Style::default().fg(Color::Yellow),
                        LogLevel::Error => Style::default().fg(Color::Red),
                    };
                    let level = match &entry.level {
                        LogLevel::Info => "INFO ",
                        LogLevel::Warning => "WARN ",
                        LogLevel::Error => "ERROR",
                    };
                    ListItem::new(format!("{level}  {}  {}", entry.timestamp, entry.message))
                        .style(style)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let items = if all_items.is_empty() {
        vec![ListItem::new(t!("activity_center.empty").to_string())]
    } else {
        let visible_rows = usize::from(area.height.saturating_sub(2)).max(1);
        let max_offset = all_items.len().saturating_sub(visible_rows);
        let offset = usize::from(app.selection.log_scroll).min(max_offset);
        all_items.into_iter().skip(offset).collect()
    };
    frame.render_widget(
        List::new(items).block(panel_block(t!("tui.screen.logs").as_ref(), true)),
        area,
    );
}

fn draw_games(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let indices = app.visible_game_indices();
    let items = if app.data.games.is_empty() {
        vec![ListItem::new(
            t!("sync_settings.overview.no_games").to_string(),
        )]
    } else if indices.is_empty() {
        vec![ListItem::new(t!("misc.no_search_results").to_string())]
    } else {
        indices
            .iter()
            .enumerate()
            .map(|(visible_index, index)| {
                let game = &app.data.games[*index];
                let cloud = if game.cloud_sync_enabled { "☁" } else { " " };
                ListItem::new(format!("{cloud} {}", game.name)).style(selected_style(
                    visible_index == app.selection.game && app.pane == Pane::Left,
                ))
            })
            .collect()
    };
    let title = list_title(
        t!("sync_settings.overview.game_name").as_ref(),
        &app.game_filter,
        app.game_sort,
        indices.len(),
    );
    let mut state = ListState::default()
        .with_offset(list_scroll_offset(
            app.selection.game,
            indices.len(),
            list_visible_rows(area),
        ))
        .with_selected((!indices.is_empty()).then_some(app.selection.game));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Yellow))
            .highlight_symbol("▶ ")
            .block(panel_block(title, app.pane == Pane::Left)),
        area,
        &mut state,
    );
}

fn draw_snapshots(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = app
        .data
        .selected_snapshots
        .as_ref()
        .map(|snapshots| {
            if snapshots.backups.is_empty() {
                return vec![ListItem::new(t!("manage.no_snapshots").to_string())];
            }
            let current = snapshots.current_device_head().cloned();
            snapshots
                .backups
                .iter()
                .enumerate()
                .map(|(index, snapshot)| {
                    let marker = if current.as_deref() == Some(snapshot.date.as_str()) {
                        "●"
                    } else {
                        " "
                    };
                    ListItem::new(format!(
                        "{marker} {} | {} | {} bytes",
                        snapshot.date,
                        created_by_label(&snapshot.created_by),
                        snapshot.size
                    ))
                    .style(selected_style(index == app.selection.snapshot))
                })
                .collect()
        })
        .unwrap_or_else(|| vec![ListItem::new(t!("manage.no_snapshots").to_string())]);
    let snapshot_count = app
        .data
        .selected_snapshots
        .as_ref()
        .map(|snapshots| snapshots.backups.len())
        .unwrap_or_default();
    let mut state = ListState::default()
        .with_offset(list_scroll_offset(
            app.selection.snapshot,
            snapshot_count,
            list_visible_rows(area),
        ))
        .with_selected((snapshot_count > 0).then_some(app.selection.snapshot));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Yellow))
            .highlight_symbol("▶ ")
            .block(panel_block(
                t!("tui.panel.snapshots").as_ref(),
                app.pane == Pane::Middle,
            )),
        area,
        &mut state,
    );
}

fn draw_snapshot_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = app
        .data
        .selected_snapshots
        .as_ref()
        .and_then(|snapshots| {
            snapshots
                .backups
                .get(app.selection.snapshot)
                .map(|snapshot| {
                    vec![
                        format!("{}: {}", t!("manage.save_date"), snapshot.date),
                        format!("{}: {}", t!("manage.description"), snapshot.describe),
                        format!("{}: {}", t!("manage.size"), snapshot.size),
                        format!(
                            "{}: {}",
                            t!("tui.snapshot.source"),
                            created_by_label(&snapshot.created_by)
                        ),
                        format!(
                            "{}: {}",
                            t!("settings.device_id"),
                            snapshot.device_id.as_deref().unwrap_or("-")
                        ),
                        format!(
                            "{}: {}",
                            t!("tui.snapshot.parent"),
                            snapshot.parent.as_deref().unwrap_or("-")
                        ),
                        format!(
                            "{}: {}",
                            t!("manage.extra_backups"),
                            app.extra_backup_count()
                        ),
                    ]
                })
        })
        .unwrap_or_else(|| vec![t!("tui.empty.select_snapshot").to_string()]);
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .wrap(Wrap { trim: false })
            .block(panel_block(t!("tui.panel.snapshot_detail").as_ref(), false)),
        area,
    );
}

fn draw_editor_units(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let selected_game = app.selected_game();
    let save_unit_count = selected_game
        .as_ref()
        .map(|game| game.save_paths.len())
        .unwrap_or_default();
    let items = selected_game
        .as_ref()
        .map(|game| {
            if game.save_paths.is_empty() {
                return vec![ListItem::new(
                    t!("save_location_drawer.no_active_paths").to_string(),
                )];
            }
            game.save_paths
                .iter()
                .enumerate()
                .map(|(index, unit)| {
                    ListItem::new(format!(
                        "#{} {} [{}]",
                        unit.id,
                        unit.unit_type()
                            .map(save_unit_type_label)
                            .unwrap_or_else(|| t!("path_variable.tooltip").to_string()),
                        bool_label(unit.enabled)
                    ))
                    .style(selected_style(index == app.selection.save_unit))
                })
                .collect()
        })
        .unwrap_or_else(|| vec![ListItem::new(t!("tui.empty.select_game").to_string())]);
    let mut state = ListState::default()
        .with_offset(list_scroll_offset(
            app.selection.save_unit,
            save_unit_count,
            list_visible_rows(area),
        ))
        .with_selected((save_unit_count > 0).then_some(app.selection.save_unit));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Yellow))
            .highlight_symbol("▶ ")
            .block(panel_block(
                t!("save_location_drawer.save_locations").as_ref(),
                app.pane == Pane::Middle,
            )),
        area,
        &mut state,
    );
}

fn draw_editor_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = app
        .selected_game()
        .map(|game| editor_detail_lines(&game, app))
        .unwrap_or_else(|| vec![t!("tui.empty.select_game").to_string()]);
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .wrap(Wrap { trim: false })
            .block(panel_block(
                t!("tui.panel.editor_detail").as_ref(),
                app.pane == Pane::Right,
            )),
        area,
    );
}

fn editor_detail_lines(game: &Game, app: &App) -> Vec<String> {
    let device_id = app
        .device_rows()
        .get(app.selection.device)
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| get_current_device_id().clone());
    let unit = game.save_paths.get(app.selection.save_unit);
    let store_user_id = app
        .data
        .config
        .devices
        .get(&device_id)
        .and_then(|device| game.path_context(Some(device)).store_user_id);
    let mut lines = vec![
        format!("{}: {}", t!("addgame.game_name"), game.name),
        format!(
            "{}: {}",
            t!("tui.editor.selected_path"),
            app.selection.save_unit + 1
        ),
        format!("{}: {}", t!("settings.device_id"), device_id),
        format!(
            "{}: {}",
            t!("save_location_drawer.launch_path"),
            game.game_paths
                .get(&device_id)
                .map(String::as_str)
                .unwrap_or("-")
        ),
        format!(
            "{}: {}",
            t!("game_batch_import.store_user_id"),
            store_user_id.as_deref().unwrap_or("-")
        ),
    ];
    if let Some(unit) = unit {
        lines.extend([
            String::new(),
            format!(
                "{}: {}",
                t!("addgame.type"),
                unit.unit_type()
                    .map(save_unit_type_label)
                    .unwrap_or_else(|| t!("path_variable.tooltip").to_string())
            ),
            format!(
                "{}: {}",
                t!("save_location_drawer.backup_enabled"),
                bool_label(unit.enabled)
            ),
            format!(
                "{}: {}",
                t!("save_location_drawer.delete_before_apply"),
                bool_label(unit.delete_before_apply)
            ),
            format!(
                "{}: {}",
                t!("addgame.path"),
                unit.paths()
                    .and_then(|paths| paths.get(&device_id))
                    .map(String::as_str)
                    .or_else(|| unit.manifest_pattern().map(|(pattern, _)| pattern.raw()))
                    .unwrap_or("-")
            ),
            format!(
                "{}: {}",
                t!("tui.path.validation"),
                validate_path_label(
                    unit.paths()
                        .and_then(|paths| paths.get(&device_id))
                        .map(String::as_str)
                        .or_else(|| unit.manifest_pattern().map(|(pattern, _)| pattern.raw()))
                )
            ),
        ]);
    }
    lines.push(String::new());
    lines.push(format!("{}:", t!("settings.device_settings")));
    for (index, (id, device)) in app.device_rows().iter().enumerate() {
        let selected = if index == app.selection.device {
            ">"
        } else {
            " "
        };
        let current = if id == get_current_device_id() {
            "*"
        } else {
            " "
        };
        lines.push(format!("{selected}{current} {} ({id})", device.name));
    }
    if matches!(
        unit.and_then(|unit| unit.unit_type()),
        Some(SaveUnitType::WinRegistry)
    ) {
        lines.push(t!("game_import_customize.registry").to_string());
    }
    if let Some(meta) = &game.ludusavi_meta {
        lines.extend([
            String::new(),
            format!(
                "{}: {}",
                t!("game_import.steam_id"),
                meta.store_game_id(StoreKind::Steam)
                    .map(str::to_string)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "{}: {}",
                t!("addgame.install_dirs"),
                meta.install_dirs.join(", ")
            ),
        ]);
    }
    lines.extend([String::new(), t!("tui.help.editor_actions").to_string()]);
    lines
}

fn list_title(base: &str, filter: &str, sort: crate::model::ListSort, count: usize) -> String {
    let sort = sort_label(sort);
    if filter.is_empty() {
        format!("{base} ({count}) [{sort}]")
    } else {
        format!("{base} ({count}) [{sort}] /{filter}")
    }
}

fn cloud_detail_lines(app: &App) -> Vec<String> {
    let settings = &app.data.config.settings.cloud_settings;
    let root_label = if matches!(&settings.backend, Backend::Fs) {
        t!("sync_settings.fs.root")
    } else {
        t!("sync_settings.cloud_root")
    };
    let mut lines = vec![
        format!(
            "{}: {}",
            t!("sync_settings.backend"),
            backend_label(&settings.backend)
        ),
        format!("{}: {}", root_label, settings.root_path),
        format!(
            "{}: {}",
            t!("sync_settings.max_concurrency"),
            settings.max_concurrency
        ),
        String::new(),
    ];
    match &settings.backend {
        Backend::Disabled => lines.push(t!("sync_settings.overview.status_disabled").to_string()),
        Backend::Fs => {}
        Backend::WebDAV {
            endpoint, username, ..
        } => {
            lines.push(format!(
                "{}: {}",
                t!("sync_settings.webdav.endpoint"),
                endpoint
            ));
            lines.push(format!(
                "{}: {}",
                t!("sync_settings.webdav.username"),
                username
            ));
            lines.push(format!("{}: ******", t!("sync_settings.webdav.password")));
        }
        Backend::S3 {
            endpoint,
            bucket,
            region,
            access_key_id,
            addressing_style,
            ..
        } => {
            lines.push(format!("{}: {}", t!("sync_settings.s3.endpoint"), endpoint));
            lines.push(format!("{}: {}", t!("sync_settings.s3.bucket"), bucket));
            lines.push(format!("{}: {}", t!("sync_settings.s3.region"), region));
            lines.push(format!(
                "{}: {}",
                t!("sync_settings.s3.access_key_id"),
                access_key_id
            ));
            lines.push(format!(
                "{}: {:?}",
                t!("sync_settings.s3.addressing_style"),
                addressing_style
            ));
        }
    }

    if let Some(game) = app.selected_game() {
        lines.extend([
            String::new(),
            format!("{}: {}", t!("addgame.game_name"), game.name),
            format!(
                "{}: {}",
                t!("sync_settings.overview.cloud_sync"),
                game.cloud_sync_enabled
            ),
        ]);
        if let Some(state) = app.data.sync_state.games.get(&game.name) {
            lines.push(format!(
                "{}: {}",
                t!("tui.cloud.local_head"),
                option(&state.last_known_local_head)
            ));
            lines.push(format!(
                "{}: {}",
                t!("tui.cloud.remote_head"),
                option(&state.last_known_remote_head)
            ));
            lines.push(format!(
                "{}: {}",
                t!("sync_settings.overview.status"),
                sync_state_label(state)
            ));
            if let Some(error) = sync_error(state) {
                lines.push(format!("{}: {}", t!("misc.error"), error));
            }
        }
        let extra_count = list_extra_backups(&game)
            .map(|items| items.len())
            .unwrap_or_default();
        lines.push(format!("{}: {}", t!("manage.extra_backups"), extra_count));
    }
    lines.extend([String::new(), t!("tui.help.cloud_actions").to_string()]);
    lines
}
