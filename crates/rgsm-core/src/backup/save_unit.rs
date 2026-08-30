use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

use crate::default_value;
use crate::device::{DeviceId, get_current_device_id};
use crate::path_pattern::{ManifestPathConstraints, ManifestPathPattern};
use crate::path_resolver::PathContext;
use crate::preclude::BackupFileError;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type, utoipa::ToSchema)]
pub enum SaveUnitType {
    File,
    Folder,
    /// Windows Registry key tree (stored as `registry.reg` inside new archives).
    WinRegistry,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SaveUnitSource {
    Concrete {
        #[serde(alias = "unitType")]
        unit_type: SaveUnitType,
        #[serde(default)]
        paths: HashMap<DeviceId, String>,
    },
    ManifestPattern {
        /// Optional kind declared by the source that introduced the pattern.
        /// Legacy configurations always provide it; manifest imports may leave
        /// it unresolved and rely on the matched filesystem entry instead.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_type: Option<SaveUnitType>,
        pattern: ManifestPathPattern,
        #[serde(default)]
        constraints: ManifestPathConstraints,
    },
}

/// A save unit declares one concrete per-Device location or one portable
/// Manifest Path Pattern. A pattern preserves a known source kind when one was
/// declared, while imported patterns may defer the kind to each resolved match.
#[derive(Debug, Serialize, Clone, Type, utoipa::ToSchema)]
pub struct SaveUnit {
    #[serde(default)]
    pub id: u32,
    pub source: SaveUnitSource,
    #[serde(default = "default_value::default_false")]
    pub delete_before_apply: bool,
    #[serde(default = "default_value::default_true")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
struct SaveUnitWire {
    #[serde(default)]
    id: u32,
    #[serde(default)]
    source: Option<SaveUnitSource>,
    #[serde(default)]
    unit_type: Option<SaveUnitType>,
    #[serde(default)]
    paths: HashMap<DeviceId, String>,
    #[serde(default = "default_value::default_false")]
    delete_before_apply: bool,
    #[serde(default = "default_value::default_true")]
    enabled: bool,
}

impl<'de> Deserialize<'de> for SaveUnit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SaveUnitWire::deserialize(deserializer)?;
        let source = match (wire.source, wire.unit_type) {
            (Some(source), _) => source,
            (None, Some(unit_type)) => SaveUnitSource::Concrete {
                unit_type,
                paths: wire.paths,
            },
            (None, None) => {
                return Err(serde::de::Error::missing_field("source"));
            }
        };
        Ok(Self {
            id: wire.id,
            source,
            delete_before_apply: wire.delete_before_apply,
            enabled: wire.enabled,
        })
    }
}

#[derive(Debug, Serialize, Clone, Type, utoipa::ToSchema)]
pub struct SaveUnitDraft {
    #[serde(default)]
    pub id: Option<u32>,
    pub source: SaveUnitSource,
    #[serde(default = "default_value::default_false")]
    pub delete_before_apply: bool,
    #[serde(default = "default_value::default_true")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
struct SaveUnitDraftWire {
    #[serde(default)]
    id: Option<u32>,
    #[serde(default)]
    source: Option<SaveUnitSource>,
    #[serde(default)]
    unit_type: Option<SaveUnitType>,
    #[serde(default)]
    paths: HashMap<DeviceId, String>,
    #[serde(default = "default_value::default_false")]
    delete_before_apply: bool,
    #[serde(default = "default_value::default_true")]
    enabled: bool,
}

impl<'de> Deserialize<'de> for SaveUnitDraft {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SaveUnitDraftWire::deserialize(deserializer)?;
        let source = match (wire.source, wire.unit_type) {
            (Some(source), _) => source,
            (None, Some(unit_type)) => SaveUnitSource::Concrete {
                unit_type,
                paths: wire.paths,
            },
            (None, None) => {
                return Err(serde::de::Error::missing_field("source"));
            }
        };
        Ok(Self {
            id: wire.id,
            source,
            delete_before_apply: wire.delete_before_apply,
            enabled: wire.enabled,
        })
    }
}

impl SaveUnit {
    pub fn concrete(
        id: u32,
        unit_type: SaveUnitType,
        paths: HashMap<DeviceId, String>,
        delete_before_apply: bool,
        enabled: bool,
    ) -> Self {
        Self {
            id,
            source: SaveUnitSource::Concrete { unit_type, paths },
            delete_before_apply,
            enabled,
        }
    }

    pub fn get_path_for_device(&self, device_id: &DeviceId) -> Option<&String> {
        self.paths()?.get(device_id)
    }

    pub fn unit_type(&self) -> Option<&SaveUnitType> {
        match &self.source {
            SaveUnitSource::Concrete { unit_type, .. } => Some(unit_type),
            SaveUnitSource::ManifestPattern { expected_type, .. } => expected_type.as_ref(),
        }
    }

