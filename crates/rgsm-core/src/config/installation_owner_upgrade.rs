use super::{OwnerStore, OwnerStoreError};

impl OwnerStore {
    /// Upgrade already-owned local profiles as well as the monolithic legacy
    /// import. No profile publication or other cloud operation occurs here.
    pub(crate) fn upgrade_local_installations(&self) -> Result<bool, OwnerStoreError> {
        let mut owners = self.load()?;
        let mut config = owners.assemble_effective()?;
        if !super::super::installation_upgrade::migrate(&mut config) {
            return Ok(false);
        }
        crate::updater::preserve_original(
            &self
                .root
                .join("GameSaveManager.installations-before-upgrade.json"),
            &serde_json::to_vec_pretty(&owners)?,
        )?;
        for (device_id, profile) in &mut owners.device_profiles {
            if let Some(device) = config.devices.get(device_id) {
                profile.device = device.clone();
            }
            for game in &config.games {
                if let Some(settings) = profile.games.get_mut(game.backup_dir_name().as_ref()) {
                    settings.binding = game.device_bindings.get(device_id).cloned();
                }
            }
        }
        self.write(&owners)?;
        Ok(true)
    }
}
