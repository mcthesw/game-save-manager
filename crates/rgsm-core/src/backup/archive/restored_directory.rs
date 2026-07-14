use std::{fs, io, path::Path};

pub(super) fn remove_restored_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    make_directory_tree_removable(path)?;

    fs::remove_dir_all(path)
}

#[cfg(unix)]
fn make_directory_tree_removable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Ok(());
    }

    let mut permissions = metadata.permissions();
    let mode = permissions.mode();
    if mode & 0o700 != 0o700 {
        permissions.set_mode(mode | 0o700);
        fs::set_permissions(path, permissions)?;
    }

    for child in fs::read_dir(path)? {
        let child = child?;
        if child.file_type()?.is_dir() {
            make_directory_tree_removable(&child.path())?;
        }
    }
    Ok(())
}
