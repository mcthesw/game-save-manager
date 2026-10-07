use serde::{Deserialize, Serialize};
use specta::Type;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct GameDeviceBinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation_path: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub path_variables: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub path_overrides: BTreeMap<u32, SavePathOverride>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "read_resource_ids"
    )]
    pub root_ids: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "read_resource_ids"
    )]
    pub account_ids: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "read_resource_ids"
    )]
    /// Unresolved historical selection, cleared when installation_path is set.
    /// Retains ambiguous imports without treating them as multiple instances.
    pub installation_ids: Option<Vec<String>>,
}

impl GameDeviceBinding {
    pub fn is_explicit(&self) -> bool {
        self.root_ids.is_some() || self.account_ids.is_some() || self.installation_ids.is_some()
    }
}

// Older local profiles stored only registered numeric resource IDs.
fn read_resource_ids<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        Current(String),
        Legacy(u32),
    }
    Ok(Option::<Vec<Id>>::deserialize(deserializer)?.map(|ids| {
        ids.into_iter()
            .map(|id| match id {
                Id::Current(value) => value,
                Id::Legacy(value) => format!("resource:{value}"),
            })
            .collect()
    }))
}
