#[cfg(test)]
use std::path::PathBuf;

use crate::backup::{
    CapturePlan, Game, SaveUnit, SaveUnitCaptureInput, SaveUnitSource, SaveUnitType,
};
use crate::config::Config;
use crate::device::get_current_device_id;
use crate::path_pattern::{PathPatternError, parse_manifest_path_pattern};
use crate::path_resolution::{
    CandidateDimensions, CandidateExpression, ResolutionDiagnostic, ResolutionDiagnosticKind,
    ResolutionReport, ResolutionSelectionState, ResolvedLocationKind, ResolvedSaveLocation,
    StoreAccountCandidate, match_resolution_plan, plan_resolution,
};

use super::ServiceContext;
use crate::path_resolution::context::{detected_id, game_context};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolutionPurpose {
    Capture,
    Restore,
}

#[derive(
    Debug, Clone, Default, serde::Deserialize, serde::Serialize, specta::Type, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct PathPreviewContext {
    pub game: Option<crate::backup::GameDraft>,
    pub store_user_id: Option<String>,
    #[serde(default)]
    pub install_dirs: Vec<String>,
    pub steam_id: Option<u32>,
    #[serde(default)]
    pub literal: bool,
}

impl ServiceContext {
    pub(crate) fn validate_game_paths(&self, config: &Config, game: &Game) -> anyhow::Result<()> {
        let device_id = get_current_device_id();
        let context = game.path_context(config.devices.get(device_id));
        if let Some(path) = game
            .game_paths
            .get(device_id)
            .filter(|path| !path.trim().is_empty())
        {
            crate::path_resolver::resolve_path_explicit(path, Some(&context))?;
        }
        for unit in game.save_paths.iter().filter(|unit| unit.enabled) {
            match &unit.source {
                SaveUnitSource::Concrete { paths, .. } => {
                    if let Some(path) = paths.get(device_id) {
                        crate::path_resolver::resolve_path_explicit(path, Some(&context))?;
                    }
                }
                SaveUnitSource::ManifestPattern { constraints, .. } => {
                    let has_override = game.path_override(unit.id, device_id).is_some();
                    if !has_override
                        && !constraints.allows_platform(crate::path_pattern::PlatformKind::host())
                    {
                        continue;
                    }
                    let report = self.resolve_save_unit_for_restore(config, game, unit);
                    if matches!(
                        report.selection_state,
                        ResolutionSelectionState::Ambiguous { .. }
                            | ResolutionSelectionState::StaleSelection { .. }
                    ) || (has_override
                        && matches!(report.selection_state, ResolutionSelectionState::Missing))
                    {
                        anyhow::bail!(
                            "{}: {}",
                            report.raw_pattern,
                            crate::path_resolution::selection_error(&report.selection_state)
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub fn check_ad_hoc_paths(
        &self,
        config: &Config,
        paths: &[String],
        preview: &PathPreviewContext,
    ) -> Vec<crate::path_resolver::PathCheckResult> {
        paths
            .iter()
            .map(|path| {
                if crate::backup::registry::is_registry_path(path) {
                    return crate::path_resolver::check_path(path, None, config);
                }
                let game = preview
                    .game
                    .clone()
                    .map(|draft| draft.into_game(None))
                    .unwrap_or_else(|| Game {
                        ludusavi_meta: Some(crate::backup::LudusaviMeta {
                            install_dirs: preview.install_dirs.clone(),
                            store_game_ids: preview
                                .steam_id
                                .map(|id| crate::backup::StoreGameId {
                                    store: crate::path_pattern::StoreKind::Steam,
                                    id: id.to_string(),
                                })
                                .into_iter()
                                .collect(),
                        }),
                        name: "Path preview".into(),
                        storage_key: String::new(),
                        save_paths: vec![],
                        game_paths: Default::default(),
                        next_save_unit_id: 0,
                        cloud_sync_enabled: false,
                        auto_backup: None,
                        auto_backup_limit: None,
                        device_bindings: Default::default(),
                    });
                let mut context = game.path_context(config.devices.get(get_current_device_id()));
                if let Some(user_id) = &preview.store_user_id {
                    let resolution = context.resolution.as_mut().expect("game path context");
                    let id = detected_id("preview-account", user_id);
                    resolution.accounts = vec![StoreAccountCandidate {
                        id: id.clone(),
                        store: crate::path_pattern::StoreKind::Steam,
                        user_id: user_id.clone(),
                    }];
                    resolution.selection.account_ids = Some([id].into_iter().collect());
                }
                if preview.literal {
                    return crate::path_resolver::check_path(path, Some(&context), config);
                }
                let report = match parse_manifest_path_pattern(path) {
                    Ok(parsed) => {
                        let plan = plan_resolution(
                            &parsed,
                            Default::default(),
                            context.resolution.as_ref().expect("game path context"),
                        );
                        match match_resolution_plan(&plan) {
                            Ok(report) => report,
                            Err(error) => {
                                return crate::path_resolver::PathCheckResult::ResolveFailed {
                                    raw_path: path.clone(),
                                    error: error.to_string(),
                                };
                            }
                        }
                    }
                    Err(error) => invalid_pattern_report(path, error),
                };
                if matches!(
                    report.selection_state,
                    ResolutionSelectionState::Missing
                        | ResolutionSelectionState::Ambiguous { .. }
                        | ResolutionSelectionState::StaleSelection { .. }
                ) {
                    return crate::path_resolver::PathCheckResult::ResolveFailed {
                        raw_path: path.clone(),
                        error: crate::path_resolution::selection_error(&report.selection_state),
                    };
                }
                if let Some(location) = report.locations.first() {
                    return crate::path_resolver::PathCheckResult::Ok {
                        raw_path: path.clone(),
                        resolved_path: location.path.clone(),
                        is_file: matches!(location.kind, ResolvedLocationKind::File),
                    };
                }
                crate::path_resolver::PathCheckResult::NotFound {
                    raw_path: path.clone(),
                    resolved_path: report
                        .candidates
                        .first()
                        .map(|candidate| candidate.expression.clone())
                        .unwrap_or_else(|| path.clone()),
                }
            })
            .collect()
    }

    pub fn resolve_ad_hoc_pattern(
        &self,
        config: &Config,
        pattern: &str,
        store_user_id: Option<&str>,
        install_dirs: &[String],
        steam_id: Option<u32>,
    ) -> ResolutionReport {
        let game = Game {
            name: "Path preview".to_string(),
            storage_key: String::new(),
            save_paths: Vec::new(),
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: false,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: Some(crate::backup::LudusaviMeta {
                install_dirs: install_dirs.to_vec(),
                store_game_ids: steam_id
                    .map(|id| crate::backup::StoreGameId {
                        store: crate::path_pattern::StoreKind::Steam,
                        id: id.to_string(),
                    })
                    .into_iter()
                    .collect(),
            }),
            device_bindings: Default::default(),
        };
        let mut context = game_context(&game, config.devices.get(get_current_device_id()));
        if let Some(user_id) = store_user_id {
            let id = detected_id("preview-account", user_id);
            context.accounts = vec![StoreAccountCandidate {
                id: id.clone(),
                store: crate::path_pattern::StoreKind::Steam,
                user_id: user_id.to_string(),
            }];
            context.selection.account_ids = Some([id].into_iter().collect());
        }
        let parsed = match parse_manifest_path_pattern(pattern) {
            Ok(parsed) => parsed,
            Err(error) => return invalid_pattern_report(pattern, error),
        };
        let plan = plan_resolution(
            &parsed,
            crate::path_pattern::ManifestPathConstraints::default(),
            &context,
        );
        match match_resolution_plan(&plan) {
            Ok(report) => report,
            Err(error) => ResolutionReport {
                raw_pattern: pattern.to_string(),
                selection_state: plan.selection_state,
                candidates: plan.candidates,
                locations: Vec::new(),
                diagnostics: vec![ResolutionDiagnostic {
                    kind: ResolutionDiagnosticKind::InvalidGlob,
                    message: error.to_string(),
                }],
            },
        }
    }

    pub(crate) fn capture_plan(
        &self,
        config: &Config,
        game: &Game,
    ) -> Result<CapturePlan, crate::backup::CapturePlanError> {
        CapturePlan::from_resolution_reports(
            game.save_paths
                .iter()
                .filter(|save_unit| save_unit.enabled)
                .map(|save_unit| SaveUnitCaptureInput {
                    save_unit_id: save_unit.id,
                    delete_before_apply: save_unit.delete_before_apply,
                    report: self.resolve_save_unit(config, game, save_unit),
                })
                .collect(),
        )
    }

    /// Resolve a Save Unit against an explicitly supplied configuration snapshot.
    /// This is the composition boundary for host paths and per-device resources.
    pub fn resolve_save_unit(
        &self,
        config: &Config,
        game: &Game,
        save_unit: &SaveUnit,
    ) -> ResolutionReport {
        self.resolve_save_unit_for(config, game, save_unit, ResolutionPurpose::Capture)
    }

    pub(crate) fn resolve_save_unit_for_restore(
        &self,
        config: &Config,
        game: &Game,
        save_unit: &SaveUnit,
    ) -> ResolutionReport {
        self.resolve_save_unit_for(config, game, save_unit, ResolutionPurpose::Restore)
    }

    fn resolve_save_unit_for(
        &self,
        config: &Config,
        game: &Game,
        save_unit: &SaveUnit,
        purpose: ResolutionPurpose,
    ) -> ResolutionReport {
        let device_id = get_current_device_id();
        if matches!(save_unit.source, SaveUnitSource::ManifestPattern { .. })
            && let Some(path) = game.path_override(save_unit.id, device_id)
        {
            if path.path.trim().is_empty()
                || save_unit.unit_type() == Some(&SaveUnitType::WinRegistry)
            {
                return blocked_report(
                    &path.path,
                    &rust_i18n::t!("path_variable.invalid_override"),
                );
            }
            return resolve_concrete(
                Some(&path.path),
                save_unit.unit_type(),
                Some(&game.path_context(config.devices.get(device_id))),
                purpose,
            );
        }
        match &save_unit.source {
            SaveUnitSource::Concrete { unit_type, paths } => {
                let path_context = game.path_context(config.devices.get(device_id));
                resolve_concrete(
                    paths.get(device_id),
                    Some(unit_type),
                    Some(&path_context),
                    purpose,
                )
            }
            SaveUnitSource::ManifestPattern {
                pattern,
                constraints,
                ..
            } => {
                let context = game_context(game, config.devices.get(device_id));
                let parsed = match parse_manifest_path_pattern(pattern.raw()) {
                    Ok(parsed) => parsed,
                    Err(error) => return invalid_pattern_report(pattern.raw(), error),
                };
                let plan = plan_resolution(&parsed, constraints.clone(), &context);
                if purpose == ResolutionPurpose::Restore {
                    return ResolutionReport {
                        raw_pattern: pattern.raw().to_string(),
                        selection_state: plan.selection_state,
                        candidates: plan.candidates,
                        locations: Vec::new(),
                        diagnostics: plan.diagnostics,
                    };
                }
                match match_resolution_plan(&plan) {
                    Ok(report) => report,
                    Err(error) => ResolutionReport {
                        raw_pattern: pattern.raw().to_string(),
                        selection_state: plan.selection_state,
                        candidates: plan.candidates,
                        locations: Vec::new(),
                        diagnostics: vec![ResolutionDiagnostic {
                            kind: ResolutionDiagnosticKind::InvalidGlob,
                            message: error.to_string(),
                        }],
                    },
                }
            }
        }
    }
}

fn resolve_concrete(
    path: Option<&String>,
    unit_type: Option<&SaveUnitType>,
    path_context: Option<&crate::path_resolver::PathContext>,
    purpose: ResolutionPurpose,
) -> ResolutionReport {
    let Some(path) = path else {
        return blocked_report(
            "",
            "this save location is not configured for the current device",
        );
    };

    #[cfg(not(target_os = "windows"))]
    if matches!(unit_type, Some(SaveUnitType::WinRegistry)) {
        return empty_concrete_report(path);
    }

    let resolved = match crate::path_resolver::resolve_path_explicit(path, path_context) {
        Ok(resolved) => resolved,
        Err(error) => return blocked_report(path, &error.to_string()),
    };
    let source = resolved.as_path();
    if purpose == ResolutionPurpose::Capture {
        let exists = match unit_type {
            None => source.is_file() || source.is_dir(),
            Some(SaveUnitType::File) => source.is_file(),
            Some(SaveUnitType::Folder) => source.is_dir(),
            Some(SaveUnitType::WinRegistry) => {
                match crate::backup::registry::registry_key_exists(&resolved.to_string_lossy()) {
                    Ok(exists) => exists,
                    Err(error) => return blocked_report(path, &error.to_string()),
                }
            }
        };
        if !exists {
            return blocked_report(path, "the configured save location is unavailable");
        }
    }
    let kind = match unit_type {
        Some(SaveUnitType::File) => Some(ResolvedLocationKind::File),
        Some(SaveUnitType::Folder) => Some(ResolvedLocationKind::Directory),
        Some(SaveUnitType::WinRegistry) => Some(ResolvedLocationKind::Registry),
        None if source.is_file() => Some(ResolvedLocationKind::File),
        None if source.is_dir() => Some(ResolvedLocationKind::Directory),
        None => None,
    };
    ResolutionReport {
        raw_pattern: path.clone(),
        selection_state: ResolutionSelectionState::Explicit {
            candidate_ids: vec!["concrete".to_string()],
        },
        candidates: vec![CandidateExpression {
            id: "concrete".to_string(),
            expression: globset::escape(&resolved.to_string_lossy()),
            logical_anchor: source
                .parent()
                .unwrap_or(source)
                .to_string_lossy()
                .into_owned(),
            dimensions: CandidateDimensions::default(),
            case_sensitive: !cfg!(target_os = "windows"),
        }],
        locations: kind
            .map(|kind| ResolvedSaveLocation {
                path: resolved.to_string_lossy().into_owned(),
                kind,
                candidate_id: "concrete".to_string(),
                logical_anchor: source
                    .parent()
                    .unwrap_or(source)
                    .to_string_lossy()
                    .into_owned(),
                dimensions: CandidateDimensions::default(),
            })
            .into_iter()
            .collect(),
        diagnostics: Vec::new(),
    }
}

#[cfg(not(target_os = "windows"))]
fn empty_concrete_report(path: &str) -> ResolutionReport {
    ResolutionReport {
        raw_pattern: path.to_string(),
        selection_state: ResolutionSelectionState::Explicit {
            candidate_ids: vec!["concrete".to_string()],
        },
        candidates: Vec::new(),
        locations: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn blocked_report(raw: &str, message: &str) -> ResolutionReport {
    ResolutionReport {
        raw_pattern: raw.to_string(),
        selection_state: ResolutionSelectionState::Missing,
        candidates: Vec::new(),
        locations: Vec::new(),
        diagnostics: vec![ResolutionDiagnostic {
            kind: ResolutionDiagnosticKind::NoCandidate,
            message: message.to_string(),
        }],
    }
}

fn invalid_pattern_report(raw: &str, error: PathPatternError) -> ResolutionReport {
    let kind = match error {
        PathPatternError::InvalidGlob { .. } => ResolutionDiagnosticKind::InvalidGlob,
        _ => ResolutionDiagnosticKind::UnknownPlaceholder,
    };
    ResolutionReport {
        raw_pattern: raw.to_string(),
        selection_state: ResolutionSelectionState::Missing,
        candidates: Vec::new(),
        locations: Vec::new(),
        diagnostics: vec![ResolutionDiagnostic {
            kind,
            message: error.to_string(),
        }],
    }
}

#[cfg(test)]
#[path = "path_override_tests.rs"]
mod override_tests;

#[cfg(test)]
mod concrete_tests {
    use super::*;

    #[test]
    fn uninstalled_manifest_game_can_be_saved_before_its_location_is_configured() {
        let game: Game = serde_json::from_value(serde_json::json!({
            "name": "Not installed",
            "save_paths": [{"source": {"type": "manifestPattern", "pattern": "<base>/save.dat"}}]
        }))
        .unwrap();
        let service =
            ServiceContext::new(std::sync::Arc::new(crate::hooks::HookPipeline::new(vec![])));
        let config = Config::default();
        assert!(service.validate_game_paths(&config, &game).is_ok());
        assert!(service.capture_plan(&config, &game).is_err());
    }

    #[test]
    fn concrete_paths_resolve_placeholders_before_preflight() {
        let temp = temp_dir::TempDir::new().unwrap();
        std::fs::write(temp.path().join("save.dat"), b"save").unwrap();
        let context = crate::path_resolver::PathContext {
            game_roots: vec![temp.path().to_string_lossy().into_owned()],
            ..Default::default()
        };

        let report = resolve_concrete(
            Some(&"<root>/save.dat".to_string()),
            Some(&SaveUnitType::File),
            Some(&context),
            ResolutionPurpose::Capture,
        );

        assert_eq!(report.locations.len(), 1);
        assert_eq!(
            PathBuf::from(&report.candidates[0].expression),
            temp.path().join("save.dat")
        );
        assert_eq!(
            PathBuf::from(&report.locations[0].path),
            temp.path().join("save.dat")
        );
    }

    #[test]
    fn concrete_candidate_escapes_glob_characters_but_keeps_the_literal_target() {
        let temp = temp_dir::TempDir::new().unwrap();
        let save = temp.path().join("Games[Main]").join("slot*.sav");
        let path = save.to_string_lossy().into_owned();

        let report = resolve_concrete(
            Some(&path),
            Some(&SaveUnitType::File),
            None,
            ResolutionPurpose::Restore,
        );

        assert_eq!(report.candidates.len(), 1);
        let candidate = &report.candidates[0];
        assert!(candidate.is_exact());
        assert_eq!(candidate.exact_target_path(), Some(save));
    }

    #[test]
    fn concrete_restore_targets_do_not_need_to_exist() {
        let temp = temp_dir::TempDir::new().unwrap();
        let target = temp.path().join("missing").join("save.dat");

        let report = resolve_concrete(
            Some(&target.to_string_lossy().into_owned()),
            Some(&SaveUnitType::File),
            None,
            ResolutionPurpose::Restore,
        );

        assert_eq!(report.candidates.len(), 1);
        assert_eq!(PathBuf::from(&report.candidates[0].expression), target);
        assert!(report.diagnostics.is_empty());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn unsupported_registry_units_are_non_blocking() {
        let report = resolve_concrete(
            Some(&"HKEY_CURRENT_USER/Software/Game".to_string()),
            Some(&SaveUnitType::WinRegistry),
            None,
            ResolutionPurpose::Capture,
        );

        assert!(report.locations.is_empty());
        assert!(report.diagnostics.is_empty());
        assert!(matches!(
            report.selection_state,
            ResolutionSelectionState::Explicit { .. }
        ));
    }
}
