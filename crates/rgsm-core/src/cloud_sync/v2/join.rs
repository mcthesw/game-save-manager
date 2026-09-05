use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use tokio::sync::Mutex;

use super::{
    CloudLibraryTransport, CloudNamespaceClassification, CloudNamespaceClassifier,
    CloudNamespaceDescriptor, CloudNamespaceError, GameJoinClassification,
    OpenDalNamespaceTransport, SHARED_LIBRARY_PATH, compare_join_libraries, device_profile_path,
};
use crate::config::{DeviceProfile, OwnershipError, SharedGame, SharedLibrary};

static JOIN_WRITER_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct CloudLibraryJoinReview {
    pub cloud_game_count: usize,
    pub items: Vec<CloudLibraryJoinItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct CloudLibraryJoinItem {
    pub local_game_id: String,
    pub local_name: String,
    pub local_fingerprint: String,
    pub cloud_names: Vec<String>,
    pub cloud_fingerprint: Option<String>,
    pub classification: GameJoinClassification,
    pub difference: GameDefinitionDifference,
}

#[derive(Debug)]
pub struct CloudLibraryJoinResult {
    pub library_id: String,
    pub shared_library: SharedLibrary,
    pub device_profile: DeviceProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct GameDefinitionDifference {
    pub name_changed: bool,
    pub local_save_unit_count: usize,
    pub cloud_save_unit_count: usize,
    pub save_units_changed: bool,
    pub local_recognition: bool,
    pub cloud_recognition: bool,
    pub recognition_changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JoinGameAction {
    KeepCloud,
    AddLocal,
    ReplaceCloud,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct JoinGameDecision {
    pub local_game_id: String,
    pub local_fingerprint: String,
    pub cloud_fingerprint: Option<String>,
    pub action: JoinGameAction,
}

pub struct CloudLibraryJoin<T: CloudLibraryTransport> {
    transport: Arc<T>,
    max_attempts: usize,
}

impl CloudLibraryJoin<OpenDalNamespaceTransport> {
    pub fn new(operator: opendal::Operator, max_attempts: usize) -> Self {
        Self::with_transport(OpenDalNamespaceTransport::new(operator), max_attempts)
    }
}

impl<T: CloudLibraryTransport> CloudLibraryJoin<T> {
    pub fn with_transport(transport: T, max_attempts: usize) -> Self {
        Self {
            transport: Arc::new(transport),
            max_attempts: max_attempts.max(1),
        }
    }

    pub async fn review(
        &self,
        local: &SharedLibrary,
    ) -> Result<CloudLibraryJoinReview, CloudLibraryJoinError> {
        let (_, cloud) = self.load_supported().await?;
        build_review(local, &cloud)
    }

    pub async fn join(
        &self,
        local: &SharedLibrary,
        local_profile: &DeviceProfile,
        decisions: &[JoinGameDecision],
        confirmed_replacements: bool,
    ) -> Result<CloudLibraryJoinResult, CloudLibraryJoinError> {
        let _guard = JOIN_WRITER_LOCK.lock().await;
        validate_decisions(local, decisions, confirmed_replacements)?;

        for _ in 0..self.max_attempts {
            let (descriptor, latest) = self.load_supported().await?;
            let accepted = apply_decisions(local, latest, decisions)?;
            let expected = serde_json::to_vec_pretty(&accepted)?;
            if decisions
                .iter()
                .any(|decision| decision.action != JoinGameAction::KeepCloud)
            {
                self.transport.write(SHARED_LIBRARY_PATH, &expected).await?;
                if self.transport.read(SHARED_LIBRARY_PATH).await?.as_deref()
                    != Some(expected.as_slice())
                {
                    continue;
                }
            }

            let profile = local_profile.for_shared_library(&accepted);
            let profile_path = device_profile_path(&profile.device.id);
            let profile_bytes = serde_json::to_vec_pretty(&profile)?;
            self.write_verified(&profile_path, &profile_bytes).await?;
            return Ok(CloudLibraryJoinResult {
                library_id: descriptor.library_id,
                shared_library: accepted,
                device_profile: profile,
            });
        }
        Err(CloudLibraryJoinError::WriteVerificationFailed {
            path: SHARED_LIBRARY_PATH.to_string(),
            attempts: self.max_attempts,
        })
    }

    async fn load_supported(
        &self,
    ) -> Result<(CloudNamespaceDescriptor, SharedLibrary), CloudLibraryJoinError> {
        match CloudNamespaceClassifier::with_transport(self.transport.clone())
            .classify()
            .await?
        {
            CloudNamespaceClassification::SupportedV2 {
                descriptor,
                shared_library,
                ..
            } => Ok((descriptor, shared_library)),
            CloudNamespaceClassification::V1Only { .. } => {
                Err(CloudLibraryJoinError::JoinUnavailable("legacy_v1"))
            }
            CloudNamespaceClassification::Empty => {
                Err(CloudLibraryJoinError::JoinUnavailable("empty"))
            }
        }
    }

    async fn write_verified(&self, path: &str, bytes: &[u8]) -> Result<(), CloudLibraryJoinError> {
        for _ in 0..self.max_attempts {
            self.transport.write(path, bytes).await?;
            if self.transport.read(path).await?.as_deref() == Some(bytes) {
                return Ok(());
            }
        }
        Err(CloudLibraryJoinError::WriteVerificationFailed {
            path: path.to_string(),
            attempts: self.max_attempts,
        })
    }
}

fn build_review(
    local: &SharedLibrary,
    cloud: &SharedLibrary,
) -> Result<CloudLibraryJoinReview, CloudLibraryJoinError> {
    let items = compare_join_libraries(local, cloud)?
        .into_iter()
        .map(|candidate| {
            let cloud = candidate.cloud_candidates.first();
            Ok(CloudLibraryJoinItem {
                local_game_id: candidate.local.storage_key.clone(),
                local_name: candidate.local.name.clone(),
                local_fingerprint: candidate.local.portable_fingerprint()?,
                cloud_names: candidate
                    .cloud_candidates
                    .iter()
                    .map(|game| game.name.clone())
                    .collect(),
                cloud_fingerprint: cloud
                    .filter(|game| game.storage_key == candidate.local.storage_key)
                    .map(|game| game.portable_fingerprint())
                    .transpose()?,
                classification: candidate.classification,
                difference: difference(&candidate.local, cloud),
            })
        })
        .collect::<Result<Vec<_>, CloudLibraryJoinError>>()?;
    Ok(CloudLibraryJoinReview {
        cloud_game_count: cloud.games.len(),
        items,
    })
}

fn difference(local: &SharedGame, cloud: Option<&SharedGame>) -> GameDefinitionDifference {
    GameDefinitionDifference {
        name_changed: cloud.is_some_and(|game| game.name != local.name),
        local_save_unit_count: local.save_units.len(),
        cloud_save_unit_count: cloud.map_or(0, |game| game.save_units.len()),
        save_units_changed: cloud.is_none_or(|game| {
            game.normalized_portable().save_units != local.normalized_portable().save_units
        }),
        local_recognition: local.ludusavi_meta.is_some(),
        cloud_recognition: cloud.is_some_and(|game| game.ludusavi_meta.is_some()),
        recognition_changed: cloud.is_none_or(|game| game.ludusavi_meta != local.ludusavi_meta),
    }
}

fn validate_decisions(
    local: &SharedLibrary,
    decisions: &[JoinGameDecision],
    confirmed_replacements: bool,
) -> Result<(), CloudLibraryJoinError> {
    local.validate()?;
    let local_ids = local
        .games
        .iter()
        .map(|game| game.storage_key.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    for decision in decisions {
        if !local_ids.contains(decision.local_game_id.as_str())
            || !seen.insert(decision.local_game_id.as_str())
        {
            return Err(CloudLibraryJoinError::InvalidDecision(
                decision.local_game_id.clone(),
            ));
        }
        if decision.action == JoinGameAction::ReplaceCloud && !confirmed_replacements {
            return Err(CloudLibraryJoinError::ReplacementConfirmationRequired);
        }
    }
    Ok(())
}

fn apply_decisions(
    local: &SharedLibrary,
    mut latest: SharedLibrary,
    decisions: &[JoinGameDecision],
) -> Result<SharedLibrary, CloudLibraryJoinError> {
    let local_by_id = local
        .games
        .iter()
        .map(|game| (game.storage_key.as_str(), game))
        .collect::<HashMap<_, _>>();
    // A missing decision is not permission to replace a local definition.
    // Only the same stable identity can require a choice; titles are not IDs.
    for cloud_game in &latest.games {
        if let Some(local_game) = local_by_id.get(cloud_game.storage_key.as_str())
            && local_game.normalized_portable() != cloud_game.normalized_portable()
            && !decisions
                .iter()
                .any(|decision| decision.local_game_id == local_game.storage_key)
        {
            return Err(CloudLibraryJoinError::DecisionRequired(
                local_game.name.clone(),
            ));
        }
    }
    for decision in decisions {
        let local_game = local_by_id[decision.local_game_id.as_str()];
        if local_game.portable_fingerprint()? != decision.local_fingerprint {
            return Err(CloudLibraryJoinError::LocalGameChanged(
                local_game.name.clone(),
            ));
        }
        let cloud_index = latest
            .games
            .iter()
            .position(|game| game.storage_key == decision.local_game_id);
        match decision.action {
            JoinGameAction::KeepCloud => {
                let actual = cloud_index
                    .map(|index| latest.games[index].portable_fingerprint())
                    .transpose()?;
                if actual != decision.cloud_fingerprint {
                    return Err(CloudLibraryJoinError::TargetChanged(
                        local_game.name.clone(),
                    ));
                }
            }
            JoinGameAction::AddLocal => match cloud_index {
                None => latest.games.push(local_game.clone()),
                Some(_) => {
                    return Err(CloudLibraryJoinError::InvalidDecision(
                        decision.local_game_id.clone(),
                    ));
                }
            },
            JoinGameAction::ReplaceCloud => {
                let Some(index) = cloud_index else {
                    return Err(CloudLibraryJoinError::TargetChanged(
                        local_game.name.clone(),
                    ));
                };
                let expected = decision.cloud_fingerprint.as_deref();
                let actual = latest.games[index].portable_fingerprint()?;
                if expected.is_none() || Some(actual.as_str()) != expected {
                    return Err(CloudLibraryJoinError::TargetChanged(
                        local_game.name.clone(),
                    ));
                }
                let retention = latest.games[index].snapshot_retention;
                latest.games[index] = local_game.clone();
                latest.games[index].snapshot_retention = retention;
            }
        }
    }
    latest.games.sort_by(|left, right| {
        left.storage_key
            .cmp(&right.storage_key)
            .then_with(|| left.name.cmp(&right.name))
    });
    latest.validate()?;
    Ok(latest)
}

#[derive(Debug, Error)]
pub enum CloudLibraryJoinError {
    #[error(transparent)]
    Namespace(#[from] CloudNamespaceError),
    #[error(transparent)]
    Comparison(#[from] super::GameJoinComparisonError),
    #[error(transparent)]
    Ownership(#[from] OwnershipError),
    #[error("Cloud Library serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Cloud Library transport error: {0}")]
    Transport(#[from] opendal::Error),
    #[error("Cloud Library join is unavailable for root state: {0}")]
    JoinUnavailable(&'static str),
    #[error("Invalid join decision for Game: {0}")]
    InvalidDecision(String),
    #[error("Choose the local or cloud definition for Game: {0}")]
    DecisionRequired(String),
    #[error("Local Game changed during join review: {0}")]
    LocalGameChanged(String),
    #[error("Cloud Game changed during join review: {0}")]
    TargetChanged(String),
    #[error("Replacing a shared Game requires explicit confirmation")]
    ReplacementConfirmationRequired,
    #[error("Cloud object {path} did not match after {attempts} write attempts")]
    WriteVerificationFailed { path: String, attempts: usize },
}

#[cfg(test)]
#[path = "join_tests.rs"]
mod tests;
