use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use rand::{Rng, distr::Alphanumeric};
use serde::{Deserialize, Serialize};

const HOST_CONFIG_FILE: &str = "GameSaveManager.host.json";
const HOST_CONFIG_SCHEMA_VERSION: u32 = 1;
const TOKEN_LENGTH: usize = 48;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostConfig {
    pub schema_version: u32,
    pub port: u16,
    pub api_token: String,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            schema_version: HOST_CONFIG_SCHEMA_VERSION,
            port: 0,
            api_token: generate_token(),
        }
    }
}

#[derive(Debug)]
pub struct HostConfigStore {
    path: PathBuf,
    config: HostConfig,
}

impl HostConfigStore {
    pub fn load(app_data_dir: &Path) -> anyhow::Result<Self> {
        fs::create_dir_all(app_data_dir)?;
        let path = app_data_dir.join(HOST_CONFIG_FILE);
        let mut config = if path.exists() {
            restrict_private_permissions(&path)?;
            serde_json::from_slice::<HostConfig>(&fs::read(&path)?)?
        } else {
            HostConfig::default()
        };

        if config.schema_version != HOST_CONFIG_SCHEMA_VERSION {
            anyhow::bail!(
                "Unsupported Host configuration schema version: {}",
                config.schema_version
            );
        }
        if config.api_token.is_empty() {
            config.api_token = generate_token();
        }

        let store = Self { path, config };
        if !store.path.exists() {
            store.save()?;
        }
        Ok(store)
    }

    pub fn prepare(app_data_dir: &Path) -> anyhow::Result<HostConfig> {
        let mut store = Self::load(app_data_dir)?;
        if store.config.port == 0 {
            let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
            store.set_bound_port(listener.local_addr()?.port())?;
        }
        Ok(store.config.clone())
    }

    pub fn config(&self) -> &HostConfig {
        &self.config
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn regenerate_token(&mut self) -> anyhow::Result<String> {
        self.config.api_token = generate_token();
        self.save()?;
        Ok(self.config.api_token.clone())
    }

    pub fn set_bound_port(&mut self, port: u16) -> anyhow::Result<()> {
        if self.config.port != port {
            self.config.port = port;
            self.save()?;
        }
        Ok(())
    }

    fn save(&self) -> anyhow::Result<()> {
        let temp_path = self.path.with_extension("json.tmp");
        write_private(&temp_path, &serde_json::to_vec_pretty(&self.config)?)?;
        replace_file(&temp_path, &self.path)?;
        restrict_private_permissions(&self.path)?;
        Ok(())
    }
}

fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

#[cfg(unix)]
fn restrict_private_permissions(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_private_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn generate_token() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(TOKEN_LENGTH)
        .map(char::from)
        .collect()
}

fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    if destination.exists() {
        let backup = destination.with_extension("json.bak");
        let _ = fs::remove_file(&backup);
        fs::rename(destination, &backup)?;
        match fs::rename(source, destination) {
            Ok(()) => {
                let _ = fs::remove_file(backup);
                return Ok(());
            }
            Err(error) => {
                let _ = fs::rename(backup, destination);
                return Err(error);
            }
        }
    }

    fs::rename(source, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_reuses_host_configuration() {
        let root = temp_dir::TempDir::new().unwrap();
        let mut first = HostConfigStore::load(root.path()).unwrap();
        assert_eq!(first.config().port, 0);
        assert_eq!(first.config().api_token.len(), TOKEN_LENGTH);

        first.set_bound_port(43127).unwrap();
        let second = HostConfigStore::load(root.path()).unwrap();
        assert_eq!(second.config().port, 43127);
        assert_eq!(second.config().api_token, first.config().api_token);
    }

    #[cfg(unix)]
    #[test]
    fn host_configuration_is_owner_only() {
        let root = temp_dir::TempDir::new().unwrap();
        let store = HostConfigStore::load(root.path()).unwrap();
        let mode = fs::metadata(store.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn regenerates_and_persists_api_token() {
        let root = temp_dir::TempDir::new().unwrap();
        let mut store = HostConfigStore::load(root.path()).unwrap();
        let original = store.config().api_token.clone();

        let regenerated = store.regenerate_token().unwrap();
        assert_ne!(regenerated, original);
        assert_eq!(regenerated.len(), TOKEN_LENGTH);

        let reloaded = HostConfigStore::load(root.path()).unwrap();
        assert_eq!(reloaded.config().api_token, regenerated);
    }
}
