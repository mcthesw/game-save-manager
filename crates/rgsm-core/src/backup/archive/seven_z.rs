use std::{
    fs::{self, File},
    io::{Cursor, Write},
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

use sevenz_rust2::{
    ArchiveEntry, ArchiveReader, ArchiveWriter, EncoderMethod, NtTime, Password,
    encoder_options::DeflateOptions,
};

use crate::{
    backup::{CaptureGroup, CapturePlan, CaptureSourceKind, CompressionPreset, RestorePlan},
    preclude::{BackupFileError, CompressError},
};

use super::{ArchiveManifestV4, V4_MANIFEST_ENTRY, restored_directory::remove_restored_directory};

const UNIX_EXTENSION: u32 = 0x8000;
#[cfg(unix)]
const POSIX_MODE_MASK: u32 = 0o7777;

#[derive(Clone)]
struct EntryMetadata {
    created: Option<SystemTime>,
    accessed: Option<SystemTime>,
    modified: Option<SystemTime>,
    attributes: Option<u32>,
}

struct DeferredDirectory {
    path: PathBuf,
    entry: ArchiveEntry,
}

pub(super) fn compress_capture_plan(
    plan: &CapturePlan,
    archive_path: &Path,
    preset: CompressionPreset,
    source_fingerprint: Option<String>,
) -> Result<u64, CompressError> {
    let temp_path = archive_path.with_extension("7z.capture.tmp");
    let result = write_archive(plan, &temp_path, preset, source_fingerprint);
    let size = match result {
        Ok(size) => size,
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }
    };
    if let Err(error) = fs::rename(&temp_path, archive_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(single(error));
    }
    Ok(size)
}

fn write_archive(
    plan: &CapturePlan,
    path: &Path,
    preset: CompressionPreset,
    source_fingerprint: Option<String>,
) -> Result<u64, CompressError> {
    let mut writer = ArchiveWriter::create(path).map_err(unexpected)?;
    configure_writer(&mut writer, preset);
    for group in &plan.groups {
        append_group(&mut writer, group)?;
    }
    let manifest = ArchiveManifestV4::from_plan(plan, source_fingerprint);
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| CompressError::Unexpected(error.into()))?;
    let mut entry = ArchiveEntry::new_file(V4_MANIFEST_ENTRY);
    set_entry_metadata(&mut entry, &metadata_now())?;
    writer
        .push_archive_entry(entry, Some(Cursor::new(bytes)))
        .map_err(unexpected)?;
    writer.finish().map_err(unexpected)?;
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(single)
}

fn configure_writer(writer: &mut ArchiveWriter<File>, preset: CompressionPreset) {
    match preset {
        CompressionPreset::Store => {
            writer.set_content_methods(vec![EncoderMethod::COPY.into()]);
        }
        CompressionPreset::Fast => {
            writer.set_content_methods(vec![DeflateOptions::from_level(1).into()]);
        }
        CompressionPreset::Standard => {
            writer.set_content_methods(vec![DeflateOptions::from_level(6).into()]);
        }
        CompressionPreset::Best => {
            writer.set_content_methods(vec![DeflateOptions::from_level(9).into()]);
        }
    }
}

fn append_group(
    writer: &mut ArchiveWriter<File>,
    group: &CaptureGroup,
) -> Result<(), CompressError> {
    match group.kind {
        CaptureSourceKind::File => append_path(
            writer,
            Path::new(&group.source_path),
            group.archive_path.trim_end_matches('/'),
        ),
        CaptureSourceKind::Directory => append_path(
            writer,
            Path::new(&group.source_path),
            group.archive_path.trim_end_matches('/'),
        ),
        CaptureSourceKind::Registry => {
            let data = crate::backup::registry::export_registry_key(&group.source_path)
                .map_err(|error| registry_error(error.to_string()))?;
            let bytes = crate::backup::registry::serialize_reg_file(&data)
                .map_err(|error| registry_error(error.to_string()))?;
            let mut entry = ArchiveEntry::new_file(&group.archive_path);
            set_entry_metadata(&mut entry, &metadata_now())?;
            writer
                .push_archive_entry(entry, Some(Cursor::new(bytes)))
                .map(|_| ())
                .map_err(unexpected)
        }
    }
}

