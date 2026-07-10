use serde::{Deserialize, Serialize};

use crate::backup::{CapturePlan, CaptureSourceKind};
use crate::path_resolution::CandidateDimensions;

pub const V3_MANIFEST_ENTRY: &str = "_rgsm/manifest-v3.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveManifestV3 {
    pub version: u32,
    pub groups: Vec<ArchiveCaptureGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveCaptureGroup {
    pub id: u32,
    pub save_unit_id: u32,
    pub candidate_id: String,
    pub dimensions: CandidateDimensions,
    pub relative_path: String,
    pub archive_path: String,
    pub kind: CaptureSourceKind,
    pub delete_before_apply: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path_diagnostic: Option<String>,
}

impl From<&CapturePlan> for ArchiveManifestV3 {
    fn from(plan: &CapturePlan) -> Self {
        Self {
            version: 3,
            groups: plan
                .groups
                .iter()
                .map(|group| ArchiveCaptureGroup {
                    id: group.id,
                    save_unit_id: group.save_unit_id,
                    candidate_id: group.candidate_id.clone(),
                    dimensions: group.dimensions.clone(),
                    relative_path: group.relative_path.clone(),
                    archive_path: group.archive_path.clone(),
                    kind: group.kind,
                    delete_before_apply: group.delete_before_apply,
                    source_path_diagnostic: Some(group.source_path.clone()),
                })
                .collect(),
        }
    }
}
