//! Windows Registry export and import for `SaveUnitType::WinRegistry`.
//!
//! Registry data is serialized as JSON (`RegistryData`) and stored inside
//! the ZIP archive at `{save_unit_id}/registry.json`. The JSON format uses
//! a `format_version` field for future-proofing.
//!
//! On non-Windows platforms, export/import return `RegistryError::UnsupportedPlatform`.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Root structure for serialized registry data inside an archive.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegistryData {
    pub format_version: u32,
    /// Full registry path including hive, e.g. `HKEY_CURRENT_USER\SOFTWARE\GameName`.
    pub root_key: String,
    /// Flattened list of key entries with values.
    pub entries: Vec<RegistryKeyEntry>,
}

/// A single registry key and its values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegistryKeyEntry {
    /// Subkey path relative to `root_key`. Empty string means the root key itself.
    pub subkey: String,
    pub values: Vec<RegistryValue>,
}

/// A typed registry value.
///
/// The `name` field is the value name; an empty string represents the default value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegistryValue {
    Sz { name: String, data: String },
    ExpandSz { name: String, data: String },
    MultiSz { name: String, data: Vec<String> },
    Dword { name: String, data: u32 },
    Qword { name: String, data: u64 },
    Binary { name: String, data: String },
}

/// Name of the registry data file inside a save-unit's ZIP directory.
pub const REGISTRY_DATA_FILENAME: &str = "registry.json";

// ── Error type ──────────────────────────────────────────────────────────────

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("Unsupported registry hive: {0}")]
    UnsupportedHive(String),
    #[error("Registry operations are not supported on this platform")]
    UnsupportedPlatform,
    #[error("Registry I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[cfg(target_os = "windows")]
    #[error("Registry access error: {0}")]
    WinReg(String),
}