fn append_path(
    writer: &mut ArchiveWriter<File>,
    source: &Path,
    archive_name: &str,
) -> Result<(), CompressError> {
    let metadata = fs::symlink_metadata(source).map_err(single)?;
    if is_link_like(&metadata) {
        return Err(CompressError::Unexpected(anyhow::anyhow!(
            "symbolic links are outside Archive V4 scope: {}",
            source.display()
        )));
    }
    let captured = capture_metadata(&metadata);
    let mut entry = ArchiveEntry::from_path(source, archive_name.to_string());
    set_entry_metadata(&mut entry, &captured)?;
    if metadata.is_dir() {
        writer
            .push_archive_entry::<&[u8]>(entry, None)
            .map_err(unexpected)?;
        let mut children = fs::read_dir(source)
            .map_err(single)?
            .map(|child| child.map(|child| child.path()))
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(single)?;
        children.sort();
        for child in children {
            let name = child.file_name().ok_or_else(|| {
                CompressError::Unexpected(anyhow::anyhow!("path has no file name"))
            })?;
            let name = name.to_str().ok_or_else(|| {
                CompressError::Unexpected(anyhow::anyhow!(
                    "Archive V4 does not support non-UTF-8 file names: {}",
                    child.display()
                ))
            })?;
            let child_name = format!("{archive_name}/{name}");
            append_path(writer, &child, &child_name)?;
        }
        apply_metadata(source, &captured).map_err(single)?;
    } else if metadata.is_file() {
        let file = open_without_atime(source).map_err(single)?;
        writer
            .push_archive_entry(entry, Some(file))
            .map_err(unexpected)?;
        apply_metadata(source, &captured).map_err(single)?;
    } else {
        return Err(CompressError::Unexpected(anyhow::anyhow!(
            "unsupported source type: {}",
            source.display()
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn is_link_like(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_like(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

pub(super) fn read_manifest(path: &Path) -> Result<ArchiveManifestV4, CompressError> {
    let file = File::open(path).map_err(single)?;
    let mut reader = ArchiveReader::new(file, Password::empty()).map_err(unexpected)?;
    let mut bytes = None;
    reader
        .for_each_entries(|entry, source| {
            if entry.name() == V4_MANIFEST_ENTRY {
                let mut value = Vec::new();
                source.read_to_end(&mut value)?;
                bytes = Some(value);
            }
            Ok(true)
        })
        .map_err(unexpected)?;
    let bytes = bytes.ok_or_else(|| {
        CompressError::Unexpected(anyhow::anyhow!("Archive V4 manifest is missing"))
    })?;
    let manifest: ArchiveManifestV4 =
        serde_json::from_slice(&bytes).map_err(|error| CompressError::Unexpected(error.into()))?;
    if manifest.version != 4 {
        return Err(CompressError::Unexpected(anyhow::anyhow!(
            "unsupported Archive V4 manifest version: {}",
            manifest.version
        )));
    }
    Ok(manifest)
}

pub(super) fn restore_capture_plan(plan: &RestorePlan, path: &Path) -> Result<(), CompressError> {
    let file = File::open(path).map_err(single)?;
    let mut reader = ArchiveReader::new(file, Password::empty()).map_err(unexpected)?;
    verify_entries(reader.archive().files.iter(), plan)?;

    for planned in &plan.entries {
        if planned.delete_before_apply && planned.target_path.exists() {
            if planned.target_path.is_dir() {
                remove_restored_directory(&planned.target_path).map_err(single)?;
            } else {
                fs::remove_file(&planned.target_path).map_err(single)?;
            }
        }
    }

    let mut directories = Vec::new();
    reader
        .for_each_entries(|entry, source| {
            let matches = matching_plan_entries(entry.name(), plan);
            let Some((first, _)) = matches.first() else {
                return Ok(true);
            };
            if first.kind == CaptureSourceKind::Registry {
                let mut bytes = Vec::new();
                source.read_to_end(&mut bytes)?;
                let data = crate::backup::registry::deserialize_reg_file(&bytes)
                    .map_err(|error| sevenz_io(error.to_string()))?;
                crate::backup::registry::import_registry_data(&data)
                    .map_err(|error| sevenz_io(error.to_string()))?;
                return Ok(true);
            }

            let targets = matches
                .into_iter()
                .map(|(planned, relative)| {
                    relative.map_or_else(
                        || Ok(planned.target_path.clone()),
                        |relative| checked_join(&planned.target_path, &relative),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            if entry.is_directory {
                for target in targets {
                    fs::create_dir_all(&target)?;
                    directories.push(DeferredDirectory {
                        path: target,
                        entry: entry.clone(),
                    });
                }
                return Ok(true);
            }

            let mut outputs = Vec::with_capacity(targets.len());
            for target in targets {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                outputs.push((target.clone(), File::create(target)?));
            }
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = source.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                for (_, output) in &mut outputs {
                    output.write_all(&buffer[..read])?;
                }
            }
            for (target, mut output) in outputs {
                output.flush()?;
                drop(output);
                apply_entry_metadata(&target, entry)?;
            }
            Ok(true)
        })
        .map_err(unexpected)?;

    directories.sort_by_key(|directory| std::cmp::Reverse(directory.path.components().count()));
    for directory in directories {
        apply_entry_metadata(&directory.path, &directory.entry).map_err(unexpected)?;
    }
    Ok(())
}

fn verify_entries<'a>(
    entries: impl Iterator<Item = &'a ArchiveEntry>,
    plan: &RestorePlan,
) -> Result<(), CompressError> {
    let entries = entries.collect::<Vec<_>>();
    let mut unique_names = std::collections::HashSet::new();
    for entry in &entries {
        if !unique_names.insert(entry.name()) {
            return Err(CompressError::Unexpected(anyhow::anyhow!(
                "duplicate Archive V4 entry: {}",
                entry.name()
            )));
        }
    }
    for planned in &plan.entries {
        let base = planned.archive_path.trim_end_matches('/');
        let found = match planned.kind {
            CaptureSourceKind::File | CaptureSourceKind::Registry => entries
                .iter()
                .any(|entry| entry.name() == base && !entry.is_directory),
            CaptureSourceKind::Directory => {
                let prefix = format!("{base}/");
                let mut found = false;
                for entry in &entries {
                    if entry.name() == base {
                        if !entry.is_directory {
                            return Err(CompressError::Unexpected(anyhow::anyhow!(
                                "Archive V4 directory root is a file: {base}"
                            )));
                        }
                        found = true;
                    } else if let Some(relative) = entry.name().strip_prefix(&prefix) {
                        checked_join(Path::new("."), relative).map_err(unexpected)?;
                        found = true;
                    }
                }
                found
            }
        };
        if !found {
            return Err(CompressError::Unexpected(anyhow::anyhow!(
                "capture entry is missing from Archive V4: {base}"
            )));
        }
    }
    Ok(())
}

fn matching_plan_entries<'a>(
    name: &str,
    plan: &'a RestorePlan,
) -> Vec<(&'a crate::backup::RestoreEntry, Option<String>)> {
    plan.entries
        .iter()
        .filter_map(|planned| {
            let base = planned.archive_path.trim_end_matches('/');
            if name == base {
                return Some((planned, None));
            }
            (planned.kind == CaptureSourceKind::Directory)
                .then(|| name.strip_prefix(&format!("{base}/")))
                .flatten()
                .map(|relative| (planned, Some(relative.to_string())))
        })
        .collect()
}

fn checked_join(root: &Path, relative: &str) -> Result<PathBuf, sevenz_rust2::Error> {
    let mut target = root.to_path_buf();
    for component in Path::new(&relative.replace('\\', "/")).components() {
        match component {
            Component::Normal(value) => target.push(value),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(sevenz_io(format!("unsafe archive path: {relative}")));
            }
        }
    }
    Ok(target)
}

fn metadata_now() -> EntryMetadata {
    let now = SystemTime::now();
    EntryMetadata {
        created: Some(now),
        accessed: Some(now),
        modified: Some(now),
        attributes: None,
    }
}

fn capture_metadata(metadata: &fs::Metadata) -> EntryMetadata {
    EntryMetadata {
        created: metadata.created().ok(),
        accessed: metadata.accessed().ok(),
        modified: metadata.modified().ok(),
        attributes: capture_attributes(metadata),
    }
}

#[cfg(unix)]
fn capture_attributes(metadata: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    Some((metadata.mode() << 16) | UNIX_EXTENSION)
}

#[cfg(windows)]
fn capture_attributes(metadata: &fs::Metadata) -> Option<u32> {
    use std::os::windows::fs::MetadataExt;
    Some(metadata.file_attributes())
}

#[cfg(not(any(unix, windows)))]
fn capture_attributes(_metadata: &fs::Metadata) -> Option<u32> {
    None
}

fn set_entry_metadata(
    entry: &mut ArchiveEntry,
    metadata: &EntryMetadata,
) -> Result<(), CompressError> {
    if let Some(time) = metadata.created {
        entry.creation_date = to_nt_time(time)?;
        entry.has_creation_date = true;
    }
    if let Some(time) = metadata.accessed {
        entry.access_date = to_nt_time(time)?;
        entry.has_access_date = true;
    }
    if let Some(time) = metadata.modified {
        entry.last_modified_date = to_nt_time(time)?;
        entry.has_last_modified_date = true;
    }
    if let Some(attributes) = metadata.attributes {
        entry.windows_attributes = attributes;
        entry.has_windows_attributes = true;
    }
    Ok(())
}

fn to_nt_time(time: SystemTime) -> Result<NtTime, CompressError> {
    NtTime::try_from(time).map_err(|error| {
        CompressError::Unexpected(anyhow::anyhow!("invalid 7z timestamp: {error:?}"))
    })
}

#[cfg(unix)]
fn open_without_atime(path: &Path) -> std::io::Result<File> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;
    match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOATIME)
        .open(path)
    {
        Ok(file) => Ok(file),
        Err(error) if error.raw_os_error() == Some(libc::EPERM) => File::open(path),
        Err(error) => Err(error),
    }
}

#[cfg(not(unix))]
fn open_without_atime(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

fn apply_entry_metadata(path: &Path, entry: &ArchiveEntry) -> Result<(), sevenz_rust2::Error> {
    apply_posix_mode(path, entry)?;
    if let Err(error) = apply_raw_times(
        path,
        entry
            .has_creation_date
            .then(|| u64::from(entry.creation_date)),
        entry.has_access_date.then(|| u64::from(entry.access_date)),
        entry
            .has_last_modified_date
            .then(|| u64::from(entry.last_modified_date)),
    ) {
        log::warn!(
            target: "rgsm::backup::archive",
            "Could not restore every timestamp for {}: {}",
            path.display(),
            error
        );
    }
    apply_windows_attributes(path, entry)?;
    Ok(())
}

#[cfg(unix)]
fn apply_posix_mode(path: &Path, entry: &ArchiveEntry) -> Result<(), sevenz_rust2::Error> {
    use std::os::unix::fs::PermissionsExt;
    if entry.has_windows_attributes && entry.windows_attributes & UNIX_EXTENSION != 0 {
        let mode = (entry.windows_attributes >> 16) & POSIX_MODE_MASK;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn apply_posix_mode(_path: &Path, _entry: &ArchiveEntry) -> Result<(), sevenz_rust2::Error> {
    Ok(())
}

#[cfg(windows)]
fn apply_windows_attributes(path: &Path, entry: &ArchiveEntry) -> Result<(), sevenz_rust2::Error> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_NOT_CONTENT_INDEXED, FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_READONLY,
        FILE_ATTRIBUTE_SYSTEM, FILE_ATTRIBUTE_TEMPORARY, SetFileAttributesW,
    };

    if !entry.has_windows_attributes || entry.windows_attributes & UNIX_EXTENSION != 0 {
        return Ok(());
    }
    let settable = FILE_ATTRIBUTE_ARCHIVE
        | FILE_ATTRIBUTE_HIDDEN
        | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED
        | FILE_ATTRIBUTE_OFFLINE
        | FILE_ATTRIBUTE_READONLY
        | FILE_ATTRIBUTE_SYSTEM
        | FILE_ATTRIBUTE_TEMPORARY;
    let attributes = entry.windows_attributes & settable;
    let attributes = if attributes == 0 {
        FILE_ATTRIBUTE_NORMAL
    } else {
        attributes
    };
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    if unsafe { SetFileAttributesW(wide.as_ptr(), attributes) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn apply_windows_attributes(
    _path: &Path,
    _entry: &ArchiveEntry,
) -> Result<(), sevenz_rust2::Error> {
    Ok(())
}

fn apply_metadata(path: &Path, metadata: &EntryMetadata) -> std::io::Result<()> {
    let created = metadata
        .created
        .and_then(|time| NtTime::try_from(time).ok())
        .map(u64::from);
    let accessed = metadata
        .accessed
        .and_then(|time| NtTime::try_from(time).ok())
        .map(u64::from);
    let modified = metadata
        .modified
        .and_then(|time| NtTime::try_from(time).ok())
        .map(u64::from);
    apply_raw_times(path, created, accessed, modified)
}

#[cfg(unix)]
fn apply_raw_times(
    path: &Path,
    _created: Option<u64>,
    accessed: Option<u64>,
    modified: Option<u64>,
) -> std::io::Result<()> {
    let current = fs::metadata(path)?;
    let accessed = accessed
        .map(NtTime::new)
        .map(SystemTime::from)
        .unwrap_or(current.accessed()?);
    let modified = modified
        .map(NtTime::new)
        .map(SystemTime::from)
        .unwrap_or(current.modified()?);
    filetime::set_file_times(
        path,
        filetime::FileTime::from_system_time(accessed),
        filetime::FileTime::from_system_time(modified),
    )
}

#[cfg(windows)]
fn apply_raw_times(
    path: &Path,
    created: Option<u64>,
    accessed: Option<u64>,
    modified: Option<u64>,
) -> std::io::Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, OPEN_EXISTING, SetFileTime,
        },
    };
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_WRITE_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    let as_filetime = |value: u64| FILETIME {
        dwLowDateTime: value as u32,
        dwHighDateTime: (value >> 32) as u32,
    };
    let created = created.map(as_filetime);
    let accessed = accessed.map(as_filetime);
    let modified = modified.map(as_filetime);
    let result = unsafe {
        SetFileTime(
            handle,
            created.as_ref().map_or(ptr::null(), ptr::from_ref),
            accessed.as_ref().map_or(ptr::null(), ptr::from_ref),
            modified.as_ref().map_or(ptr::null(), ptr::from_ref),
        )
    };
    let error = (result == 0).then(std::io::Error::last_os_error);
    unsafe { CloseHandle(handle) };
    error.map_or(Ok(()), Err)
}

#[cfg(not(any(unix, windows)))]
fn apply_raw_times(
    _path: &Path,
    _created: Option<u64>,
    _accessed: Option<u64>,
    _modified: Option<u64>,
) -> std::io::Result<()> {
    Ok(())
}

fn sevenz_io(message: String) -> sevenz_rust2::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message).into()
}

fn unexpected(error: impl Into<anyhow::Error>) -> CompressError {
    CompressError::Unexpected(error.into())
}

fn single(error: std::io::Error) -> CompressError {
    CompressError::Single(error.into())
}

fn registry_error(message: String) -> CompressError {
    CompressError::Single(BackupFileError::RegistryError(message))
}
