use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::path_pattern::{
    ManifestPathConstraints, ParsedManifestPathPattern, PathPlaceholder, PlatformKind,
};

use super::model::{first_unescaped_glob, unescape_glob_literal};

use super::{
    CandidateDimensions, CandidateExpression, GameInstallationCandidate, GameRootCandidate,
    ResolutionContext, ResolutionDiagnostic, ResolutionDiagnosticKind, ResolutionPlan,
    ResolutionSelectionState, StoreAccountCandidate,
};

#[derive(Debug, Clone, Default)]
struct CandidateParts<'a> {
    root: Option<&'a GameRootCandidate>,
    account: Option<&'a StoreAccountCandidate>,
    installation: Option<&'a GameInstallationCandidate>,
}

pub fn plan_resolution(
    parsed: &ParsedManifestPathPattern,
    constraints: ManifestPathConstraints,
    context: &ResolutionContext,
) -> ResolutionPlan {
    let mut diagnostics = Vec::new();
    if !constraints.allows_platform(context.platform) {
        diagnostics.push(diagnostic(
            ResolutionDiagnosticKind::UnsupportedPlatform,
            "the manifest path does not apply to this platform",
        ));
        return empty_plan(parsed, constraints, diagnostics, context);
    }

    if context.platform == PlatformKind::Windows
        && (parsed.contains(PathPlaceholder::XdgData)
            || parsed.contains(PathPlaceholder::XdgConfig))
    {
        diagnostics.push(diagnostic(
            ResolutionDiagnosticKind::UnsupportedPlatform,
            "XDG path placeholders are not applicable on Windows",
        ));
        return empty_plan(parsed, constraints, diagnostics, context);
    }

    let stale_ids = stale_selected_ids(context);
    let combinations = build_combinations(parsed, &constraints, context);
    let mut candidates = Vec::new();
    for parts in combinations {
        match render_candidate(parsed, context, &parts) {
            Ok(candidate) => candidates.push(candidate),
            Err(message) => diagnostics.push(diagnostic(
                ResolutionDiagnosticKind::MissingContext,
                message,
            )),
        }
    }

    candidates.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then(left.expression.cmp(&right.expression))
    });
    candidates.dedup_by(|left, right| left.id == right.id && left.expression == right.expression);

    let candidate_ids = candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();
    let selection_state =
        if !stale_ids.is_empty() || (context.selection.is_explicit() && candidates.is_empty()) {
            ResolutionSelectionState::StaleSelection {
                selected_resource_ids: stale_ids,
                candidate_ids,
            }
        } else if context.selection.is_explicit() {
            ResolutionSelectionState::Explicit { candidate_ids }
        } else {
            match candidate_ids.as_slice() {
                [] => ResolutionSelectionState::Missing,
                [candidate_id] => ResolutionSelectionState::ImplicitUnique {
                    candidate_id: candidate_id.clone(),
                },
                _ => {
                    diagnostics.push(diagnostic(
                        ResolutionDiagnosticKind::MultipleCandidates,
                        "multiple save-location candidates require a device selection",
                    ));
                    ResolutionSelectionState::Ambiguous { candidate_ids }
                }
            }
        };

    if candidates.is_empty() && diagnostics.is_empty() {
        diagnostics.push(diagnostic(
            ResolutionDiagnosticKind::NoCandidate,
            "no save-location candidate could be constructed",
        ));
    }

    ResolutionPlan {
        pattern: parsed.pattern.clone(),
        constraints,
        candidates,
        selection_state,
        diagnostics,
    }
}

fn empty_plan(
    parsed: &ParsedManifestPathPattern,
    constraints: ManifestPathConstraints,
    diagnostics: Vec<ResolutionDiagnostic>,
    context: &ResolutionContext,
) -> ResolutionPlan {
    let selected_resource_ids = selected_ids(context);
    ResolutionPlan {
        pattern: parsed.pattern.clone(),
        constraints,
        candidates: Vec::new(),
        selection_state: if context.selection.is_explicit() {
            ResolutionSelectionState::StaleSelection {
                selected_resource_ids,
                candidate_ids: Vec::new(),
            }
        } else {
            ResolutionSelectionState::Missing
        },
        diagnostics,
    }
}

