use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    backup::{CapturePlan, CaptureSourceKind, CreatedBy},
    device::DeviceId,
    preclude::CompressError,
};

use super::ArchiveManifest;

pub const MANIFEST_ENTRY: &str = "_rgsm/manifest.json";
pub const RECOVERY_ENTRY: &str = "RESTORE.txt";
pub const CURRENT_VERSION: u32 = 6;

/// Immutable capture facts. Later catalog edits do not rewrite an archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveIdentity {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recovered_metadata: bool,
    pub game_id: String,
    pub game_name: String,
    pub snapshot_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    /// Original wall clock when historical data contains no time zone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_local_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<DeviceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub description: String,
    pub created_by: CreatedBy,
    pub locations: Vec<ArchiveLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveLocation {
    pub save_unit_id: u32,
    pub expression: String,
}

pub(super) fn prepare_archive(
    plan: &CapturePlan,
    identity: ArchiveIdentity,
    fingerprint: Option<String>,
) -> Result<(CapturePlan, ArchiveManifest), CompressError> {
    if identity.game_id.is_empty() || identity.snapshot_id.is_empty() || plan.groups.is_empty() {
        return Err(invalid(
            "archive identity and captured save data are required",
        ));
    }
    let mut plan = plan.clone();
    let mut roots = BTreeMap::new();
    let mut used = BTreeSet::from(["_rgsm".to_string(), RECOVERY_ENTRY.to_lowercase()]);
    for group in &plan.groups {
        if roots.contains_key(&group.save_unit_id) {
            continue;
        }
        let name = match group.kind {
            CaptureSourceKind::Registry => {
                format!("{}.reg", basename(&group.logical_anchor.to_string_lossy()))
            }
            _ if group.relative_path.is_empty() => basename(&group.source_path),
            _ => basename(&group.logical_anchor.to_string_lossy()),
        };
        let name = safe_component(&name);
        let mut unique = name.clone();
        let mut suffix = 2;
        while !used.insert(unique.to_lowercase()) {
            unique = disambiguate(&name, suffix);
            suffix += 1;
        }
        roots.insert(group.save_unit_id, unique);
    }
    for group in &mut plan.groups {
        let root = &roots[&group.save_unit_id];
        group.archive_path =
            if group.relative_path.is_empty() || group.kind == CaptureSourceKind::Registry {
                root.clone()
            } else {
                validate_relative_name(&group.relative_path)?;
                format!("{root}/{}", group.relative_path.replace('\\', "/"))
            };
    }
    // A recursive glob may match both a directory and its children. The parent
    // already captures those children; keep one archive/restore entry for it.
    let groups = plan.groups.clone();
    plan.groups.retain(|group| {
        !groups.iter().any(|parent| {
            parent.id != group.id
                && parent.save_unit_id == group.save_unit_id
                && parent.kind == CaptureSourceKind::Directory
                && group
                    .archive_path
                    .starts_with(&format!("{}/", parent.archive_path))
        })
    });
    let mut names = BTreeSet::new();
    for group in &plan.groups {
        if !names.insert(group.archive_path.to_lowercase()) {
            return Err(invalid(
                "save data contains colliding portable archive paths",
            ));
        }
        if !identity
            .locations
            .iter()
            .any(|location| location.save_unit_id == group.save_unit_id)
        {
            return Err(invalid("a captured save unit has no location description"));
        }
    }
    let mut manifest = ArchiveManifest::from(&plan);
    manifest.version = CURRENT_VERSION;
    manifest.source_fingerprint = fingerprint;
    manifest.identity = Some(identity);
    for group in &mut manifest.groups {
        group.candidate_id.clear();
        group.dimensions = Default::default();
    }
    Ok((plan, manifest))
}

