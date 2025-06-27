use std::path::PathBuf;
use log::{error, info};
use rodio::{Decoder, OutputStream, Sink};
use std::fs::File;
use std::io::BufReader;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::config::get_config;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
pub enum SoundType {
    None,
    Default,
    Custom(String),
}

impl Default for SoundType {
    fn default() -> Self {
        SoundType::Default
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct SoundSettings {
    #[serde(default)]
    pub enable_sound: bool,
    #[serde(default)]
    pub success_sound: SoundType,
    #[serde(default)]
    pub error_sound: SoundType,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            enable_sound: true,
            success_sound: SoundType::Default,
            error_sound: SoundType::Default,
        }
    }
}

pub enum NotificationResult {
    Success,
    Error,
}

pub fn play_notification_sound(result: NotificationResult) {
    let config = match get_config() {
        Ok(config) => config,
        Err(e) => {
            error!(target: "rgsm::sound", "Failed to get config: {}", e);
            return;
        }
    };

    if !config.settings.sound_settings.enable_sound {
        return;
    }

    let sound_type = match result {
        NotificationResult::Success => &config.settings.sound_settings.success_sound,
        NotificationResult::Error => &config.settings.sound_settings.error_sound,
    };

    match sound_type {
        SoundType::None => return,
        SoundType::Default => play_default_sound(result),
        SoundType::Custom(path) => play_custom_sound(path),
    }
}

fn play_default_sound(result: NotificationResult) {
    // Generate a simple beep sound using rodio
    match result {
        NotificationResult::Success => {
            info!(target: "rgsm::sound", "Playing default success sound");
            if let Err(e) = play_beep(800.0, 200) {
                error!(target: "rgsm::sound", "Failed to play success sound: {}", e);
            }
        }
        NotificationResult::Error => {
            info!(target: "rgsm::sound", "Playing default error sound");
            if let Err(e) = play_beep(300.0, 300) {
                error!(target: "rgsm::sound", "Failed to play error sound: {}", e);
            }
        }
    }
}

fn play_custom_sound(path: &str) {
    info!(target: "rgsm::sound", "Playing custom sound from: {}", path);
    
    let path = PathBuf::from(path);
    
    // Try to open the file
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(e) => {
            error!(target: "rgsm::sound", "Failed to open sound file {}: {}", path.display(), e);
            return;
        }
    };
    
    let (_stream, stream_handle) = match OutputStream::try_default() {
        Ok(output) => output,
        Err(e) => {
            error!(target: "rgsm::sound", "Failed to get audio output stream: {}", e);
            return;
        }
    };
    
    let sink = Sink::try_new(&stream_handle).unwrap();
    
    // Load the sound file
    let source = match Decoder::new(BufReader::new(file)) {
        Ok(source) => source,
        Err(e) => {
            error!(target: "rgsm::sound", "Failed to decode sound file: {}", e);
            return;
        }
    };
    
    // Play the sound
    sink.append(source);
    sink.sleep_until_end();
}

fn play_beep(frequency: f32, duration_ms: u64) -> Result<(), Box<dyn std::error::Error>> {
    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&stream_handle)?;
    
    // Create a simple sine wave
    let source = rodio::source::SineWave::new(frequency);
    let source = source.take_duration(std::time::Duration::from_millis(duration_ms));
    
    sink.append(source);
    sink.sleep_until_end();
    
    Ok(())
}