fn build_combinations<'a>(
    parsed: &ParsedManifestPathPattern,
    constraints: &ManifestPathConstraints,
    context: &'a ResolutionContext,
) -> Vec<CandidateParts<'a>> {
    let needs_installation =
        parsed.contains(PathPlaceholder::Game) || parsed.contains(PathPlaceholder::Base);
    let needs_root = parsed.contains(PathPlaceholder::Root);
    let needs_account = parsed.contains(PathPlaceholder::StoreUserId);

    let mut bases = if needs_installation {
        context
            .installations
            .iter()
            .filter(|installation| constraints.allows(context.platform, Some(installation.store)))
            .filter(|installation| selected(&context.selection.installation_ids, &installation.id))
            .filter_map(|installation| {
                let root = context
                    .roots
                    .iter()
                    .find(|root| root.id == installation.root_id)?;
                selected(&context.selection.root_ids, &root.id).then_some(CandidateParts {
                    root: Some(root),
                    installation: Some(installation),
                    account: None,
                })
            })
            .collect::<Vec<_>>()
    } else if needs_root {
        context
            .roots
            .iter()
            .filter(|root| constraints.allows(context.platform, Some(root.store)))
            .filter(|root| selected(&context.selection.root_ids, &root.id))
            .map(|root| CandidateParts {
                root: Some(root),
                ..CandidateParts::default()
            })
            .collect()
    } else {
        vec![CandidateParts::default()]
    };

    if !needs_account {
        return bases;
    }

    let mut combinations = Vec::new();
    for base in bases.drain(..) {
        let store = base
            .installation
            .map(|installation| installation.store)
            .or_else(|| base.root.map(|root| root.store));
        for account in context.accounts.iter().filter(|account| {
            constraints.allows(context.platform, Some(account.store))
                && store.is_none_or(|store| store == account.store)
                && selected(&context.selection.account_ids, &account.id)
        }) {
            combinations.push(CandidateParts {
                account: Some(account),
                ..base.clone()
            });
        }
    }
    combinations
}

fn render_candidate(
    parsed: &ParsedManifestPathPattern,
    context: &ResolutionContext,
    parts: &CandidateParts<'_>,
) -> Result<CandidateExpression, String> {
    let mut expression = parsed.pattern.raw().replace('\\', "/");
    let store = parts
        .installation
        .map(|installation| installation.store)
        .or_else(|| parts.root.map(|root| root.store))
        .or_else(|| parts.account.map(|account| account.store));

    replace_path(
        &mut expression,
        PathPlaceholder::Home,
        context.platform_paths.home.as_deref(),
    )?;
    replace_text(
        &mut expression,
        PathPlaceholder::OsUserName,
        context.platform_paths.os_user_name.as_deref(),
    )?;
    replace_path(
        &mut expression,
        PathPlaceholder::WinAppData,
        context.platform_paths.win_app_data.as_deref(),
    )?;
    replace_path(
        &mut expression,
        PathPlaceholder::WinLocalAppData,
        context.platform_paths.win_local_app_data.as_deref(),
    )?;
    replace_path(
        &mut expression,
        PathPlaceholder::WinLocalAppDataLow,
        context.platform_paths.win_local_app_data_low.as_deref(),
    )?;
    replace_path(
        &mut expression,
        PathPlaceholder::WinDocuments,
        context.platform_paths.win_documents.as_deref(),
    )?;
    replace_path(
        &mut expression,
        PathPlaceholder::WinPublic,
        context.platform_paths.win_public.as_deref(),
    )?;
    replace_path(
        &mut expression,
        PathPlaceholder::WinProgramData,
        context.platform_paths.win_program_data.as_deref(),
    )?;
    replace_path(
        &mut expression,
        PathPlaceholder::WinDir,
        context.platform_paths.win_dir.as_deref(),
    )?;
    replace_path(
        &mut expression,
        PathPlaceholder::XdgData,
        context.platform_paths.xdg_data.as_deref(),
    )?;
    replace_path(
        &mut expression,
        PathPlaceholder::XdgConfig,
        context.platform_paths.xdg_config.as_deref(),
    )?;

    replace_path(
        &mut expression,
        PathPlaceholder::Root,
        parts.root.map(|root| root.path.as_path()),
    )?;
    replace_text(
        &mut expression,
        PathPlaceholder::Game,
        parts
            .installation
            .map(|installation| installation.install_dir.as_str()),
    )?;
    replace_path(
        &mut expression,
        PathPlaceholder::Base,
        parts
            .installation
            .map(|installation| installation.install_path.as_path()),
    )?;
    let store_game_id = parts
        .installation
        .and_then(|installation| installation.store_game_id.as_deref())
        .or_else(|| store.and_then(|store| context.store_game_ids.get(&store).map(String::as_str)))
        .or_else(|| {
            (context.store_game_ids.len() == 1)
                .then(|| context.store_game_ids.values().next().map(String::as_str))
                .flatten()
        });
    replace_text(&mut expression, PathPlaceholder::StoreGameId, store_game_id)?;
    replace_text(
        &mut expression,
        PathPlaceholder::StoreUserId,
        parts.account.map(|account| account.user_id.as_str()),
    )?;
    if expression.contains("<storeuserid>") {
        let user_id = parts
            .account
            .map(|account| account.user_id.as_str())
            .ok_or_else(|| "<storeuserid> requires resolution context".to_string())?;
        expression = expression.replace("<storeuserid>", &globset::escape(user_id));
    }

    let dimensions = CandidateDimensions {
        root_id: parts.root.map(|root| root.id.clone()),
        account_id: parts.account.map(|account| account.id.clone()),
        installation_id: parts
            .installation
            .map(|installation| installation.id.clone()),
        store,
    };
    let id = candidate_id(&dimensions);
    let logical_anchor = glob_logical_anchor(&expression);

    Ok(CandidateExpression {
        id,
        expression,
        logical_anchor: logical_anchor.to_string_lossy().into_owned(),
        dimensions,
        case_sensitive: context.platform != PlatformKind::Windows,
    })
}

