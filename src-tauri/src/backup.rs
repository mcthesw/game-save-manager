use crate::archive::{compress_to_file, decompress_from_file};
use crate::cloud::{upload_backup_info, upload_config};
use crate::config::{get_config, set_config, Game};
use crate::errors::BackupError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{fs, path};

/// A backup is a zip file that contains
/// all the file that the save unit has declared.
/// The date is the unique indicator for a backup
#[derive(Debug, Serialize, Deserialize)]
pub struct Backup {
    pub date: String,
    pub describe: String,
    pub path: String, // like "D:\\SaveManager\save_data\Game1\date.zip"
}

/// A backup list info is a json file in a backup folder for a game.
/// It contains the name of the game,
/// and all backups' path
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupListInfo {
    pub name: String,
    pub backups: Vec<Backup>,
}

impl Game {
    pub fn get_backup_list_info(&self) -> Result<BackupListInfo, BackupError> {
        let config = get_config()?;
        let backup_path = path::Path::new(&config.backup_path)
            .join(&self.name)
            .join("Backups.json");
        let backup_info = serde_json::from_slice(&fs::read(backup_path)?)?;
        Ok(backup_info)
    }
    pub fn set_backup_list_info(&self, new_info: &BackupListInfo) -> Result<(), BackupError> {
        let config = get_config()?;
        let saves_path = path::Path::new(&config.backup_path)
            .join(&self.name)
            .join("Backups.json");
        // 处理文件夹不存在的情况，一般发生在初次下载云存档时
        let prefix_root = saves_path.parent().ok_or(BackupError::NonePathError)?;
        if !prefix_root.exists() {
            fs::create_dir_all(prefix_root)?;
        }
        fs::write(saves_path, serde_json::to_string_pretty(&new_info)?)?;
        Ok(())
    }
    pub async fn backup_save(&self, describe: &str) -> Result<(), BackupError> {
        let config = get_config()?;
        let backup_path = path::Path::new(&config.backup_path).join(&self.name); // the backup zip file should be placed here
        let date = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let save_paths = &self.save_paths; // everything you should copy

        let zip_path = backup_path.join([&date, ".zip"].concat());
        if let Err(e) = compress_to_file(save_paths, &zip_path) {
            // delete the zip if failed to write
            fs::remove_file(&zip_path)?;
            return Err(BackupError::BackupFileError(e));
        }

        let backup_list_info = Backup {
            date,
            describe: describe.to_string(),
            path: zip_path
                .to_str()
                .ok_or(BackupError::NonePathError)?
                .to_string(),
        };
        let mut infos = self.get_backup_list_info()?;
        infos.backups.push(backup_list_info);
        self.set_backup_list_info(&infos)?;

        // 随时同步到云端
        if config.settings.cloud_settings.always_sync {
            let op = config.settings.cloud_settings.backend.get_op()?;
            // 上传存档记录信息
            upload_backup_info(&op, infos).await?;
            // 上传对应压缩包
            // 此处防止路径中出现反斜杠，导致云端无法识别，替换win的反斜杠为斜杠
            let p = zip_path
                .iter()
                .map(|s| s.to_str().ok_or(BackupError::NonePathError))
                .collect::<Result<Vec<&str>, BackupError>>()?
                .join("/");
            op.write(&p, fs::read(&zip_path)?).await?;
        }
        Result::Ok(())
    }
    pub fn apply_backup(&self, date: &str) -> Result<(), BackupError> {
        let config = get_config()?;
        let backup_path = path::Path::new(&config.backup_path).join(&self.name);
        if config.settings.extra_backup_when_apply {
            self.create_extra_backup()?;
        }
        decompress_from_file(&self.save_paths, &backup_path, date)?;
        Result::Ok(())
    }
    pub fn create_extra_backup(&self) -> Result<(), BackupError> {
        let config = get_config()?;
        let extra_backup_path = path::Path::new(&config.backup_path)
            .join(&self.name)
            .join("extra_backup");

        // Create extra backup
        if !extra_backup_path.exists() {
            fs::create_dir_all(&extra_backup_path)?;
        }
        let date = chrono::Local::now()
            .format("Overwrite_%Y-%m-%d_%H-%M-%S")
            .to_string();
        let zip_path = &extra_backup_path.join([&date, ".zip"].concat());
        compress_to_file(&self.save_paths, zip_path)?;

        // Delete oldest extra backup if there are more than 5 file
        let extra_backups_dir: Vec<_> = extra_backup_path.read_dir()?.collect();
        let mut extra_backups = Vec::new();
        if extra_backups_dir.len() >= 5 {
            extra_backups_dir.into_iter().try_for_each(|f| {
                extra_backups.push(
                    f?.file_name()
                        .to_str()
                        .ok_or(BackupError::NonePathError)?
                        .to_string(),
                );
                Result::<(), BackupError>::Ok(())
            })?;
            extra_backups.sort();
            let oldest = extra_backups.first().ok_or(BackupError::NonePathError)?; // 一定要改好这一行
            println!("oldest{:?}", oldest);
            fs::remove_file(extra_backup_path.join(oldest))?;
        }
        Result::Ok(())
    }
    pub async fn delete_backup(&self, date: &str) -> Result<(), BackupError> {
        let config = get_config()?;
        let save_path = PathBuf::from(&config.backup_path)
            .join(&self.name)
            .join(date.to_string() + ".zip");
        fs::remove_file(&save_path)?;

        let mut saves = self.get_backup_list_info()?;
        saves.backups.retain(|x| x.date != date);
        self.set_backup_list_info(&saves)?;

        // 随时同步到云端
        if config.settings.cloud_settings.always_sync {
            let op = config.settings.cloud_settings.backend.get_op()?;
            // 上传存档记录信息
            upload_backup_info(&op, saves).await?;
            // 删除对应压缩包
            // 此处防止路径中出现反斜杠，导致云端无法识别，替换win的反斜杠为斜杠
            let p = save_path
                .iter()
                .map(|s| s.to_str().ok_or(BackupError::NonePathError))
                .collect::<Result<Vec<&str>, BackupError>>()?
                .join("/");
            op.delete(&p).await?;
        }
        Ok(())
    }
    pub async fn delete(&self) -> Result<(), BackupError> {
        let mut config = get_config()?;
        let backup_path = PathBuf::from(&config.backup_path).join(&self.name);
        fs::remove_dir_all(&backup_path)?;

        config.games.retain(|x| x.name != self.name);
        set_config(&config).await?;

        // 随时同步到云端
        if config.settings.cloud_settings.always_sync {
            let op = config.settings.cloud_settings.backend.get_op()?;
            println!(
                "{:#?}",
                backup_path.to_str().ok_or(BackupError::NonePathError)?
            );
            // 此处防止路径中出现反斜杠，导致云端无法识别，替换win的反斜杠为斜杠
            let p = backup_path
                .iter()
                .map(|s| s.to_str().ok_or(BackupError::NonePathError))
                .collect::<Result<Vec<&str>, BackupError>>()?
                .join("/");
            op.remove_all(&p).await?;
            // 也上传新的配置文件
            upload_config(&op).await?;
        }

        Ok(())
    }
    pub async fn set_backup_describe(&self, date: &str, describe: &str) -> Result<(), BackupError> {
        let mut saves = self.get_backup_list_info()?;
        let pos = saves.backups.iter().position(|x| x.date == date).ok_or(
            BackupError::BackupNotExist {
                name: self.name.clone(),
                date: date.to_string(),
            },
        )?;
        saves.backups[pos].describe = describe.to_string();
        self.set_backup_list_info(&saves)?;
        Ok(())
    }
}