// ── Windows implementation ──────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use winreg::RegKey;
    use winreg::enums::*;

    /// Map a hive name string to a Windows HKEY constant.
    pub(crate) fn parse_hive(hive_str: &str) -> Result<winreg::HKEY, RegistryError> {
        match hive_str {
            "HKEY_CURRENT_USER" => Ok(HKEY_CURRENT_USER),
            "HKEY_LOCAL_MACHINE" => Ok(HKEY_LOCAL_MACHINE),
            "HKEY_CLASSES_ROOT" => Ok(HKEY_CLASSES_ROOT),
            "HKEY_USERS" => Ok(HKEY_USERS),
            "HKEY_CURRENT_CONFIG" => Ok(HKEY_CURRENT_CONFIG),
            _ => Err(RegistryError::UnsupportedHive(hive_str.to_string())),
        }
    }

    /// Split `"HKEY_CURRENT_USER\SOFTWARE\Game"` into `(HKEY, "SOFTWARE\\Game")`.
    ///
    /// Also handles the Ludusavi `"REGISTRY:"` prefix format and normalizes `/` to `\`.
    pub(crate) fn parse_registry_path(path: &str) -> Result<(winreg::HKEY, String), RegistryError> {
        // Strip optional Ludusavi "REGISTRY:" prefix
        let path = path.strip_prefix("REGISTRY:").unwrap_or(path);
        let path = path.trim_start_matches(['/', '\\']);
        let path = path.replace('/', "\\");

        let sep = path
            .find('\\')
            .ok_or_else(|| RegistryError::UnsupportedHive(path.to_string()))?;
        let (hive_str, rest) = path.split_at(sep);
        let subkey = rest.trim_start_matches('\\').to_string();
        let hive = parse_hive(hive_str)?;
        Ok((hive, subkey))
    }

    /// Read a single registry value and convert it to `RegistryValue`.
    fn read_value(key: &RegKey, name: &str) -> Result<RegistryValue, RegistryError> {
        let value = key
            .get_raw_value(name)
            .map_err(|e| RegistryError::WinReg(e.to_string()))?;

        let val_name = name.to_string();
        match value.vtype {
            REG_SZ => {
                let data = String::from_utf16_lossy(
                    &value
                        .bytes
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect::<Vec<_>>(),
                )
                .trim_end_matches('\0')
                .to_string();
                Ok(RegistryValue::Sz {
                    name: val_name,
                    data,
                })
            }
            REG_EXPAND_SZ => {
                let data = String::from_utf16_lossy(
                    &value
                        .bytes
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect::<Vec<_>>(),
                )
                .trim_end_matches('\0')
                .to_string();
                Ok(RegistryValue::ExpandSz {
                    name: val_name,
                    data,
                })
            }
            REG_MULTI_SZ => {
                let wide: Vec<u16> = value
                    .bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let full = String::from_utf16_lossy(&wide);
                let data: Vec<String> = full
                    .trim_end_matches('\0')
                    .split('\0')
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect();
                Ok(RegistryValue::MultiSz {
                    name: val_name,
                    data,
                })
            }
            REG_DWORD => {
                let data = if value.bytes.len() >= 4 {
                    u32::from_le_bytes([
                        value.bytes[0],
                        value.bytes[1],
                        value.bytes[2],
                        value.bytes[3],
                    ])
                } else {
                    0
                };
                Ok(RegistryValue::Dword {
                    name: val_name,
                    data,
                })
            }
            REG_QWORD => {
                let data = if value.bytes.len() >= 8 {
                    u64::from_le_bytes([
                        value.bytes[0],
                        value.bytes[1],
                        value.bytes[2],
                        value.bytes[3],
                        value.bytes[4],
                        value.bytes[5],
                        value.bytes[6],
                        value.bytes[7],
                    ])
                } else {
                    0
                };
                Ok(RegistryValue::Qword {
                    name: val_name,
                    data,
                })
            }
            _ => {
                // REG_BINARY and all other types → store as base64
                let data = BASE64.encode(&value.bytes);
                Ok(RegistryValue::Binary {
                    name: val_name,
                    data,
                })
            }
        }
    }

    /// Write a single `RegistryValue` back to the registry.
    fn write_value(key: &RegKey, value: &RegistryValue) -> Result<(), RegistryError> {
        use winreg::RegValue;

        match value {
            RegistryValue::Sz { name, data } => {
                let wide = to_reg_sz_bytes(data);
                key.set_raw_value(
                    name,
                    &RegValue {
                        vtype: REG_SZ,
                        bytes: wide,
                    },
                )
                .map_err(|e| RegistryError::WinReg(e.to_string()))
            }
            RegistryValue::ExpandSz { name, data } => {
                let wide = to_reg_sz_bytes(data);
                key.set_raw_value(
                    name,
                    &RegValue {
                        vtype: REG_EXPAND_SZ,
                        bytes: wide,
                    },
                )
                .map_err(|e| RegistryError::WinReg(e.to_string()))
            }
            RegistryValue::MultiSz { name, data } => {
                let mut wide_bytes = Vec::new();
                for s in data {
                    let wide: Vec<u16> = s.encode_utf16().collect();
                    for w in &wide {
                        wide_bytes.extend_from_slice(&w.to_le_bytes());
                    }
                    // null terminator between strings
                    wide_bytes.extend_from_slice(&0u16.to_le_bytes());
                }
                // final double null terminator
                wide_bytes.extend_from_slice(&0u16.to_le_bytes());
                key.set_raw_value(
                    name,
                    &RegValue {
                        vtype: REG_MULTI_SZ,
                        bytes: wide_bytes,
                    },
                )
                .map_err(|e| RegistryError::WinReg(e.to_string()))
            }
            RegistryValue::Dword { name, data } => key
                .set_value(name, data)
                .map_err(|e| RegistryError::WinReg(e.to_string())),
            RegistryValue::Qword { name, data } => key
                .set_value(name, data)
                .map_err(|e| RegistryError::WinReg(e.to_string())),
            RegistryValue::Binary { name, data } => {
                let bytes = BASE64.decode(data)?;
                key.set_raw_value(
                    name,
                    &RegValue {
                        vtype: REG_BINARY,
                        bytes,
                    },
                )
                .map_err(|e| RegistryError::WinReg(e.to_string()))
            }
        }
    }

    /// Encode a Rust string as UTF-16LE bytes with null terminator (REG_SZ/REG_EXPAND_SZ format).
    fn to_reg_sz_bytes(s: &str) -> Vec<u8> {
        let mut wide: Vec<u16> = s.encode_utf16().collect();
        wide.push(0); // null terminator
        wide.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// Recursively collect all values and subkeys under `key`.
    fn collect_key_tree(
        key: &RegKey,
        rel_subkey: &str,
        entries: &mut Vec<RegistryKeyEntry>,
    ) -> Result<(), RegistryError> {
        // Collect values for this key
        let mut values = Vec::new();
        for name_result in key.enum_values() {
            let (name, _) = name_result.map_err(|e| RegistryError::WinReg(e.to_string()))?;
            match read_value(key, &name) {
                Ok(val) => values.push(val),
                Err(e) => {
                    log::warn!(target: "rgsm::backup::registry", "Skipping value '{name}': {e}");
                }
            }
        }

        entries.push(RegistryKeyEntry {
            subkey: rel_subkey.to_string(),
            values,
        });

        // Recurse into subkeys
        for subkey_result in key.enum_keys() {
            let subkey_name = subkey_result.map_err(|e| RegistryError::WinReg(e.to_string()))?;
            let child = key
                .open_subkey(&subkey_name)
                .map_err(|e| RegistryError::WinReg(e.to_string()))?;
            let child_rel = if rel_subkey.is_empty() {
                subkey_name.clone()
            } else {
                format!("{rel_subkey}\\{subkey_name}")
            };
            collect_key_tree(&child, &child_rel, entries)?;
        }

        Ok(())
    }

    /// Export a registry key tree to `RegistryData`.
    pub fn export_registry_key(path: &str) -> Result<RegistryData, RegistryError> {
        let (hive, subkey) = parse_registry_path(path)?;
        let root = RegKey::predef(hive);
        let key = root
            .open_subkey(&subkey)
            .map_err(|e| RegistryError::WinReg(e.to_string()))?;

        let mut entries = Vec::new();
        collect_key_tree(&key, "", &mut entries)?;

        Ok(RegistryData {
            format_version: 1,
            root_key: path.to_string(),
            entries,
        })
    }

    /// Import `RegistryData` back into the Windows Registry.
    pub fn import_registry_data(data: &RegistryData) -> Result<(), RegistryError> {
        let (hive, base_subkey) = parse_registry_path(&data.root_key)?;
        let root = RegKey::predef(hive);

        for entry in &data.entries {
            let full_subkey = if entry.subkey.is_empty() {
                base_subkey.to_string()
            } else {
                format!("{base_subkey}\\{}", entry.subkey)
            };
            let (key, _) = root
                .create_subkey(&full_subkey)
                .map_err(|e| RegistryError::WinReg(e.to_string()))?;
            for value in &entry.values {
                write_value(&key, value)?;
            }
        }

        Ok(())
    }
}

