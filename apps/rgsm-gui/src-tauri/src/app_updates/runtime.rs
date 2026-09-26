use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use super::{UpdateAction, UpdateCheck};
use crate::app_operations::AppOperations;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStage {
    #[default]
    Idle,
    Downloading,
    Ready,
    Waiting,
    Installing,
    Failed,
}

#[derive(Debug, Default, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub stage: UpdateStage,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
    pub update: Option<UpdateCheck>,
}

#[derive(Clone)]
struct PreparedUpdate {
    update: Update,
    bytes: Arc<Vec<u8>>,
}

#[derive(Default)]
struct Session {
    progress: UpdateProgress,
    prepared: Option<PreparedUpdate>,
    cancel: CancellationToken,
}

impl Session {
    fn busy(&self) -> bool {
        matches!(
            self.progress.stage,
            UpdateStage::Downloading | UpdateStage::Waiting | UpdateStage::Installing
        )
    }

    fn start(&mut self, update: UpdateCheck) -> Result<(), String> {
        if self.busy() {
            return Err("An update is already in progress".into());
        }
        self.prepared = None;
        self.progress = UpdateProgress {
            stage: UpdateStage::Downloading,
            update: Some(update),
            ..Default::default()
        };
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct InstallState(Arc<Mutex<Session>>);

pub fn snapshot(app: &AppHandle) -> UpdateProgress {
    app.state::<InstallState>()
        .0
        .lock()
        .unwrap()
        .progress
        .clone()
}

fn publish(app: &AppHandle) {
    crate::http::emit(app, "app-update-progress", &snapshot(app));
}

fn fail(app: &AppHandle, error: String) {
    log::warn!(target: "rgsm::updates", "{error}");
    let state = app.state::<InstallState>();
    let mut session = state.0.lock().unwrap();
    session.progress.stage = UpdateStage::Failed;
    session.progress.error = Some(error);
    drop(session);
    publish(app);
}

pub async fn download(app: AppHandle, expected_version: String) -> Result<(), String> {
    let check = super::check(&app).await?;
    if !check.available
        || check.latest_version != expected_version
        || !matches!(check.action, UpdateAction::Install)
    {
        return Err("Check for an available installable update again".into());
    }
    app.state::<InstallState>().0.lock().unwrap().start(check)?;
    publish(&app);
    tauri::async_runtime::spawn(async move {
        let result = prepare(&app, &expected_version).await;
        match result {
            Ok(prepared) => {
                let state = app.state::<InstallState>();
                let mut session = state.0.lock().unwrap();
                session.prepared = Some(prepared);
                session.progress.stage = UpdateStage::Ready;
                drop(session);
                publish(&app);
            }
            Err(error) => fail(&app, error),
        }
    });
    Ok(())
}

async fn prepare(app: &AppHandle, version: &str) -> Result<PreparedUpdate, String> {
    let mut update = app
        .updater_builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("No newer stable version is available")?;
    if update.version != version {
        return Err("The available version changed; check for updates again".into());
    }
    update.timeout = Some(std::time::Duration::from_secs(600));
    let bytes = update
        .download(
            |chunk, total| {
                let state = app.state::<InstallState>();
                let mut session = state.0.lock().unwrap();
                session.progress.downloaded_bytes += chunk as u64;
                session.progress.total_bytes = total;
                drop(session);
                publish(app);
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(PreparedUpdate {
        update,
        bytes: Arc::new(bytes),
    })
}

pub async fn install(app: AppHandle, expected_version: String) -> Result<(), String> {
    let (prepared, cancellation) = {
        let state = app.state::<InstallState>();
        let mut session = state.0.lock().unwrap();
        if session.busy() {
            return Err("An update is already in progress".into());
        }
        let prepared = session
            .prepared
            .clone()
            .ok_or("Download the update first")?;
        if prepared.update.version != expected_version {
            return Err("The downloaded version changed; check again".into());
        }
        session.cancel = CancellationToken::new();
        session.progress.stage = UpdateStage::Waiting;
        session.progress.error = None;
        (prepared, session.cancel.clone())
    };
    publish(&app);
    tauri::async_runtime::spawn(async move {
        let operations = app.state::<AppOperations>().inner().clone();
        let window = tokio::select! {
            biased;
            _ = cancellation.cancelled() => None,
            window = operations.pause_and_wait() => Some(window),
        };
        let Some(window) = window else {
            app.state::<InstallState>().0.lock().unwrap().progress.stage = UpdateStage::Ready;
            publish(&app);
            return;
        };
        {
            let state = app.state::<InstallState>();
            let mut session = state.0.lock().unwrap();
            if cancellation.is_cancelled() {
                session.progress.stage = UpdateStage::Ready;
                drop(session);
                drop(window);
                publish(&app);
                return;
            }
            session.progress.stage = UpdateStage::Installing;
        }
        publish(&app);
        let result = tauri::async_runtime::spawn_blocking(move || {
            prepared.update.install(prepared.bytes.as_slice())
        })
        .await;
        drop(window);
        match result {
            Ok(Ok(())) => app.restart(),
            Ok(Err(error)) => fail(&app, error.to_string()),
            Err(error) => fail(&app, error.to_string()),
        }
    });
    Ok(())
}

pub fn cancel_install(app: &AppHandle) {
    let state = app.state::<InstallState>();
    let session = state.0.lock().unwrap();
    if session.progress.stage == UpdateStage::Waiting {
        session.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_admission_rejects_double_clicks_and_pending_installation() {
        let update = UpdateCheck {
            available: true,
            current_version: "1.9.1".into(),
            latest_version: "1.9.2".into(),
            action: UpdateAction::Install,
            download_url: None,
            release_url: String::new(),
        };
        let mut session = Session::default();
        session.start(update.clone()).unwrap();
        assert!(session.start(update.clone()).is_err());
        session.progress.stage = UpdateStage::Waiting;
        assert!(session.start(update.clone()).is_err());
        session.progress.stage = UpdateStage::Failed;
        assert!(session.start(update).is_ok());
    }
}