async fn create_backup_folder(name: &str) -> Result<(), BackupError> {
    let config = get_config()?;

    let backup_path = PathBuf::from(&config.backup_path).join(name);
    let info: BackupListInfo = if !backup_path.exists() {
        fs::create_dir_all(&backup_path)?;
        BackupListInfo {
            name: name.to_string(),
            backups: Vec::new(),
        }
    } else {
        // 如果已经存在，info从原来的文件中读取
        let bytes = fs::read(backup_path.join("Backups.json"));
        serde_json::from_slice(&bytes?)?
    };
    fs::write(
        backup_path.join("Backups.json"),
        serde_json::to_string_pretty(&info)?,
    )?;

    // 处理云同步
    if config.settings.cloud_settings.always_sync {
        let op = config.settings.cloud_settings.backend.get_op()?;
        // 上传存档记录信息
        upload_backup_info(&op, info).await?;
    }

    Ok(())
}

pub async fn create_game_backup(game: Game) -> Result<(), BackupError> {
    let mut config = get_config()?;
    create_backup_folder(&game.name).await?;

    // 查找是否存在与新游戏中的 `name` 字段相同的游戏
    let pos = config.games.iter().position(|g| g.name == game.name);
    match pos {
        Some(index) => {
            // 如果找到了，就用新的游戏覆盖它
            config.games[index] = game;
        }
        None => {
            // 如果没有找到，就将新的游戏添加到 `games` 数组中
            config.games.push(game);
        }
    }
    set_config(&config).await?;
    Ok(())
}

pub async fn backup_all() -> Result<(), BackupError> {
    let config = get_config()?;
    for game in &config.games {
        game.backup_save("Backup all").await?;
    }
    Ok(())
}
pub async fn apply_all() -> Result<(), BackupError> {
    let config = get_config()?;
    for game in &config.games {
        let date = game
            .get_backup_list_info()?
            .backups
            .last()
            .ok_or(BackupError::NoBackupAvailable)?
            .date
            .clone();
        game.apply_backup(&date)?;
    }
    Ok(())
}