// ── Non-Windows stubs ───────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::*;

    pub fn export_registry_key(_path: &str) -> Result<RegistryData, RegistryError> {
        Err(RegistryError::UnsupportedPlatform)
    }

    pub fn import_registry_data(_data: &RegistryData) -> Result<(), RegistryError> {
        Err(RegistryError::UnsupportedPlatform)
    }
}

// Re-export platform functions at module level.
pub use platform::{export_registry_key, import_registry_data};

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_data_json_roundtrip() {
        let data = RegistryData {
            format_version: 1,
            root_key: "HKEY_CURRENT_USER\\SOFTWARE\\TestGame".to_string(),
            entries: vec![
                RegistryKeyEntry {
                    subkey: String::new(),
                    values: vec![
                        RegistryValue::Sz {
                            name: String::new(),
                            data: "default".to_string(),
                        },
                        RegistryValue::Dword {
                            name: "Score".to_string(),
                            data: 42,
                        },
                        RegistryValue::Qword {
                            name: "BigNum".to_string(),
                            data: 9_999_999_999,
                        },
                        RegistryValue::Binary {
                            name: "Blob".to_string(),
                            data: BASE64.encode(b"\x00\x01\x02\xff"),
                        },
                        RegistryValue::MultiSz {
                            name: "Tags".to_string(),
                            data: vec!["alpha".into(), "beta".into()],
                        },
                        RegistryValue::ExpandSz {
                            name: "Path".to_string(),
                            data: "%USERPROFILE%\\Saves".to_string(),
                        },
                    ],
                },
                RegistryKeyEntry {
                    subkey: "Settings\\Video".to_string(),
                    values: vec![RegistryValue::Sz {
                        name: "Resolution".to_string(),
                        data: "1920x1080".to_string(),
                    }],
                },
            ],
        };

        let json = serde_json::to_string_pretty(&data).unwrap();
        let parsed: RegistryData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, parsed);
    }

    #[test]
    fn registry_data_empty_key() {
        let data = RegistryData {
            format_version: 1,
            root_key: "HKEY_CURRENT_USER\\SOFTWARE\\EmptyGame".to_string(),
            entries: vec![RegistryKeyEntry {
                subkey: String::new(),
                values: vec![],
            }],
        };

        let json = serde_json::to_string(&data).unwrap();
        let parsed: RegistryData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, parsed);
    }

    #[cfg(target_os = "windows")]
    mod windows_tests {
        use super::*;
        use winreg::RegKey;
        use winreg::enums::*;

        const TEST_KEY: &str = "HKEY_CURRENT_USER\\Software\\RGSM_TEST";

        fn cleanup_test_key() {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let _ = hkcu.delete_subkey_all("Software\\RGSM_TEST");
        }

        fn setup_test_key() {
            cleanup_test_key();
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let (key, _) = hkcu.create_subkey("Software\\RGSM_TEST").unwrap();
            key.set_value("TestString", &"hello").unwrap();
            key.set_value("TestDword", &42u32).unwrap();
            let (child, _) = hkcu.create_subkey("Software\\RGSM_TEST\\Child").unwrap();
            child.set_value("ChildVal", &"world").unwrap();
        }

        #[test]
        fn export_and_import_roundtrip() {
            setup_test_key();

            // Export
            let data = export_registry_key(TEST_KEY).unwrap();
            assert_eq!(data.format_version, 1);
            assert!(!data.entries.is_empty());

            // Verify root key has expected values
            let root_entry = data.entries.iter().find(|e| e.subkey.is_empty()).unwrap();
            assert!(root_entry.values.iter().any(|v| matches!(v, RegistryValue::Sz { name, data } if name == "TestString" && data == "hello")));
            assert!(root_entry.values.iter().any(|v| matches!(v, RegistryValue::Dword { name, data } if name == "TestDword" && *data == 42)));

            // Verify child key
            let child_entry = data.entries.iter().find(|e| e.subkey == "Child").unwrap();
            assert!(child_entry.values.iter().any(|v| matches!(v, RegistryValue::Sz { name, data } if name == "ChildVal" && data == "world")));

            // Clean and re-import
            cleanup_test_key();
            import_registry_data(&data).unwrap();

            // Verify re-imported data matches
            let reimported = export_registry_key(TEST_KEY).unwrap();
            assert_eq!(data.entries.len(), reimported.entries.len());

            cleanup_test_key();
        }

        #[test]
        fn export_nonexistent_key_errors() {
            let result =
                export_registry_key("HKEY_CURRENT_USER\\Software\\RGSM_NONEXISTENT_KEY_12345");
            assert!(result.is_err());
        }

        #[test]
        fn parse_ludusavi_registry_prefix() {
            let (_, subkey) =
                platform::parse_registry_path("REGISTRY:HKEY_CURRENT_USER/Software/Game").unwrap();
            assert_eq!(subkey, "Software\\Game");
        }

        #[test]
        fn export_ludusavi_style_path_uses_normalized_subkey() {
            setup_test_key();
            let result = export_registry_key("REGISTRY:HKEY_CURRENT_USER/Software/RGSM_TEST");
            assert!(result.is_ok());
            cleanup_test_key();
        }
    }
}
