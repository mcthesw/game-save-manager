use std::{collections::BTreeMap, hash::Hasher, path::Path};

use super::{
    ArchiveManifest,
    migration::ArchiveMigrationError,
    payload::{ArchivePayload, PayloadSource},
    portable::{MANIFEST_ENTRY, RECOVERY_ENTRY},
};

pub(super) fn verify_payloads(
    payloads: &[ArchivePayload],
    archive: &Path,
    manifest: &ArchiveManifest,
) -> Result<(), ArchiveMigrationError> {
    let mut expected = BTreeMap::new();
    for payload in payloads {
        match &payload.source {
            PayloadSource::Bytes(bytes) => {
                expected.insert(
                    payload.name.clone(),
                    Some(format!("{:016x}", xxhash_rust::xxh3::xxh3_64(bytes))),
                );
            }
            PayloadSource::FileSystem(path) => {
                for entry in walkdir::WalkDir::new(path) {
                    let entry =
                        entry.map_err(|error| ArchiveMigrationError::Invalid(error.to_string()))?;
                    let relative = entry
                        .path()
                        .strip_prefix(path)
                        .expect("payload child")
                        .to_string_lossy()
                        .replace('\\', "/");
                    let name = if relative.is_empty() {
                        payload.name.clone()
                    } else {
                        format!("{}/{relative}", payload.name)
                    };
                    let hash = if entry.file_type().is_dir() {
                        None
                    } else {
                        Some(crate::backup::compute_file_hash(entry.path())?)
                    };
                    if expected.insert(name, hash).is_some() {
                        return Err(ArchiveMigrationError::Invalid(
                            "overlapping archive payloads".into(),
                        ));
                    }
                }
            }
        }
    }
    let mut reader = sevenz_rust2::ArchiveReader::open(archive, sevenz_rust2::Password::empty())
        .map_err(invalid)?;
    reader
        .for_each_entries(|entry, input| {
            if entry.name() == MANIFEST_ENTRY || entry.name() == RECOVERY_ENTRY {
                return Ok(true);
            }
            let Some(hash) = expected.remove(entry.name()) else {
                return Err(sevenz_error("unexpected archive entry"));
            };
            if entry.is_directory != hash.is_none() {
                return Err(sevenz_error("archive entry type changed"));
            }
            if let Some(expected) = hash {
                let mut hasher = xxhash_rust::xxh3::Xxh3::new();
                let mut buffer = [0_u8; 64 * 1024];
                loop {
                    let read = input.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    hasher.write(&buffer[..read]);
                }
                if format!("{:016x}", hasher.finish()) != expected {
                    return Err(sevenz_error("archive content changed"));
                }
            }
            Ok(true)
        })
        .map_err(invalid)?;
    if !expected.is_empty() || super::seven_z::read_manifest(archive)? != *manifest {
        return Err(ArchiveMigrationError::Invalid(
            "converted archive verification failed".into(),
        ));
    }
    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> ArchiveMigrationError {
    ArchiveMigrationError::Invalid(error.to_string())
}

fn sevenz_error(message: impl Into<String>) -> sevenz_rust2::Error {
    std::io::Error::other(message.into()).into()
}