fn glob_logical_anchor(expression: &str) -> PathBuf {
    let first_glob = first_unescaped_glob(expression);
    let anchor = match first_glob {
        Some(index) => expression[..index]
            .rfind('/')
            .map(|slash| &expression[..slash])
            .unwrap_or("."),
        None => expression,
    };
    PathBuf::from(unescape_glob_literal(anchor))
}

fn replace_path(
    expression: &mut String,
    placeholder: PathPlaceholder,
    value: Option<&Path>,
) -> Result<(), String> {
    let value = value.map(|path| path.to_string_lossy().replace('\\', "/"));
    replace_text(expression, placeholder, value.as_deref())
}

fn replace_text(
    expression: &mut String,
    placeholder: PathPlaceholder,
    value: Option<&str>,
) -> Result<(), String> {
    if !expression.contains(placeholder.token()) {
        return Ok(());
    }
    let value =
        value.ok_or_else(|| format!("{} requires resolution context", placeholder.token()))?;
    *expression = expression.replace(placeholder.token(), &globset::escape(value));
    Ok(())
}

fn selected(selection: &Option<BTreeSet<String>>, id: &str) -> bool {
    selection
        .as_ref()
        .is_none_or(|selected| selected.contains(id))
}

fn selected_ids(context: &ResolutionContext) -> Vec<String> {
    context
        .selection
        .root_ids
        .iter()
        .chain(context.selection.account_ids.iter())
        .chain(context.selection.installation_ids.iter())
        .flat_map(|ids| ids.iter().cloned())
        .collect()
}

fn stale_selected_ids(context: &ResolutionContext) -> Vec<String> {
    let known = context
        .roots
        .iter()
        .map(|resource| resource.id.as_str())
        .chain(context.accounts.iter().map(|resource| resource.id.as_str()))
        .chain(
            context
                .installations
                .iter()
                .map(|resource| resource.id.as_str()),
        )
        .collect::<BTreeSet<_>>();
    selected_ids(context)
        .into_iter()
        .filter(|id| !known.contains(id.as_str()))
        .collect()
}

fn candidate_id(dimensions: &CandidateDimensions) -> String {
    let mut parts = Vec::new();
    if let Some(root_id) = &dimensions.root_id {
        parts.push(format!("root:{root_id}"));
    }
    if let Some(installation_id) = &dimensions.installation_id {
        parts.push(format!("install:{installation_id}"));
    }
    if let Some(account_id) = &dimensions.account_id {
        parts.push(format!("account:{account_id}"));
    }
    if parts.is_empty() {
        "platform".to_string()
    } else {
        parts.join("|")
    }
}