pub(super) fn recovery_instructions(manifest: &ArchiveManifest) -> String {
    let identity = manifest
        .identity
        .as_ref()
        .expect("current archive identity");
    let time = identity
        .created_at
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|time| time.to_rfc3339())
        .or_else(|| {
            identity
                .legacy_local_time
                .as_ref()
                .map(|time| rust_i18n::t!("portable_archive.local_time", time = time).into_owned())
        })
        .unwrap_or_else(|| rust_i18n::t!("portable_archive.unknown_time").into_owned());
    let mut text = format!(
        "{}\n\n{}\n\n",
        rust_i18n::t!(
            "portable_archive.title",
            game = identity.game_name,
            time = time
        ),
        rust_i18n::t!("portable_archive.instructions")
    );
    if identity.recovered_metadata {
        text.push_str(&format!(
            "{}\n\n",
            rust_i18n::t!("portable_archive.recovered_metadata")
        ));
    }
    for group in &manifest.groups {
        let location = identity
            .locations
            .iter()
            .find(|location| location.save_unit_id == group.save_unit_id);
        text.push_str(&format!("{}\n", group.archive_path));
        if let Some(location) = location {
            text.push_str(&format!(
                "  {}\n",
                rust_i18n::t!("portable_archive.expression", path = location.expression)
            ));
        }
        if let Some(path) = &group.source_path_diagnostic {
            text.push_str(&format!(
                "  {}\n",
                rust_i18n::t!("portable_archive.source", path = path)
            ));
        }
        if group.kind == CaptureSourceKind::Registry {
            text.push_str(&format!(
                "  {}\n",
                rust_i18n::t!("portable_archive.registry")
            ));
        }
        text.push('\n');
    }
    text.push_str(&format!(
        "{}\n{}\n",
        rust_i18n::t!(
            "portable_archive.description",
            description = identity.description
        ),
        rust_i18n::t!(
            "portable_archive.identity",
            game = identity.game_id,
            snapshot = identity.snapshot_id
        )
    ));
    text
}

pub(super) fn validate_relative_name(name: &str) -> Result<(), CompressError> {
    if name.is_empty()
        || name
            .replace('\\', "/")
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == ".." || part.contains(':'))
    {
        return Err(invalid(&format!("unsafe archive path: {name}")));
    }
    Ok(())
}

fn basename(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("Saves")
        .to_string()
}

fn safe_component(name: &str) -> String {
    let value: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let value = value.trim_matches([' ', '.']);
    if value.is_empty() {
        "Saves".into()
    } else {
        value.into()
    }
}

fn disambiguate(name: &str, suffix: u32) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) => format!("{stem} ({suffix}).{extension}"),
        None => format!("{name} ({suffix})"),
    }
}

fn invalid(message: &str) -> CompressError {
    CompressError::Unexpected(anyhow::anyhow!(message.to_string()))
}

impl ArchiveIdentity {
    pub fn for_snapshot(
        game: &crate::backup::Game,
        snapshot: &crate::backup::Snapshot,
        device_id: &DeviceId,
    ) -> Self {
        let locations = game
            .save_paths
            .iter()
            .map(|unit| {
                let expression = game
                    .path_override(unit.id, device_id)
                    .map(|value| value.path.clone())
                    .or_else(|| {
                        unit.manifest_pattern()
                            .map(|(pattern, _)| pattern.raw().to_string())
                    })
                    .or_else(|| unit.get_path_for_device(device_id).cloned())
                    .unwrap_or_default();
                ArchiveLocation {
                    save_unit_id: unit.id,
                    expression,
                }
            })
            .collect();
        let legacy_local_time = if snapshot.created_at.is_none() {
            super::naming::legacy_archive_time(&snapshot.date)
        } else {
            None
        };
        Self {
            recovered_metadata: false,
            game_id: game.backup_dir_name().into_owned(),
            game_name: game.name.clone(),
            snapshot_id: snapshot.date.clone(),
            created_at: snapshot.created_at,
            legacy_local_time,
            device_id: snapshot.device_id.clone(),
            parent: snapshot.parent.clone(),
            description: snapshot.describe.clone(),
            created_by: snapshot.created_by.clone(),
            locations,
        }
    }
}
