use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};
use rgsm_core::device::get_current_device_id;
use rust_i18n::t;

use crate::{
    app::App,
    model::{Pane, SettingsItem},
};

use super::{backend_label, panel_block, selected_style};

pub(super) fn draw_settings(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(chunks[1]);

    draw_settings_menu(frame, chunks[0], app);
    draw_selected_setting(frame, right[0], app);
    draw_settings_context(frame, right[1], app);
}

fn draw_settings_menu(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = SettingsItem::ALL
        .iter()
        .enumerate()
        .map(|(index, item)| {
            ListItem::new(format!(
                "{:<24} {}",
                setting_label(*item),
                setting_value(*item, app)
            ))
            .style(selected_style(index == app.selection.settings))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.selection.settings));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().fg(Color::White).bg(Color::Blue))
            .highlight_symbol("> ")
            .block(panel_block(
                t!("tui.panel.settings_actions").as_ref(),
                app.pane == Pane::Left,
            )),
        area,
        &mut state,
    );
}

fn draw_selected_setting(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let item = SettingsItem::ALL
        .get(app.selection.settings)
        .copied()
        .unwrap_or(SettingsItem::AutoCloudEnqueue);
    let mut lines = vec![
        setting_label(item),
        format!(
            "{}: {}",
            t!("settings.current_device"),
            get_current_device_id()
        ),
        format!(
            "{}: {}",
            t!("tui.settings.current_value"),
            setting_value(item, app)
        ),
        String::new(),
        setting_detail(item),
        String::new(),
        setting_action_hint(item),
    ];
    if matches!(item, SettingsItem::AddGameRoot) {
        lines.extend([String::new(), game_roots_line(app)]);
    } else if matches!(item, SettingsItem::AddVnScanRoot) {
        lines.extend([String::new(), vn_scan_dirs_line(app)]);
    }
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .wrap(Wrap { trim: false })
            .block(panel_block(
                t!("tui.panel.settings_detail").as_ref(),
                app.pane != Pane::Left,
            )),
        area,
    );
}

fn draw_settings_context(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let settings = &app.data.config.settings;
    let current_device_id = get_current_device_id();
    let current_device = app.data.config.devices.get(current_device_id);
    let lines = vec![
        format!(
            "{}: {}",
            t!("tui.settings.data_dir"),
            app.data_dir.display()
        ),
        format!("{}: {}", t!("settings.locale_name"), settings.locale),
        format!(
            "{}: {:?}",
            t!("settings.compression_preset"),
            settings.compression_preset
        ),
        format!(
            "{}: {}",
            t!("settings.compute_archive_hash"),
            settings.compute_archive_hash
        ),
        format!(
            "{}: {}",
            t!("settings.verify_archive_before_apply"),
            settings.verify_archive_before_apply
        ),
        format!(
            "{}: {}",
            t!("settings.extra_backup_when_apply"),
            settings.extra_backup_when_apply
        ),
        format!(
            "{}: {}",
            t!("sync_settings.backend"),
            backend_label(&settings.cloud_settings.backend)
        ),
        format!(
            "{}: {}",
            t!("settings.vn_scan_dirs"),
            vn_scan_dirs_value(app)
        ),
        String::new(),
        format!(
            "{}: {}",
            t!("settings.device_name"),
            current_device
                .map(|device| device.name.as_str())
                .unwrap_or("-")
        ),
        game_roots_line(app),
    ];
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .wrap(Wrap { trim: false })
            .block(panel_block(t!("tui.panel.core_settings").as_ref(), false)),
        area,
    );
}

fn setting_label(item: SettingsItem) -> String {
    match item {
        SettingsItem::AutoCloudEnqueue => t!("tui.settings.auto_enqueue_cloud").to_string(),
        SettingsItem::LudusaviLocalOnly => t!("tui.settings.ludusavi_local_only").to_string(),
        SettingsItem::CurrentDeviceName => t!("tui.settings.device_name_action").to_string(),
        SettingsItem::AddGameRoot => t!("tui.settings.add_game_root_action").to_string(),
        SettingsItem::AddVnScanRoot => t!("tui.settings.add_vn_scan_root_action").to_string(),
    }
}

fn setting_value(item: SettingsItem, app: &App) -> String {
    match item {
        SettingsItem::AutoCloudEnqueue => on_off(app.settings.auto_enqueue_cloud_on_change),
        SettingsItem::LudusaviLocalOnly => {
            if app.settings.ludusavi_local_only {
                t!("tui.ludusavi.scope_local").to_string()
            } else {
                t!("tui.ludusavi.scope_all").to_string()
            }
        }
        SettingsItem::CurrentDeviceName => app
            .data
            .config
            .devices
            .get(get_current_device_id())
            .map(|device| device.name.clone())
            .unwrap_or_else(|| "-".to_string()),
        SettingsItem::AddGameRoot | SettingsItem::AddVnScanRoot => {
            t!("tui.settings.enter_to_edit").to_string()
        }
    }
}

fn setting_detail(item: SettingsItem) -> String {
    match item {
        SettingsItem::AutoCloudEnqueue => t!("tui.settings.auto_enqueue_cloud_detail").to_string(),
        SettingsItem::LudusaviLocalOnly => {
            t!("tui.settings.ludusavi_local_only_detail").to_string()
        }
        SettingsItem::CurrentDeviceName => t!("tui.settings.device_name_detail").to_string(),
        SettingsItem::AddGameRoot => t!("tui.settings.add_game_root_detail").to_string(),
        SettingsItem::AddVnScanRoot => t!("tui.settings.add_vn_scan_root_detail").to_string(),
    }
}

fn setting_action_hint(item: SettingsItem) -> String {
    match item {
        SettingsItem::AutoCloudEnqueue | SettingsItem::LudusaviLocalOnly => {
            t!("tui.settings.toggle_hint").to_string()
        }
        SettingsItem::CurrentDeviceName => t!("tui.settings.rename_hint").to_string(),
        SettingsItem::AddGameRoot => t!("tui.settings.add_root_hint").to_string(),
        SettingsItem::AddVnScanRoot => t!("tui.settings.add_vn_root_hint").to_string(),
    }
}

fn game_roots_line(app: &App) -> String {
    let roots = app
        .data
        .config
        .devices
        .get(get_current_device_id())
        .map(|device| {
            if device.game_roots.is_empty() {
                "-".to_string()
            } else {
                device.game_roots.join(", ")
            }
        })
        .unwrap_or_else(|| "-".to_string());
    format!("{}: {}", t!("settings.game_roots_title"), roots)
}

fn vn_scan_dirs_line(app: &App) -> String {
    format!(
        "{}: {}",
        t!("settings.vn_scan_dirs"),
        vn_scan_dirs_value(app)
    )
}

fn vn_scan_dirs_value(app: &App) -> String {
    if app.data.config.settings.vn_scan_dirs.is_empty() {
        "-".to_string()
    } else {
        app.data.config.settings.vn_scan_dirs.join(", ")
    }
}

fn on_off(value: bool) -> String {
    if value {
        t!("tui.settings.enabled").to_string()
    } else {
        t!("tui.settings.disabled").to_string()
    }
}
