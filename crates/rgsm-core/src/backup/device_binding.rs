use serde::{Deserialize, Serialize};
use specta::Type;

use crate::device::DeviceResourceId;
use crate::path_resolution::CandidateDimensions;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", from = "SavePathOverrideWire")]
pub struct SavePathOverride {
    #[serde(rename = "expression")]
    pub path: String,
}

#[derive(Deserialize)]
struct SavePathOverrideWire {
    expression: Option<String>,
    path: Option<String>,
}

impl From<SavePathOverrideWire> for SavePathOverride {
    fn from(wire: SavePathOverrideWire) -> Self {
        Self {
            path: wire.expression.unwrap_or_else(|| {
                super::save_unit::upgrade_literal_path(wire.path.unwrap_or_default())
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreMappingRule {
    pub save_unit_id: u32,
    pub source_dimensions: CandidateDimensions,
    pub target_candidate_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct GameDeviceBinding {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub path_variables: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub path_overrides: BTreeMap<u32, SavePathOverride>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_ids: Option<Vec<DeviceResourceId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_ids: Option<Vec<DeviceResourceId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation_ids: Option<Vec<DeviceResourceId>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restore_mappings: Vec<RestoreMappingRule>,
}

impl GameDeviceBinding {
    pub fn is_explicit(&self) -> bool {
        self.root_ids.is_some() || self.account_ids.is_some() || self.installation_ids.is_some()
    }
}
