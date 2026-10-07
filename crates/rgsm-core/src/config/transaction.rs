//! A form edit commits its Game and Device changes under one store lock.
use super::{Config, owner_store::OwnerStore};
use crate::preclude::ConfigError;

pub(crate) fn edit_config<T>(
    edit: impl FnOnce(&mut Config) -> anyhow::Result<T>,
) -> anyhow::Result<(T, Config)> {
    let _guard = super::utils::CONFIG_STORE_LOCK
        .lock()
        .map_err(|_| ConfigError::StoreLockPoisoned)?;
    let previous = super::utils::get_config_unlocked()?;
    let mut next = previous.clone();
    let result = edit(&mut next)?;
    next.normalize_local_fields();
    super::backup::rotate_config_backups(&previous);
    OwnerStore::runtime().merge_effective(&next)?;
    Ok((result, next))
}
