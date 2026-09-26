use std::{collections::HashMap, time::Duration};

mod runtime;
pub use runtime::{InstallState, UpdateProgress, cancel_install, download, install, snapshot};

use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle,
    utils::{config::BundleType, platform::bundle_type},
};
use utoipa::ToSchema;

const MANIFEST_URL: &str =
    "https://github.com/mcthesw/game-save-manager/releases/latest/download/latest.json";

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum UpdateAction {
    Install,
    Download,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub action: UpdateAction,
    pub download_url: Option<String>,
    pub release_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StableManifest {
    version: String,
    downloads: HashMap<String, String>,
    release_page: String,
}

fn package_target(
    identifier: &str,
    bundle: Option<BundleType>,
    arch: &str,
) -> (&'static str, UpdateAction) {
    if identifier == "com.game-save-manager-slim" {
        return ("windows-x86_64-portable-slim", UpdateAction::Download);
    }
    match (bundle, arch) {
        (Some(BundleType::Nsis), "x86_64") => ("windows-x86_64-nsis", UpdateAction::Install),
        (Some(BundleType::Msi), "x86_64") => ("windows-x86_64-msi", UpdateAction::Install),
        (Some(BundleType::AppImage), "x86_64") => ("linux-x86_64-appimage", UpdateAction::Download),
        (Some(BundleType::Deb), "x86_64") => ("linux-x86_64-deb", UpdateAction::Download),
        (Some(BundleType::Rpm), "x86_64") => ("linux-x86_64-rpm", UpdateAction::Download),
        (Some(BundleType::App), "aarch64") | (Some(BundleType::Dmg), "aarch64") => {
            ("darwin-aarch64-dmg", UpdateAction::Download)
        }
        _ => ("", UpdateAction::Download),
    }
}

pub async fn check(app: &AppHandle) -> Result<UpdateCheck, String> {
    let manifest: StableManifest = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?
        .get(MANIFEST_URL)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())?;
    let latest = Version::parse(&manifest.version).map_err(|error| error.to_string())?;
    if !latest.pre.is_empty() || !latest.build.is_empty() {
        return Err("The stable update feed contains a prerelease version".into());
    }
    let current = app.package_info().version.clone();
    let (target, mut action) = package_target(
        &app.config().identifier,
        bundle_type(),
        std::env::consts::ARCH,
    );
    let available = latest > current;
    let release_url = manifest.release_page;
    if !manifest.downloads.contains_key(target) {
        action = UpdateAction::Download;
    }
    let download_url = available.then(|| {
        manifest
            .downloads
            .get(target)
            .cloned()
            .unwrap_or_else(|| release_url.clone())
    });
    Ok(UpdateCheck {
        available,
        current_version: current.to_string(),
        latest_version: latest.to_string(),
        action,
        download_url,
        release_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_selection_never_crosses_installer_formats() {
        assert_eq!(
            package_target("rgsm", Some(BundleType::Nsis), "x86_64").0,
            "windows-x86_64-nsis"
        );
        assert_eq!(
            package_target("rgsm", Some(BundleType::Msi), "x86_64").0,
            "windows-x86_64-msi"
        );
        assert_eq!(
            package_target("rgsm", Some(BundleType::AppImage), "x86_64").0,
            "linux-x86_64-appimage"
        );
        assert!(matches!(
            package_target("rgsm", Some(BundleType::Deb), "x86_64").1,
            UpdateAction::Download
        ));
        assert!(matches!(
            package_target("rgsm", Some(BundleType::Rpm), "x86_64").1,
            UpdateAction::Download
        ));
        assert!(matches!(
            package_target(
                "com.game-save-manager-slim",
                Some(BundleType::Nsis),
                "x86_64"
            )
            .1,
            UpdateAction::Download
        ));
    }
}
