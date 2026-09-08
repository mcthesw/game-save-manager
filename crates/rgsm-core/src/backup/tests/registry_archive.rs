#[cfg(target_os = "windows")]
mod tests {
    use super::super::utils::{ConfigFileGuard, lock_config_file};
    use crate::backup::archive::{
        ArchiveMeta, CompressionPreset, compress_to_file, decompress_from_file,
    };
    use crate::backup::{SaveUnit, SaveUnitType};
    use crate::device::get_current_device_id;
    use std::{
        collections::HashMap,
        fs::{self, File},
        io::{Read, Write},
        time::SystemTime,
    };
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn build_registry_save_unit_with_id(path: &str, id: u32) -> SaveUnit {
        let mut paths = HashMap::new();
        paths.insert(get_current_device_id().clone(), path.to_string());
        SaveUnit::concrete(id, SaveUnitType::WinRegistry, paths, false, true)
    }

    fn unique_registry_path(prefix: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        let subkey = format!("Software\\{prefix}_{unique}");
        let path = format!("HKEY_CURRENT_USER\\{subkey}");
        Ok((subkey, path))
    }

    #[test]
    fn registry_restore_uses_device_target_not_archived_root()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::backup::registry::{
            RegistryData, RegistryKeyEntry, RegistryValue, serialize_reg_file,
        };

