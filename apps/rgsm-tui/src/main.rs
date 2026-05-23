use anyhow::{Context, Result};
use rust_i18n::i18n;

i18n!("../../locales", fallback = ["en_US", "zh_SIMPLIFIED"]);

mod app;
mod args;
mod completion;
mod hooks;
mod logging;
mod model;
mod operations;
mod terminal;
mod tui_settings;
mod ui;

use app::App;
use args::CliOptions;
use terminal::TerminalGuard;
use tui_settings::TuiSettings;

#[tokio::main]
async fn main() -> Result<()> {
    let options = CliOptions::parse().context("failed to parse rgsm-tui arguments")?;
    if options.help {
        print!("{}", args::help_text());
        return Ok(());
    }

    if let Some(data_dir) = options.data_dir {
        std::fs::create_dir_all(&data_dir).with_context(|| {
            format!(
                "failed to create RGSM data directory at {}",
                data_dir.display()
            )
        })?;
        rgsm_core::app_dirs::set_app_data_dir_override(data_dir)
            .context("failed to set RGSM data directory override")?;
    }

    rgsm_core::config::config_check().context("failed to initialize RGSM config")?;
    let config = rgsm_core::config::get_config().context("failed to load RGSM config")?;
    rust_i18n::set_locale(&config.settings.locale);

    let data_dir = rgsm_core::app_dirs::get_app_data_dir().clone();
    let settings = TuiSettings::load(&data_dir).context("failed to load TUI settings")?;
    let mut app = App::new(data_dir, settings)
        .await
        .context("failed to initialize TUI state")?;
    let mut terminal = TerminalGuard::enter().context("failed to enter terminal UI")?;

    while !app.should_quit() {
        app.drain_operation_events();
        app.draw(terminal.terminal_mut())?;
        if let Some(event) = app.poll_input()? {
            app.handle_event(event)?;
        }
    }

    Ok(())
}
