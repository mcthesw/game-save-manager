use std::{fs::File, io::Cursor, path::PathBuf};

use sevenz_rust2::{ArchiveEntry, ArchiveWriter};

use crate::{
    backup::{CapturePlan, CaptureSourceKind},
    preclude::{BackupFileError, CompressError},
};

pub(super) struct ArchivePayload {
    pub name: String,
    pub source: PayloadSource,
}

pub(super) enum PayloadSource {
    FileSystem(PathBuf),
    Bytes(Vec<u8>),
}

pub(super) fn capture_payloads(plan: &CapturePlan) -> Result<Vec<ArchivePayload>, CompressError> {
    plan.groups
        .iter()
        .map(|group| {
            let source = if group.kind == CaptureSourceKind::Registry {
                let data = crate::backup::registry::export_registry_key(&group.source_path)
                    .map_err(|error| {
                        CompressError::Single(BackupFileError::RegistryError(error.to_string()))
                    })?;
                let bytes =
                    crate::backup::registry::serialize_reg_file(&data).map_err(|error| {
                        CompressError::Single(BackupFileError::RegistryError(error.to_string()))
                    })?;
                PayloadSource::Bytes(bytes)
            } else {
                PayloadSource::FileSystem(group.source_path.clone().into())
            };
            Ok(ArchivePayload {
                name: group.archive_path.clone(),
                source,
            })
        })
        .collect()
}

pub(super) fn append_payload(
    writer: &mut ArchiveWriter<File>,
    payload: &ArchivePayload,
) -> Result<(), CompressError> {
    match &payload.source {
        PayloadSource::FileSystem(source) => {
            super::seven_z::append_path(writer, source, &payload.name)
        }
        PayloadSource::Bytes(bytes) => writer
            .push_archive_entry(
                ArchiveEntry::new_file(&payload.name),
                Some(Cursor::new(bytes)),
            )
            .map(|_| ())
            .map_err(|error| CompressError::Unexpected(error.into())),
    }
}
