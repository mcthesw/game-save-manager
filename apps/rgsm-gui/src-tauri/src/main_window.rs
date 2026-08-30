use log::error;
use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_window_state::{StateFlags, WindowExt};

use crate::http::HttpHostState;

const MAIN_WINDOW_LABEL: &str = "main";

#[cfg(all(debug_assertions, target_os = "windows"))]
const E2E_WEBVIEW_DEBUG_PORT: &str = "RGSM_E2E_WEBVIEW_DEBUG_PORT";

#[cfg(all(debug_assertions, target_os = "windows"))]
fn e2e_browser_args(port: Option<&str>) -> anyhow::Result<Option<String>> {
    let Some(port) = port else {
        return Ok(None);
    };
    let port = port.parse::<u16>()?;
    anyhow::ensure!(port != 0, "WebView debug port must not be zero");
    Ok(Some(format!(
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port={port}"
    )))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeConfig<'a> {
    api_base_url: &'a str,
    token: &'a str,
}

fn runtime_initialization_script(base_url: &str, token: &str) -> anyhow::Result<String> {
    let runtime = RuntimeConfig {
        api_base_url: base_url,
        token,
    };
    Ok(format!(
        "window.__RGSM_RUNTIME__ = {};",
        serde_json::to_string(&runtime)?
    ))
}

fn current_runtime_initialization_script(app: &AppHandle) -> anyhow::Result<String> {
    let host = app
        .try_state::<HttpHostState>()
        .ok_or_else(|| anyhow::anyhow!("HTTP Host state is not available"))?;
    runtime_initialization_script(host.base_url(), &host.api_token())
}

pub fn create_main_window(app: &AppHandle) -> anyhow::Result<WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN_WINDOW_LABEL)
        .ok_or_else(|| anyhow::anyhow!("Main window configuration is not available"))?;
    let builder = WebviewWindowBuilder::from_config(app, config)?;
    #[cfg(all(debug_assertions, target_os = "windows"))]
    let builder = match e2e_browser_args(std::env::var(E2E_WEBVIEW_DEBUG_PORT).ok().as_deref())? {
        Some(arguments) => builder.additional_browser_args(&arguments),
        None => builder,
    };
    let window = builder
        .initialization_script(current_runtime_initialization_script(app)?)
        .build()?;
    window.restore_state(StateFlags::all())?;
    Ok(window)
}

pub fn show_main_window(app: &AppHandle) -> anyhow::Result<()> {
    let window = match app.get_webview_window(MAIN_WINDOW_LABEL) {
        Some(window) => window,
        None => create_main_window(app)?,
    };
    window.show()?;
    window.set_focus()?;
    Ok(())
}

pub fn defer_show_main_window(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let scheduled_app = app.clone();
        if let Err(error) = app.run_on_main_thread(move || {
            if let Err(error) = show_main_window(&scheduled_app) {
                error!(target: "rgsm::main", "Cannot show main window: {error}");
            }
        }) {
            error!(target: "rgsm::main", "Cannot schedule main window: {error}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_script_round_trips_host_credentials() {
        let script = runtime_initialization_script(
            "http://127.0.0.1:1234",
            "token-with-\"quote\"-and-\n-newline",
        )
        .unwrap();
        let payload = script
            .strip_prefix("window.__RGSM_RUNTIME__ = ")
            .and_then(|value| value.strip_suffix(';'))
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(payload).unwrap();

        assert_eq!(value["apiBaseUrl"], "http://127.0.0.1:1234");
        assert_eq!(value["token"], "token-with-\"quote\"-and-\n-newline");
    }

    #[test]
    fn configured_main_window_is_owned_by_the_runtime_factory() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let window = &config["app"]["windows"][0];

        assert_eq!(window["label"], MAIN_WINDOW_LABEL);
        assert_eq!(window["create"], false);
    }

    #[cfg(all(debug_assertions, target_os = "windows"))]
    #[test]
    fn e2e_debug_port_is_validated_and_rendered_as_a_browser_argument() {
        assert_eq!(e2e_browser_args(None).unwrap(), None);
        assert!(e2e_browser_args(Some("0")).is_err());
        assert!(e2e_browser_args(Some("not-a-port")).is_err());

        let args = e2e_browser_args(Some("9222")).unwrap().unwrap();
        assert!(args.contains("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection"));
        assert!(args.contains("--remote-debugging-port=9222"));
    }
}
