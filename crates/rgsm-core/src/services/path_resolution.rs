use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::backup::{
    CapturePlan, Game, SaveUnit, SaveUnitCaptureInput, SaveUnitSource, SaveUnitType,
};
use crate::config::Config;
use crate::device::{DeviceResourceKind, get_current_device_id};
use crate::path_pattern::{PathPatternError, PlatformKind, parse_manifest_path_pattern};
use crate::path_resolution::{
    CandidateDimensions, CandidateExpression, GameInstallationCandidate, GameRootCandidate,
    PlatformPaths, ResolutionContext, ResolutionDiagnostic, ResolutionDiagnosticKind,
    ResolutionReport, ResolutionSelection, ResolutionSelectionState, ResolvedLocationKind,
    ResolvedSaveLocation, StoreAccountCandidate, match_resolution_plan, plan_resolution,
};

use super::ServiceContext;

impl ServiceContext {
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
        let device_id = get_current_device_id();
        match &save_unit.source {
            SaveUnitSource::Concrete { unit_type, paths } => {
                let path_context = game.path_context(config.devices.get(device_id));
                resolve_concrete(paths.get(device_id), unit_type, Some(&path_context))
            }
            SaveUnitSource::ManifestPattern {
                pattern,
                constraints,
            } => {
                let context = resolution_context(config, game, device_id);
                let parsed = match parse_manifest_path_pattern(pattern.raw()) {
                    Ok(parsed) => parsed,
                    Err(error) => return invalid_pattern_report(pattern.raw(), error),
                };
                let plan = plan_resolution(&parsed, constraints.clone(), &context);
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

fn resolution_context(config: &Config, game: &Game, device_id: &str) -> ResolutionContext {
    let mut roots = Vec::new();
    let mut accounts = Vec::new();
    let mut installations = Vec::new();
    if let Some(device) = config.devices.get(device_id) {
        for resource in &device.resources {
            let id = resource_id(resource.id);
            match &resource.kind {
                DeviceResourceKind::GameRoot { store, path } => roots.push(GameRootCandidate {
                    id,
                    store: *store,
                    path: PathBuf::from(path),
                }),
                DeviceResourceKind::StoreAccount { store, user_id } => {
                    accounts.push(StoreAccountCandidate {
                        id,
                        store: *store,
                        user_id: user_id.clone(),
                    });
                }
                DeviceResourceKind::GameInstallation {
                    root_id,
                    store,
                    install_dir,
                    path,
                    store_game_id,
                } => installations.push(GameInstallationCandidate {
                    id,
                    root_id: resource_id(*root_id),
                    store: *store,
                    install_dir: install_dir.clone(),
                    install_path: PathBuf::from(path),
                    store_game_id: store_game_id.clone(),
                }),
            }
        }
    }

    let binding = game.device_bindings.get(device_id);
    ResolutionContext {
        platform: PlatformKind::host(),
        platform_paths: host_platform_paths(),
        roots,
        accounts,
        installations,
        store_game_ids: game
            .ludusavi_meta
            .iter()
            .flat_map(|meta| &meta.store_game_ids)
            .map(|entry| (entry.store, entry.id.clone()))
            .collect::<BTreeMap<_, _>>(),
        selection: ResolutionSelection {
            root_ids: binding.and_then(|value| selected_ids(value.root_ids.as_deref())),
            account_ids: binding.and_then(|value| selected_ids(value.account_ids.as_deref())),
            installation_ids: binding
                .and_then(|value| selected_ids(value.installation_ids.as_deref())),
        },
    }
}

fn selected_ids(ids: Option<&[u32]>) -> Option<BTreeSet<String>> {
    ids.map(|ids| ids.iter().copied().map(resource_id).collect())
}

fn resource_id(id: u32) -> String {
    format!("resource:{id}")
}

fn host_platform_paths() -> PlatformPaths {
    let home = dirs::home_dir();
    PlatformPaths {
        home: home.clone(),
        os_user_name: Some(whoami::username()),
        win_app_data: dirs::data_dir(),
        win_local_app_data: dirs::data_local_dir(),
        win_local_app_data_low: home.map(|path| path.join("AppData").join("LocalLow")),
        win_documents: dirs::document_dir(),
        win_public: std::env::var_os("PUBLIC").map(PathBuf::from),
        win_program_data: std::env::var_os("PROGRAMDATA").map(PathBuf::from),
        win_dir: std::env::var_os("WINDIR").map(PathBuf::from),
        xdg_data: None,
        xdg_config: None,
    }
}

fn resolve_concrete(
    path: Option<&String>,
    unit_type: &SaveUnitType,
    path_context: Option<&crate::path_resolver::PathContext>,
) -> ResolutionReport {
    let Some(path) = path else {
        return blocked_report(
            "",
            "this save location is not configured for the current device",
        );
    };

    #[cfg(not(target_os = "windows"))]
    if matches!(unit_type, SaveUnitType::WinRegistry) {
        return empty_concrete_report(path);
    }

    let resolved = match crate::path_resolver::resolve_path_explicit(path, path_context) {
        Ok(resolved) => resolved,
        Err(error) => return blocked_report(path, &error.to_string()),
    };
    let source = resolved.as_path();
    let exists = match unit_type {
        SaveUnitType::File => source.is_file(),
        SaveUnitType::Folder => source.is_dir(),
        SaveUnitType::WinRegistry => {
            crate::backup::registry::registry_key_exists(&resolved.to_string_lossy())
                .unwrap_or(false)
        }
    };
    if !exists {
        return blocked_report(path, "the configured save location is unavailable");
    }
    let kind = match unit_type {
        SaveUnitType::File => ResolvedLocationKind::File,
        SaveUnitType::Folder => ResolvedLocationKind::Directory,
        SaveUnitType::WinRegistry => ResolvedLocationKind::Registry,
    };
    ResolutionReport {
        raw_pattern: path.clone(),
        selection_state: ResolutionSelectionState::Explicit {
            candidate_ids: vec!["concrete".to_string()],
        },
        candidates: vec![CandidateExpression {
            id: "concrete".to_string(),
            expression: resolved.to_string_lossy().into_owned(),
            logical_anchor: source
                .parent()
                .unwrap_or(source)
                .to_string_lossy()
                .into_owned(),
            dimensions: CandidateDimensions::default(),
            case_sensitive: !cfg!(target_os = "windows"),
        }],
        locations: vec![ResolvedSaveLocation {
            path: resolved.to_string_lossy().into_owned(),
            kind,
            candidate_id: "concrete".to_string(),
            logical_anchor: source
                .parent()
                .unwrap_or(source)
                .to_string_lossy()
                .into_owned(),
            dimensions: CandidateDimensions::default(),
        }],
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
mod concrete_tests {
    use super::*;

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
            &SaveUnitType::File,
            Some(&context),
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

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn unsupported_registry_units_are_non_blocking() {
        let report = resolve_concrete(
            Some(&"HKEY_CURRENT_USER/Software/Game".to_string()),
            &SaveUnitType::WinRegistry,
            None,
        );

        assert!(report.locations.is_empty());
        assert!(report.diagnostics.is_empty());
        assert!(matches!(
            report.selection_state,
            ResolutionSelectionState::Explicit { .. }
        ));
    }
}
