use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};
use rust_i18n::t;

use crate::{
    app::{App, list_scroll_offset, list_visible_rows},
    model::Pane,
};

use super::{panel_block, selected_style};

pub(super) fn draw_ludusavi(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(area);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(chunks[1]);

    draw_importable_games(frame, chunks[0], app);
    draw_import_detail(frame, right[0], app);
    draw_import_paths(frame, right[1], app);
}

fn draw_importable_games(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let indices = app.visible_importable_indices();
    let title = format!(
        "{} - {} ({}){}",
        t!("tui.panel.importable_games"),
        if app.settings.ludusavi_local_only {
            t!("tui.ludusavi.scope_local")
        } else {
            t!("tui.ludusavi.scope_all")
        },
        indices.len(),
        if app.import_filter.is_empty() {
            String::new()
        } else {
            format!(" /{}", app.import_filter)
        }
    );
    let items = if app.data.importable_games.is_empty() {
        vec![ListItem::new(t!("tui.empty.no_importable").to_string())]
    } else if indices.is_empty() {
        vec![ListItem::new(t!("misc.no_search_results").to_string())]
    } else {
        indices
            .iter()
            .enumerate()
            .map(|(visible_index, index)| {
                let candidate = &app.data.importable_games[*index];
                let game = &candidate.game;
                let state = if game.is_managed {
                    t!("game_import.managed").to_string()
                } else {
                    t!("game_import.unmanaged").to_string()
                };
                ListItem::new(format!(
                    "{:<38} {:>3} {}  {}",
                    game.name,
                    game.save_paths_count,
                    t!("game_batch_import.paths"),
                    state
                ))
                .style(selected_style(
                    visible_index == app.selection.importable && app.pane == Pane::Left,
                ))
            })
            .collect()
    };
    let mut state = ListState::default()
        .with_offset(list_scroll_offset(
            app.selection.importable,
            indices.len(),
            list_visible_rows(area),
        ))
        .with_selected((!indices.is_empty()).then_some(app.selection.importable));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().fg(Color::White).bg(Color::Blue))
            .highlight_symbol("> ")
            .block(panel_block(title, app.pane == Pane::Left)),
        area,
        &mut state,
    );
}

fn draw_import_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = vec![
        format!(
            "{}: {}",
            t!("tui.ludusavi.filter"),
            if app.settings.ludusavi_local_only {
                t!("tui.ludusavi.scope_local")
            } else {
                t!("tui.ludusavi.scope_all")
            }
        ),
        format!(
            "{}: {}",
            t!("settings.manifest_source"),
            app.data.manifest_status.source
        ),
        format!(
            "{}: {}",
            t!("settings.manifest_updated_at"),
            app.data
                .manifest_status
                .updated_at
                .as_deref()
                .unwrap_or("-")
        ),
        format!(
            "{}: {}",
            t!("settings.manifest_etag"),
            app.data.manifest_status.etag.as_deref().unwrap_or("-")
        ),
        String::new(),
    ];
    if let Some(candidate) = app.selected_import_candidate() {
        let game = &candidate.game;
        lines.extend([
            format!("{}: {}", t!("addgame.game_name"), game.name),
            format!(
                "{}: {}",
                t!("game_import.steam_id"),
                game.steam_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!("{}: {}", t!("game_import.managed"), game.is_managed),
            format!(
                "{}: {}",
                t!("game_import.save_paths_count"),
                game.save_paths_count
            ),
            format!(
                "{}: {}",
                t!("addgame.install_dirs"),
                if game.install_dirs.is_empty() {
                    "-".to_string()
                } else {
                    game.install_dirs.join(", ")
                }
            ),
            String::new(),
            t!("tui.help.ludusavi_actions").to_string(),
        ]);
    } else {
        lines.push(t!("tui.empty.select_importable").to_string());
    }
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .wrap(Wrap { trim: false })
            .block(panel_block(
                t!("tui.panel.import_detail").as_ref(),
                app.pane != Pane::Left,
            )),
        area,
    );
}

fn draw_import_paths(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let paths = app.selected_import_paths();
    let items = if paths.is_empty() {
        vec![ListItem::new(
            t!("save_location_drawer.no_active_paths").to_string(),
        )]
    } else {
        paths
            .iter()
            .enumerate()
            .map(|(index, path)| {
                ListItem::new(format!("{:>2}. {}", index + 1, path)).style(selected_style(
                    index == app.selection.import_path && app.pane != Pane::Left,
                ))
            })
            .collect()
    };
    let mut state = ListState::default()
        .with_offset(list_scroll_offset(
            app.selection.import_path,
            paths.len(),
            list_visible_rows(area),
        ))
        .with_selected((!paths.is_empty()).then_some(app.selection.import_path));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().fg(Color::White).bg(Color::Blue))
            .highlight_symbol("> ")
            .block(panel_block(
                t!("game_batch_import.save_paths").as_ref(),
                app.pane != Pane::Left,
            )),
        area,
        &mut state,
    );
}