fn diagnostic(kind: ResolutionDiagnosticKind, message: impl Into<String>) -> ResolutionDiagnostic {
    ResolutionDiagnostic {
        kind,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    use super::*;
    use crate::path_pattern::{
        ManifestPathCondition, ManifestPathConstraints, StoreKind, parse_manifest_path_pattern,
    };
    use crate::path_resolution::{PlatformPaths, ResolutionSelection};

    fn context() -> ResolutionContext {
        ResolutionContext {
            platform: PlatformKind::Windows,
            platform_paths: PlatformPaths {
                home: Some(PathBuf::from("C:/Users/Player")),
                os_user_name: Some("Player".to_string()),
                win_app_data: Some(PathBuf::from("C:/Users/Player/AppData/Roaming")),
                win_local_app_data: Some(PathBuf::from("C:/Users/Player/AppData/Local")),
                win_local_app_data_low: Some(PathBuf::from("C:/Users/Player/AppData/LocalLow")),
                win_documents: Some(PathBuf::from("C:/Users/Player/Documents")),
                win_public: Some(PathBuf::from("C:/Users/Public")),
                win_program_data: Some(PathBuf::from("C:/ProgramData")),
                win_dir: Some(PathBuf::from("C:/Windows")),
                ..PlatformPaths::default()
            },
            roots: vec![
                GameRootCandidate {
                    id: "root-a".to_string(),
                    store: StoreKind::Steam,
                    path: PathBuf::from("D:/Steam[Main]"),
                },
                GameRootCandidate {
                    id: "root-b".to_string(),
                    store: StoreKind::Steam,
                    path: PathBuf::from("E:/Steam"),
                },
            ],
            accounts: vec![
                StoreAccountCandidate {
                    id: "account-a".to_string(),
                    store: StoreKind::Steam,
                    user_id: "111".to_string(),
                },
                StoreAccountCandidate {
                    id: "account-b".to_string(),
                    store: StoreKind::Steam,
                    user_id: "222".to_string(),
                },
            ],
            installations: vec![
                GameInstallationCandidate {
                    id: "install-a".to_string(),
                    root_id: "root-a".to_string(),
                    store: StoreKind::Steam,
                    install_dir: "Game[One]".to_string(),
                    install_path: PathBuf::from("D:/Steam[Main]/steamapps/common/Game[One]"),
                    store_game_id: Some("10".to_string()),
                },
                GameInstallationCandidate {
                    id: "install-b".to_string(),
                    root_id: "root-b".to_string(),
                    store: StoreKind::Steam,
                    install_dir: "Game Two".to_string(),
                    install_path: PathBuf::from("E:/Steam/steamapps/common/Game Two"),
                    store_game_id: Some("10".to_string()),
                },
            ],
            store_game_ids: BTreeMap::from([(StoreKind::Steam, "10".to_string())]),
            selection: ResolutionSelection::default(),
        }
    }

    #[test]
    fn retains_all_root_and_account_combinations() {
        let parsed = parse_manifest_path_pattern("<root>/userdata/<storeUserId>/10/*.sav").unwrap();

        let plan = plan_resolution(&parsed, ManifestPathConstraints::default(), &context());

        assert_eq!(plan.candidates.len(), 4);
        assert!(matches!(
            plan.selection_state,
            ResolutionSelectionState::Ambiguous { .. }
        ));
        assert!(
            plan.candidates
                .iter()
                .any(|candidate| candidate.logical_anchor.ends_with("userdata/111/10"))
        );
    }

    #[test]
    fn root_only_patterns_do_not_inherit_installation_filtering() {
        let parsed = parse_manifest_path_pattern("<root>/saves/*.sav").unwrap();
        let context = context();

        let plan = plan_resolution(&parsed, ManifestPathConstraints::default(), &context);

        assert_eq!(plan.candidates.len(), 2);
        assert!(
            plan.candidates
                .iter()
                .any(|candidate| candidate.dimensions.root_id.as_deref() == Some("root-b"))
        );
    }

    #[test]
    fn when_alternatives_are_or_while_each_condition_is_and() {
        let parsed = parse_manifest_path_pattern("<base>/config.py").unwrap();
        let mut context = context();
        context.roots.push(GameRootCandidate {
            id: "root-gog".to_string(),
            store: StoreKind::Gog,
            path: PathBuf::from("F:/GOG"),
        });
        context.installations.push(GameInstallationCandidate {
            id: "install-gog".to_string(),
            root_id: "root-gog".to_string(),
            store: StoreKind::Gog,
            install_dir: "Game".to_string(),
            install_path: PathBuf::from("F:/GOG/Game"),
            store_game_id: None,
        });

        let alternatives = ManifestPathConstraints {
            alternatives: vec![
                ManifestPathCondition {
                    os: None,
                    store: Some(StoreKind::Steam),
                },
                ManifestPathCondition {
                    os: Some(PlatformKind::Windows),
                    store: None,
                },
            ],
        };
        let plan = plan_resolution(&parsed, alternatives, &context);
        assert_eq!(plan.candidates.len(), 3);

        let combined = ManifestPathConstraints {
            alternatives: vec![ManifestPathCondition {
                os: Some(PlatformKind::Windows),
                store: Some(StoreKind::Steam),
            }],
        };
        let plan = plan_resolution(&parsed, combined, &context);
        assert_eq!(plan.candidates.len(), 2);
        assert!(plan.candidates.iter().all(|candidate| {
            candidate.dimensions.installation_id.as_deref() != Some("install-gog")
        }));
    }

    #[test]
    fn implicit_unique_is_not_persisted_as_explicit() {
        let parsed = parse_manifest_path_pattern("<root>/save.dat").unwrap();
        let mut context = context();
        context.roots.truncate(1);
        context.installations.clear();

        let plan = plan_resolution(&parsed, ManifestPathConstraints::default(), &context);

        assert!(matches!(
            plan.selection_state,
            ResolutionSelectionState::ImplicitUnique { .. }
        ));
    }

    #[test]
    fn explicit_selection_remains_stable_when_other_resources_exist() {
        let parsed = parse_manifest_path_pattern("<base>/save/*.sav").unwrap();
        let mut context = context();
        context.selection.installation_ids = Some(BTreeSet::from(["install-b".to_string()]));

        let plan = plan_resolution(&parsed, ManifestPathConstraints::default(), &context);

        assert_eq!(plan.candidates.len(), 1);
        assert!(matches!(
            plan.selection_state,
            ResolutionSelectionState::Explicit { .. }
        ));
        assert!(plan.candidates[0].expression.contains("Game Two"));
    }

    #[test]
    fn missing_selected_resource_is_stale() {
        let parsed = parse_manifest_path_pattern("<root>/save.dat").unwrap();
        let mut context = context();
        context.selection.root_ids = Some(BTreeSet::from(["missing".to_string()]));

        let plan = plan_resolution(&parsed, ManifestPathConstraints::default(), &context);

        assert!(matches!(
            plan.selection_state,
            ResolutionSelectionState::StaleSelection { .. }
        ));
    }

    #[test]
    fn escapes_literal_resource_glob_characters() {
        let parsed = parse_manifest_path_pattern("<base>/Save[0-9]/*.sav").unwrap();
        let mut context = context();
        context.selection.installation_ids = Some(BTreeSet::from(["install-a".to_string()]));

        let plan = plan_resolution(&parsed, ManifestPathConstraints::default(), &context);

        assert_eq!(
            plan.candidates[0].expression,
            "D:/Steam[[]Main[]]/steamapps/common/Game[[]One[]]/Save[0-9]/*.sav"
        );
    }

    #[test]
    fn literal_brackets_in_resources_do_not_truncate_logical_anchor() {
        let parsed = parse_manifest_path_pattern("<base>/save/*.sav").unwrap();
        let mut context = context();
        context.selection.installation_ids = Some(BTreeSet::from(["install-a".to_string()]));

        let plan = plan_resolution(&parsed, ManifestPathConstraints::default(), &context);

        assert_eq!(plan.candidates.len(), 1);
        assert_eq!(
            plan.candidates[0].logical_anchor,
            "D:/Steam[Main]/steamapps/common/Game[One]/save"
        );
    }

    #[test]
    fn xdg_placeholders_are_typed_as_non_applicable_on_windows() {
        let parsed = parse_manifest_path_pattern("<xdgData>/game/save").unwrap();

        let plan = plan_resolution(&parsed, ManifestPathConstraints::default(), &context());

        assert!(plan.candidates.is_empty());
        assert_eq!(
            plan.diagnostics[0].kind,
            ResolutionDiagnosticKind::UnsupportedPlatform
        );
    }

    #[test]
    fn every_windows_placeholder_renders_from_the_typed_context() {
        for placeholder in PathPlaceholder::ALL
            .into_iter()
            .filter(|placeholder| placeholder.windows_applicable())
        {
            let parsed = parse_manifest_path_pattern(placeholder.token()).unwrap();
            let plan = plan_resolution(&parsed, ManifestPathConstraints::default(), &context());

            assert!(
                !plan.candidates.is_empty(),
                "{} produced no candidates: {:?}",
                placeholder.token(),
                plan.diagnostics
            );
            assert!(
                plan.candidates
                    .iter()
                    .all(|candidate| !candidate.expression.contains('<')),
                "{} remained unresolved",
                placeholder.token()
            );
        }
    }
}
