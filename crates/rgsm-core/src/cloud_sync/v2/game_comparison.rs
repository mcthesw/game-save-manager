use std::collections::BTreeMap;
use std::hash::Hasher;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use xxhash_rust::xxh3::Xxh3;

use crate::config::{OwnershipError, SharedGame, SharedLibrary, SharedSaveUnitSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GameJoinClassification {
    Same,
    LocalOnly,
    PossibleDuplicate,
    GameDefinitionConflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct GameJoinCandidate {
    pub local: SharedGame,
    pub cloud_candidates: Vec<SharedGame>,
    pub classification: GameJoinClassification,
}

#[derive(Debug, Error)]
pub enum GameJoinComparisonError {
    #[error(transparent)]
    Ownership(#[from] OwnershipError),
    #[error("Portable Game serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl SharedGame {
    /// Clone into the canonical comparison shape without rewriting semantic
    /// identity, display, path-pattern, or Save Unit values.
    pub fn normalized_portable(&self) -> Self {
        let mut normalized = self.clone();
        // Retention is shared Cloud Library policy, not part of a portable
        // Game definition. A joining Device inherits it from the cloud side.
        normalized.snapshot_retention = None;
        for unit in &mut normalized.save_units {
            if let SharedSaveUnitSource::ManifestPattern { constraints, .. } = &mut unit.source {
                constraints
                    .alternatives
                    .sort_by_key(|condition| (condition.os, condition.store));
                constraints.alternatives.dedup();
            }
        }
        normalized
    }

    pub fn portable_fingerprint(&self) -> Result<String, serde_json::Error> {
        let bytes = serde_json::to_vec(&self.normalized_portable())?;
        let mut hasher = Xxh3::new();
        hasher.write(&bytes);
        Ok(format!("{:016x}", hasher.finish()))
    }
}

/// Compare local portable Games against the accepted cloud baseline.
///
/// For L local and C cloud Games, indexing and lookup are
/// O((L + C) log C + L log L), plus O(D) to normalize and compare D bytes of
/// portable definitions. Output ordering is deterministic by stable Game ID.
pub fn compare_join_libraries(
    local: &SharedLibrary,
    cloud: &SharedLibrary,
) -> Result<Vec<GameJoinCandidate>, GameJoinComparisonError> {
    local.validate()?;
    cloud.validate()?;

    let cloud_by_id = cloud
        .games
        .iter()
        .map(|game| (game.storage_key.as_str(), game))
        .collect::<BTreeMap<_, _>>();
    let mut cloud_by_name = BTreeMap::<&str, Vec<&SharedGame>>::new();
    for game in &cloud.games {
        cloud_by_name.entry(&game.name).or_default().push(game);
    }

    let mut local_games = local.games.iter().collect::<Vec<_>>();
    local_games.sort_by(|left, right| left.storage_key.cmp(&right.storage_key));
    let mut candidates = Vec::with_capacity(local_games.len());
    for local_game in local_games {
        let (classification, cloud_candidates) =
            if let Some(cloud_game) = cloud_by_id.get(local_game.storage_key.as_str()) {
                let same = local_game.normalized_portable() == cloud_game.normalized_portable();
                (
                    if same {
                        GameJoinClassification::Same
                    } else {
                        GameJoinClassification::GameDefinitionConflict
                    },
                    vec![(*cloud_game).clone()],
                )
            } else if let Some(same_name) = cloud_by_name.get(local_game.name.as_str()) {
                let mut matches = same_name
                    .iter()
                    .map(|game| (*game).clone())
                    .collect::<Vec<_>>();
                matches.sort_by(|left, right| left.storage_key.cmp(&right.storage_key));
                (GameJoinClassification::PossibleDuplicate, matches)
            } else {
                (GameJoinClassification::LocalOnly, Vec::new())
            };
        candidates.push(GameJoinCandidate {
            local: local_game.clone(),
            cloud_candidates,
            classification,
        });
    }
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use crate::backup::{LudusaviMeta, StoreGameId};
    use crate::config::{SharedSaveUnit, SharedSnapshotRetentionPolicy, V2_CONFIG_SCHEMA_VERSION};
    use crate::path_pattern::{ManifestPathCondition, ManifestPathConstraints, StoreKind};

    use super::*;

    fn game(id: &str, name: &str) -> SharedGame {
        SharedGame {
            name: name.into(),
            storage_key: id.into(),
            save_units: vec![
                SharedSaveUnit {
                    id: 2,
                    source: SharedSaveUnitSource::ManifestPattern {
                        expected_type: None,
                        pattern: crate::path_pattern::ManifestPathPattern::new("<home>/save"),
                        constraints: ManifestPathConstraints {
                            alternatives: vec![
                                ManifestPathCondition {
                                    os: None,
                                    store: Some(StoreKind::Gog),
                                },
                                ManifestPathCondition {
                                    os: None,
                                    store: Some(StoreKind::Steam),
                                },
                            ],
                        },
                    },
                },
                SharedSaveUnit {
                    id: 1,
                    source: SharedSaveUnitSource::Concrete {
                        unit_type: crate::backup::SaveUnitType::Folder,
                    },
                },
            ],
            next_save_unit_id: 3,
            ludusavi_meta: Some(LudusaviMeta {
                install_dirs: vec!["B".into(), "A".into()],
                store_game_ids: vec![
                    StoreGameId {
                        store: StoreKind::Gog,
                        id: "2".into(),
                    },
                    StoreGameId {
                        store: StoreKind::Steam,
                        id: "1".into(),
                    },
                ],
            }),
            snapshot_retention: None,
        }
    }

    fn library(games: Vec<SharedGame>) -> SharedLibrary {
        SharedLibrary {
            schema_version: V2_CONFIG_SCHEMA_VERSION,
            games,
        }
    }

    #[test]
    fn normalization_ignores_unordered_values_and_shared_retention_policy() {
        let original = game("game", "Game");
        let mut reordered = original.clone();
        for unit in &mut reordered.save_units {
            if let SharedSaveUnitSource::ManifestPattern { constraints, .. } = &mut unit.source {
                constraints.alternatives.reverse();
            }
        }

        assert_eq!(
            original.portable_fingerprint().unwrap(),
            reordered.portable_fingerprint().unwrap()
        );

        reordered.snapshot_retention = Some(SharedSnapshotRetentionPolicy {
            automatic_snapshots_per_branch: 5,
        });
        assert_eq!(
            original.portable_fingerprint().unwrap(),
            reordered.portable_fingerprint().unwrap()
        );

        reordered.save_units.reverse();
        assert_ne!(
            original.portable_fingerprint().unwrap(),
            reordered.portable_fingerprint().unwrap()
        );
    }

    #[test]
    fn join_classification_follows_identity_then_definition() {
        let same = game("same", "Same");
        let mut conflict = game("conflict", "Conflict");
        let cloud_conflict = conflict.clone();
        conflict.next_save_unit_id += 1;
        let possible = game("local-duplicate", "Duplicate");
        let local_only = game("local-only", "Local");
        let local = library(vec![local_only, possible, conflict, same.clone()]);
        let cloud = library(vec![
            game("remote-duplicate-b", "Duplicate"),
            same,
            cloud_conflict,
            game("remote-duplicate-a", "Duplicate"),
            game("remote-only", "Remote"),
        ]);

        let compared = compare_join_libraries(&local, &cloud).unwrap();

        assert_eq!(
            compared
                .iter()
                .map(|candidate| (
                    candidate.local.storage_key.as_str(),
                    candidate.classification
                ))
                .collect::<Vec<_>>(),
            vec![
                ("conflict", GameJoinClassification::GameDefinitionConflict),
                ("local-duplicate", GameJoinClassification::PossibleDuplicate),
                ("local-only", GameJoinClassification::LocalOnly),
                ("same", GameJoinClassification::Same),
            ]
        );
        assert_eq!(
            compared[1]
                .cloud_candidates
                .iter()
                .map(|game| game.storage_key.as_str())
                .collect::<Vec<_>>(),
            vec!["remote-duplicate-a", "remote-duplicate-b"]
        );
    }

    #[test]
    fn standalone_library_validation_rejects_invalid_portable_identity() {
        let mut empty_id = library(vec![game("", "Game")]);
        assert!(matches!(
            empty_id.validate(),
            Err(OwnershipError::EmptySharedGameId)
        ));

        empty_id.games[0].storage_key = "game".into();
        empty_id.games.push(empty_id.games[0].clone());
        assert!(matches!(
            empty_id.validate(),
            Err(OwnershipError::DuplicateSharedGame(game_id)) if game_id == "game"
        ));

        empty_id.games.pop();
        let duplicate = empty_id.games[0].save_units[0].clone();
        empty_id.games[0].save_units.push(duplicate);
        assert!(matches!(
            empty_id.validate(),
            Err(OwnershipError::DuplicateSharedSaveUnit {
                game_id,
                save_unit_id: 2
            }) if game_id == "game"
        ));
    }
}
