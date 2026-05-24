use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use rgsm_core::{
    backup::{CreatedBy, SaveUnitType},
    cloud_sync::{Backend, GameSyncState, PendingAction as SyncPendingAction, SyncResult},
};
use rust_i18n::t;

use crate::{
    app::App,
    model::{ListSort, ModalKind, Screen},
};

mod ludusavi;
mod screens;
mod settings;

const MIN_WIDTH: u16 = 78;
const MIN_HEIGHT: u16 = 20;
const ACCENT: Color = Color::Cyan;
const ACTIVE: Color = Color::Yellow;
const MUTED: Color = Color::DarkGray;

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_small_terminal(frame, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    match app.screen {
        Screen::Home => screens::draw_home(frame, chunks[1], app),
        Screen::GameEditor => screens::draw_game_editor(frame, chunks[1], app),
        Screen::Ludusavi => ludusavi::draw_ludusavi(frame, chunks[1], app),
        Screen::Cloud => screens::draw_cloud(frame, chunks[1], app),
        Screen::Settings => settings::draw_settings(frame, chunks[1], app),
        Screen::Logs => screens::draw_logs(frame, chunks[1], app),
    }
    draw_footer(frame, chunks[2], app);

    if let Some(modal) = &app.modal {
        draw_modal(
            frame,
            area,
            modal.kind,
            &modal.title,
            &modal.message,
            &modal.input,
        );
    }
}

pub fn help_text() -> String {
    t!("tui.help.body").to_string()
}

fn draw_small_terminal(frame: &mut Frame<'_>, area: Rect) {
    let message = format!(
        "{}\n{}x{} < {}x{}",
        t!("tui.status.terminal_too_small"),
        area.width,
        area.height,
        MIN_WIDTH,
        MIN_HEIGHT
    );
    frame.render_widget(
        Paragraph::new(message)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("rgsm-tui")),
        area,
    );
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let tabs = [
        ("1", Screen::Home, t!("tui.screen.home").to_string()),
        ("2", Screen::GameEditor, t!("tui.screen.editor").to_string()),
        ("3", Screen::Ludusavi, t!("tui.screen.ludusavi").to_string()),
        ("4", Screen::Cloud, t!("tui.screen.cloud").to_string()),
        ("5", Screen::Settings, t!("tui.screen.settings").to_string()),
        ("6", Screen::Logs, t!("tui.screen.logs").to_string()),
    ];
    let mut spans = vec![Span::styled(
        " RGSM TUI ",
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )];
    for (key, screen, label) in tabs {
        let key_style = if screen == app.screen {
            Style::default().fg(ACTIVE).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(MUTED)
        };
        let label_style = if screen == app.screen {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(format!("[{key}]"), key_style));
        spans.push(Span::styled(label, label_style));
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(MUTED)),
        ),
        area,
    );
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let status = if app.operation_running {
        format!("{} {}", spinner(), app.status)
    } else {
        app.status.clone()
    };
    let status_style = if app.operation_running {
        Style::default().fg(ACTIVE).add_modifier(Modifier::BOLD)
    } else if app.modal.is_some() {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(Color::Gray)
    };
    let lines = vec![
        Line::from(Span::styled(status, status_style)),
        Line::from(Span::styled(
            footer_actions(app.screen),
            Style::default().fg(MUTED),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(MUTED)),
        ),
        area,
    );
}

fn footer_actions(screen: Screen) -> String {
    match screen {
        Screen::Home => t!("tui.footer.home").to_string(),
        Screen::GameEditor => t!("tui.footer.editor").to_string(),
        Screen::Ludusavi => t!("tui.footer.ludusavi").to_string(),
        Screen::Cloud => t!("tui.footer.cloud").to_string(),
        Screen::Settings => t!("tui.footer.settings").to_string(),
        Screen::Logs => t!("tui.footer.logs").to_string(),
    }
}