    pub fn paths(&self) -> Option<&HashMap<DeviceId, String>> {
        match &self.source {
            SaveUnitSource::Concrete { paths, .. } => Some(paths),
            SaveUnitSource::ManifestPattern { .. } => None,
        }
    }

    pub fn manifest_pattern(&self) -> Option<(&ManifestPathPattern, &ManifestPathConstraints)> {
        match &self.source {
            SaveUnitSource::ManifestPattern {
                pattern,
                constraints,
                ..
            } => Some((pattern, constraints)),
            SaveUnitSource::Concrete { .. } => None,
        }
    }

    /// Transitional concrete-path adapter. Dynamic sources are resolved by the
    /// application service and never enter this scalar compatibility path.
    pub fn resolve_path_for_current_device(
        &self,
        path_ctx: Option<&PathContext>,
    ) -> Result<std::path::PathBuf, BackupFileError> {
        let current_device_id = get_current_device_id();
        let unit_path_str = self
            .get_path_for_device(current_device_id)
            .ok_or(BackupFileError::NonePathError)?;
        Ok(crate::path_resolver::resolve_path_explicit(
            unit_path_str,
            path_ctx,
        )?)
    }
}

impl SaveUnitDraft {
    pub fn concrete(
        id: Option<u32>,
        unit_type: SaveUnitType,
        paths: HashMap<DeviceId, String>,
        delete_before_apply: bool,
        enabled: bool,
    ) -> Self {
        Self {
            id,
            source: SaveUnitSource::Concrete { unit_type, paths },
            delete_before_apply,
            enabled,
        }
    }

    pub fn paths_mut(&mut self) -> Option<&mut HashMap<DeviceId, String>> {
        match &mut self.source {
            SaveUnitSource::Concrete { paths, .. } => Some(paths),
            SaveUnitSource::ManifestPattern { .. } => None,
        }
    }

    pub fn paths(&self) -> Option<&HashMap<DeviceId, String>> {
        match &self.source {
            SaveUnitSource::Concrete { paths, .. } => Some(paths),
            SaveUnitSource::ManifestPattern { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_concrete_shape_deserializes_and_serializes_as_typed_source() {
        let legacy = serde_json::json!({
            "id": 3,
            "unit_type": "Folder",
            "paths": { "device": "C:/Saves" },
            "delete_before_apply": true,
            "enabled": true
        });

        let unit: SaveUnit = serde_json::from_value(legacy).unwrap();
        assert_eq!(unit.unit_type(), Some(&SaveUnitType::Folder));
        assert_eq!(
            unit.get_path_for_device(&"device".to_string())
                .map(String::as_str),
            Some("C:/Saves")
        );

        let serialized = serde_json::to_value(unit).unwrap();
        assert_eq!(
            serialized
                .pointer("/source/type")
                .and_then(serde_json::Value::as_str),
            Some("concrete")
        );
        assert!(serialized.get("unit_type").is_none());
        assert!(serialized.get("paths").is_none());
    }

    #[test]
    fn manifest_pattern_preserves_declared_type_without_concrete_paths() {
        let unit = SaveUnit {
            id: 1,
            source: SaveUnitSource::ManifestPattern {
                expected_type: Some(SaveUnitType::File),
                pattern: ManifestPathPattern::new("<home>/*.sav"),
                constraints: ManifestPathConstraints::default(),
            },
            delete_before_apply: false,
            enabled: true,
        };

        assert_eq!(unit.unit_type(), Some(&SaveUnitType::File));
        assert!(unit.paths().is_none());
        assert_eq!(unit.manifest_pattern().unwrap().0.raw(), "<home>/*.sav");

        let serialized = serde_json::to_value(&unit).unwrap();
        assert_eq!(
            serialized.pointer("/source/expected_type"),
            Some(&serde_json::json!("File"))
        );

        let round_trip: SaveUnit = serde_json::from_value(serialized).unwrap();
        assert_eq!(round_trip.unit_type(), Some(&SaveUnitType::File));
    }

    #[test]
    fn nested_source_fields_match_generated_binding_names() {
        let source = SaveUnitSource::Concrete {
            unit_type: SaveUnitType::File,
            paths: HashMap::new(),
        };

        let serialized = serde_json::to_value(source).unwrap();
        assert_eq!(
            serialized.get("unit_type"),
            Some(&serde_json::json!("File"))
        );
        assert!(serde_json::from_value::<SaveUnitSource>(serialized).is_ok());
        assert!(
            serde_json::from_value::<SaveUnitSource>(serde_json::json!({
                "type": "concrete",
                "unitType": "File",
                "paths": {}
            }))
            .is_ok()
        );
    }
}