        let _config_lock = lock_config_file();
        let _config_guard = ConfigFileGuard::write_default_config()?;
        let temp = temp_dir::TempDir::new()?;
        let (owned_subkey, owned_path) = unique_registry_path("RGSM_TEST")?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let source_path = format!("{owned_path}\\OldName");
            let target_path = format!("{owned_path}\\NewName");
            let (source, _) = hkcu.create_subkey(format!("{owned_subkey}\\OldName"))?;
            source.set_value("Untouched", &"original")?;
            let data = RegistryData {
                format_version: 1,
                root_key: source_path,
                entries: vec![
                    RegistryKeyEntry {
                        subkey: String::new(),
                        values: vec![],
                    },
                    RegistryKeyEntry {
                        subkey: "Nested".into(),
                        values: vec![RegistryValue::Sz {
                            name: "Value".into(),
                            data: "S-1-5-21-old-user-must-stay".into(),
                        }],
                    },
                ],
            };
            for (date, filename, bytes) in [
                ("reg", "registry.reg", serialize_reg_file(&data)?),
                ("json", "registry.json", serde_json::to_vec(&data)?),
            ] {
                let mut writer =
                    ZipWriter::new(File::create(temp.path().join(format!("{date}.zip")))?);
                writer.start_file(format!("0/{filename}"), SimpleFileOptions::default())?;
                writer.write_all(&bytes)?;
                writer.set_comment(ArchiveMeta::new(CompressionPreset::Standard).to_comment());
                writer.finish()?;
                decompress_from_file(
                    &[build_registry_save_unit_with_id(&target_path, 0)],
                    temp.path(),
                    date,
                    None,
                )?;
                let restored = hkcu.open_subkey(format!("{owned_subkey}\\NewName\\Nested"))?;
                assert_eq!(
                    restored.get_value::<String, _>("Value")?,
                    "S-1-5-21-old-user-must-stay"
                );
                assert!(source.open_subkey("Nested").is_err());
                assert_eq!(source.get_value::<String, _>("Untouched")?, "original");
                hkcu.delete_subkey_all(format!("{owned_subkey}\\NewName"))?;
            }
            Ok(())
        })();
        let _ = hkcu.delete_subkey_all(&owned_subkey);
        result
    }

    #[test]
    fn capture_archives_restore_registry_to_each_mapped_target()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::backup::archive::{ArchiveBackend, SevenZBackend, ZipBackend};
        use crate::backup::{
            CaptureGroup, CapturePlan, CaptureSourceKind, RestoreEntry, RestorePlan,
        };
        let temp = temp_dir::TempDir::new()?;
        let (owned_subkey, owned_path) = unique_registry_path("RGSM_TEST")?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let (source, _) = hkcu.create_subkey(format!("{owned_subkey}\\OldName\\Nested"))?;
            let capture = CapturePlan {
                groups: vec![CaptureGroup {
                    id: 0,
                    save_unit_id: 0,
                    candidate_id: "source".into(),
                    dimensions: Default::default(),
                    logical_anchor: owned_path.clone().into(),
                    source_path: format!("{owned_path}\\OldName"),
                    relative_path: "OldName".into(),
                    archive_path: "0/0/data/registry.reg".into(),
                    kind: CaptureSourceKind::Registry,
                    delete_before_apply: false,
                }],
            };
            for backend in [&ZipBackend as &dyn ArchiveBackend, &SevenZBackend] {
                source.set_value("Value", &"archived")?;
                let archive = temp.path().join(format!("capture.{}", backend.extension()));
                backend.compress_capture_plan(
                    &capture,
                    &archive,
                    CompressionPreset::Standard,
                    None,
                )?;
                source.set_value("Value", &"live-unchanged")?;
                let targets = ["FirstTarget", "SecondTarget"];
                let restore = RestorePlan {
                    entries: targets
                        .iter()
                        .map(|name| RestoreEntry {
                            save_unit_id: 0,
                            group_id: 0,
                            archive_path: "0/0/data/registry.reg".into(),
                            target_path: format!("{owned_path}\\{name}").into(),
                            kind: CaptureSourceKind::Registry,
                            delete_before_apply: false,
                        })
                        .collect(),
                    ..Default::default()
                };
                backend.restore_capture_plan(&restore, &archive)?;
                for name in targets {
                    let restored = hkcu.open_subkey(format!("{owned_subkey}\\{name}\\Nested"))?;
                    assert_eq!(restored.get_value::<String, _>("Value")?, "archived");
                    hkcu.delete_subkey_all(format!("{owned_subkey}\\{name}"))?;
                }
                assert_eq!(source.get_value::<String, _>("Value")?, "live-unchanged");
            }
            Ok(())
        })();
        let _ = hkcu.delete_subkey_all(&owned_subkey);
        result
    }

    #[test]
    fn registry_snapshot_writes_reg_file() -> Result<(), Box<dyn std::error::Error>> {
        use crate::backup::registry;

        let _config_lock = lock_config_file();
        let _config_guard = ConfigFileGuard::write_default_config()?;

        let temp_dir = temp_dir::TempDir::new()?;
        let temp_path = temp_dir.path();
        let backup_dir = temp_path.join("backup");
        fs::create_dir_all(&backup_dir)?;
        let zip_path = backup_dir.join("registry_reg_entry.zip");

        let (reg_subkey, reg_path) = unique_registry_path("RGSM_REG_ENTRY")?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let _ = hkcu.delete_subkey_all(&reg_subkey);

        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let (key, _) = hkcu.create_subkey(&reg_subkey)?;
            key.set_value("ManualRestore", &"standard-reg-file")?;

            let save_units = [build_registry_save_unit_with_id(&reg_path, 0)];
            compress_to_file(&save_units, &zip_path, CompressionPreset::Standard, None)?;

            let zip_file = File::open(&zip_path)?;
            let mut archive = zip::ZipArchive::new(zip_file)?;
            assert!(archive.by_name("0/registry.reg").is_ok());
            assert!(archive.by_name("0/registry.json").is_err());

            let mut reg_entry = archive.by_name("0/registry.reg")?;
            let mut reg_bytes = Vec::new();
            reg_entry.read_to_end(&mut reg_bytes)?;
            let parsed = registry::deserialize_reg_file(&reg_bytes)?;
            assert_eq!(parsed.root_key, reg_path);
            assert!(
                parsed.entries[0].values.iter().any(
                    |value| matches!(value, registry::RegistryValue::Sz { name, data } if name == "ManualRestore" && data == "standard-reg-file")
                )
            );
            Ok(())
        })();

        let _ = hkcu.delete_subkey_all(&reg_subkey);
        result
    }

    #[test]
    fn registry_restore_reads_legacy_json_entry() -> Result<(), Box<dyn std::error::Error>> {
        use crate::backup::registry::{RegistryData, RegistryKeyEntry, RegistryValue};

        let _config_lock = lock_config_file();
        let _config_guard = ConfigFileGuard::write_default_config()?;

        let temp_dir = temp_dir::TempDir::new()?;
        let temp_path = temp_dir.path();
        let backup_dir = temp_path.join("backup");
        fs::create_dir_all(&backup_dir)?;
        let date = "legacy_registry_json";
        let zip_path = backup_dir.join(format!("{date}.zip"));

        let (reg_subkey, reg_path) = unique_registry_path("RGSM_LEGACY_JSON")?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let _ = hkcu.delete_subkey_all(&reg_subkey);

        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let legacy_data = RegistryData {
                format_version: 1,
                root_key: reg_path.clone(),
                entries: vec![RegistryKeyEntry {
                    subkey: String::new(),
                    values: vec![RegistryValue::Sz {
                        name: "Restored".to_string(),
                        data: "from-json".to_string(),
                    }],
                }],
            };

            let mut zip_writer = ZipWriter::new(File::create(&zip_path)?);
            zip_writer.start_file(
                "0/registry.json",
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Bzip2),
            )?;
            zip_writer.write_all(&serde_json::to_vec_pretty(&legacy_data)?)?;
            zip_writer.set_comment(ArchiveMeta::new(CompressionPreset::Standard).to_comment());
            zip_writer.finish()?;

            let save_units = [build_registry_save_unit_with_id(&reg_path, 0)];
            decompress_from_file(&save_units, &backup_dir, date, None)?;

            let restored_key = hkcu.open_subkey(&reg_subkey)?;
            let restored: String = restored_key.get_value("Restored")?;
            assert_eq!(restored, "from-json");
            Ok(())
        })();

        let _ = hkcu.delete_subkey_all(&reg_subkey);
        result
    }

    #[test]
    fn registry_restore_prefers_reg_entry() -> Result<(), Box<dyn std::error::Error>> {
        use crate::backup::registry::{
            RegistryData, RegistryKeyEntry, RegistryValue, serialize_reg_file,
        };

        let _config_lock = lock_config_file();
        let _config_guard = ConfigFileGuard::write_default_config()?;

        let temp_dir = temp_dir::TempDir::new()?;
        let temp_path = temp_dir.path();
        let backup_dir = temp_path.join("backup");
        fs::create_dir_all(&backup_dir)?;
        let date = "prefer_registry_reg";
        let zip_path = backup_dir.join(format!("{date}.zip"));

        let (reg_subkey, reg_path) = unique_registry_path("RGSM_PREFER_REG")?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let _ = hkcu.delete_subkey_all(&reg_subkey);

        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let reg_data = RegistryData {
                format_version: 1,
                root_key: reg_path.clone(),
                entries: vec![RegistryKeyEntry {
                    subkey: String::new(),
                    values: vec![RegistryValue::Sz {
                        name: "Restored".to_string(),
                        data: "from-reg".to_string(),
                    }],
                }],
            };
            let legacy_data = RegistryData {
                format_version: 1,
                root_key: reg_path.clone(),
                entries: vec![RegistryKeyEntry {
                    subkey: String::new(),
                    values: vec![RegistryValue::Sz {
                        name: "Restored".to_string(),
                        data: "from-json".to_string(),
                    }],
                }],
            };

            let mut zip_writer = ZipWriter::new(File::create(&zip_path)?);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Bzip2);
            zip_writer.start_file("0/registry.reg", options)?;
            zip_writer.write_all(&serialize_reg_file(&reg_data)?)?;
            zip_writer.start_file("0/registry.json", options)?;
            zip_writer.write_all(&serde_json::to_vec_pretty(&legacy_data)?)?;
            zip_writer.set_comment(ArchiveMeta::new(CompressionPreset::Standard).to_comment());
            zip_writer.finish()?;

            let save_units = [build_registry_save_unit_with_id(&reg_path, 0)];
            decompress_from_file(&save_units, &backup_dir, date, None)?;

            let restored_key = hkcu.open_subkey(&reg_subkey)?;
            let restored: String = restored_key.get_value("Restored")?;
            assert_eq!(restored, "from-reg");
            Ok(())
        })();

        let _ = hkcu.delete_subkey_all(&reg_subkey);
        result
    }
}