fn draw_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    kind: ModalKind,
    title: &str,
    message: &str,
    input: &str,
) {
    let modal_area = centered_rect(60, 40, area);
    frame.render_widget(Clear, modal_area);
    let mut lines = vec![message.to_string()];
    match kind {
        ModalKind::Prompt => {
            lines.push(String::new());
            lines.push(format!("> {input}"));
            lines.push(t!("tui.help.prompt").to_string());
        }
        ModalKind::Confirm => {
            lines.push(String::new());
            lines.push(t!("tui.help.confirm").to_string());
        }
        ModalKind::Help => {}
        ModalKind::Message => {
            lines.push(String::new());
            lines.push(t!("tui.help.message").to_string());
        }
    }
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::LightDoubleDashed)
                    .title(title.to_string())
                    .border_style(Style::default().fg(MUTED)),
            ),
        modal_area,
    );
}

fn spinner() -> &'static str {
    const FRAMES: [&str; 4] = ["|", "/", "-", "\\"];
    let index = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| (duration.as_millis() / 150) as usize % FRAMES.len())
        .unwrap_or_default();
    FRAMES[index]
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn panel_block(title: impl Into<String>, focused: bool) -> Block<'static> {
    let style = if focused {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(MUTED)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title.into())
        .border_style(style)
}

fn selected_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Black)
            .bg(ACTIVE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn sync_state_label(state: &GameSyncState) -> String {
    match state.pending_action {
        SyncPendingAction::UserDecisionRequired => {
            t!("sync_settings.overview.status_conflict").to_string()
        }
        SyncPendingAction::RetryRequired => t!("sync_settings.overview.status_failed").to_string(),
        SyncPendingAction::None => match &state.last_sync_result {
            Some(SyncResult::Success) => t!("sync_settings.overview.status_synced").to_string(),
            Some(SyncResult::Error(_)) => t!("sync_settings.overview.status_failed").to_string(),
            Some(SyncResult::Conflict) => t!("sync_settings.overview.status_conflict").to_string(),
            Some(SyncResult::Cancelled) => t!("cloud_sync.status_cancelled").to_string(),
            None => t!("sync_settings.overview.status_unknown").to_string(),
        },
    }
}

fn sync_error(state: &GameSyncState) -> Option<&str> {
    match &state.last_sync_result {
        Some(SyncResult::Error(error)) => Some(error.as_str()),
        _ => None,
    }
}

fn backend_label(backend: &Backend) -> String {
    match backend {
        Backend::Disabled => t!("sync_settings.backend_label.disabled").to_string(),
        Backend::WebDAV { .. } => t!("sync_settings.backend_label.webdav").to_string(),
        Backend::S3 { .. } => t!("sync_settings.backend_label.s3").to_string(),
    }
}

fn created_by_label(created_by: &CreatedBy) -> String {
    match created_by {
        CreatedBy::Manual => t!("tui.snapshot.created_by.manual").to_string(),
        CreatedBy::Timer => t!("tui.snapshot.created_by.timer").to_string(),
        CreatedBy::Tray => t!("tui.snapshot.created_by.tray").to_string(),
        CreatedBy::Hotkey => t!("tui.snapshot.created_by.hotkey").to_string(),
        CreatedBy::Unknown => t!("tui.snapshot.created_by.unknown").to_string(),
    }
}

fn save_unit_type_label(unit_type: &SaveUnitType) -> String {
    match unit_type {
        SaveUnitType::File => t!("save_location_drawer.type_file").to_string(),
        SaveUnitType::Folder => t!("save_location_drawer.type_folder").to_string(),
        SaveUnitType::WinRegistry => t!("game_import_customize.registry").to_string(),
    }
}

fn sort_label(sort: ListSort) -> String {
    match sort {
        ListSort::Natural => t!("tui.sort.natural").to_string(),
        ListSort::NameAsc => t!("tui.sort.name_asc").to_string(),
        ListSort::NameDesc => t!("tui.sort.name_desc").to_string(),
    }
}

fn bool_label(value: bool) -> String {
    if value {
        t!("tui.value.on").to_string()
    } else {
        t!("tui.value.off").to_string()
    }
}

fn validate_path_label(path: Option<&str>) -> String {
    let Some(path) = path else {
        return "-".to_string();
    };
    if path.starts_with("REGISTRY:") {
        return t!("game_import_customize.registry").to_string();
    }
    let path = std::path::Path::new(path);
    if path.is_dir() {
        t!("save_location_drawer.type_folder").to_string()
    } else if path.is_file() {
        t!("save_location_drawer.type_file").to_string()
    } else {
        t!("game_import_customize.status_missing").to_string()
    }
}

fn option(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("-")
}
