use std::{
    io::BufReader,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use anyhow::{Context, Result};
use log::{error, warn};
use rodio::{
    Decoder, OutputStream, OutputStreamHandle, Sink, buffer::SamplesBuffer, source::Source,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::config::{QuickActionSoundPreferences, QuickActionSoundSlots, QuickActionSoundSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum QuickActionSoundEffect {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SoundMode {
    QuickAction,
    Preview,
}

#[derive(Default)]
struct SoundPlayer {
    stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    active_mode: Option<SoundMode>,
    active_effect: Option<QuickActionSoundEffect>,
}

impl SoundPlayer {
    fn ensure_stream(&mut self) -> Result<()> {
        if self.stream.is_none() || self.handle.is_none() {
            let (stream, handle) =
                OutputStream::try_default().context("failed to open output stream")?;
            self.stream = Some(stream);
            self.handle = Some(handle);
        }
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.active_mode = None;
        self.active_effect = None;
    }

    fn play(
        &mut self,
        effect: QuickActionSoundEffect,
        slots: &QuickActionSoundSlots,
        mode: SoundMode,
    ) -> Result<()> {
        let source = load_source(effect, slots)?;
        self.ensure_stream()?;
        self.stop();

        let handle = self
            .handle
            .as_ref()
            .context("audio output stream handle not available")?;
        let sink = Sink::try_new(handle).context("failed to create audio sink")?;
        sink.append(source);
        sink.play();

        self.sink = Some(sink);
        self.active_mode = Some(mode);
        self.active_effect = Some(effect);
        Ok(())
    }

    fn toggle_preview(
        &mut self,
        effect: QuickActionSoundEffect,
        slots: &QuickActionSoundSlots,
    ) -> Result<()> {
        if self.active_mode == Some(SoundMode::Preview) && self.active_effect == Some(effect) {
            self.stop();
            return Ok(());
        }
        self.play(effect, slots, SoundMode::Preview)
    }
}

pub struct SoundManager {
    command_tx: mpsc::Sender<SoundCommand>,
}

impl SoundManager {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        thread::spawn(move || {
            let mut worker = SoundWorker::new(command_rx);
            worker.run();
        });

        Self { command_tx }
    }

    pub fn play_quick_action(
        &self,
        preferences: QuickActionSoundPreferences,
        effect: QuickActionSoundEffect,
    ) {
        if !preferences.enable_sound {
            let _ = self
                .command_tx
                .send(SoundCommand::Stop { respond_to: None });
            return;
        }

        if let Err(err) = self.command_tx.send(SoundCommand::Play {
            effect,
            preferences,
            mode: SoundMode::QuickAction,
            respond_to: None,
        }) {
            warn!(target: "rgsm::sound", "Failed to send quick action sound command: {err}");
        }
    }

    pub fn toggle_preview(
        &self,
        preferences: QuickActionSoundPreferences,
        effect: QuickActionSoundEffect,
    ) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(SoundCommand::Play {
                effect,
                preferences,
                mode: SoundMode::Preview,
                respond_to: Some(tx),
            })
            .context("failed to send preview sound command")?;
        rx.recv().context("preview response dropped")?
    }

    pub fn stop(&self) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(SoundCommand::Stop {
                respond_to: Some(tx),
            })
            .context("failed to send stop sound command")?;
        rx.recv().context("stop response dropped")?;
        Ok(())
    }
}

enum SoundCommand {
    Play {
        effect: QuickActionSoundEffect,
        preferences: QuickActionSoundPreferences,
        mode: SoundMode,
        respond_to: Option<mpsc::Sender<Result<()>>>,
    },
    Stop {
        respond_to: Option<mpsc::Sender<()>>,
    },
}

struct SoundWorker {
    command_rx: mpsc::Receiver<SoundCommand>,
    player: SoundPlayer,
}

impl SoundWorker {
    fn new(command_rx: mpsc::Receiver<SoundCommand>) -> Self {
        Self {
            command_rx,
            player: SoundPlayer::default(),
        }
    }

    fn run(&mut self) {
        while let Ok(command) = self.command_rx.recv() {
            self.handle_command(command);
        }
    }

    fn handle_command(&mut self, command: SoundCommand) {
        match command {
            SoundCommand::Play {
                effect,
                preferences,
                mode: SoundMode::QuickAction,
                respond_to,
            } => {
                let result = self
                    .player
                    .play(effect, &preferences.sounds, SoundMode::QuickAction);
                if let Some(tx) = respond_to {
                    let _ = tx.send(result);
                } else if let Err(err) = result {
                    warn!(target: "rgsm::sound", "Failed to play quick action sound: {err:?}");
                }
            }
            SoundCommand::Play {
                effect,
                preferences,
                mode: SoundMode::Preview,
                respond_to,
            } => {
                let result = self.player.toggle_preview(effect, &preferences.sounds);
                if let Some(tx) = respond_to {
                    let _ = tx.send(result);
                }
            }
            SoundCommand::Stop { respond_to } => {
                self.player.stop();
                if let Some(tx) = respond_to {
                    let _ = tx.send(());
                }
            }
        }
    }
}

fn load_source(
    effect: QuickActionSoundEffect,
    slots: &QuickActionSoundSlots,
) -> Result<Box<dyn Source<Item = f32> + Send>> {
    let source = match effect {
        QuickActionSoundEffect::Success => &slots.success,
        QuickActionSoundEffect::Failure => &slots.failure,
    };

    match source {
        QuickActionSoundSource::Default => Ok(default_source(effect)),
        QuickActionSoundSource::File { path } => load_from_file(path),
    }
}

fn default_source(effect: QuickActionSoundEffect) -> Box<dyn Source<Item = f32> + Send> {
    const SAMPLE_RATE: u32 = 44_100;
    let (sequence, amplitude) = match effect {
        QuickActionSoundEffect::Success => (&[(880.0, 120_u64), (1175.0, 160_u64)][..], 0.20),
        QuickActionSoundEffect::Failure => (&[(440.0, 220_u64), (330.0, 260_u64)][..], 0.22),
    };

    let mut mono = Vec::new();
    for (freq, duration_ms) in sequence {
        mono.extend(tone_samples(*freq, *duration_ms, SAMPLE_RATE, amplitude));
    }

    let mut stereo = Vec::with_capacity(mono.len() * 2);
    for sample in mono {
        stereo.push(sample);
        stereo.push(sample);
    }

    Box::new(SamplesBuffer::new(2, SAMPLE_RATE, stereo))
}

fn tone_samples(freq: f32, duration_ms: u64, sample_rate: u32, amplitude: f32) -> Vec<f32> {
    let total_samples = ((duration_ms as f32 / 1000.0) * sample_rate as f32) as usize;
    let omega = 2.0 * std::f32::consts::PI * freq / sample_rate as f32;
    (0..total_samples)
        .map(|i| (omega * i as f32).sin() * amplitude)
        .collect()
}

fn load_from_file(path: &str) -> Result<Box<dyn Source<Item = f32> + Send>> {
    if path.trim().is_empty() {
        anyhow::bail!("audio file path is empty");
    }
    let resolved = resolve_path(path);
    let file = std::fs::File::open(&resolved)
        .with_context(|| format!("failed to open audio file at {}", resolved.display()))?;
    let decoder = Decoder::new(BufReader::new(file))
        .with_context(|| format!("failed to decode audio file at {}", resolved.display()))?;
    Ok(Box::new(decoder.convert_samples()))
}

fn resolve_path(path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return candidate.to_path_buf();
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.join(candidate);
        }
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(candidate)
}

pub fn setup(app: &mut tauri::App) -> Result<()> {
    let manager = SoundManager::new();
    app.manage(manager);
    Ok(())
}

pub fn play_quick_action_sound(
    app: &AppHandle,
    preferences: QuickActionSoundPreferences,
    effect: QuickActionSoundEffect,
) {
    if let Some(manager) = app.try_state::<SoundManager>() {
        manager.play_quick_action(preferences, effect);
    }
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_quick_action_sound_preview(
    app: AppHandle,
    preferences: QuickActionSoundPreferences,
    effect: QuickActionSoundEffect,
) -> Result<(), String> {
    let manager = app.state::<SoundManager>();
    manager.toggle_preview(preferences, effect).map_err(|err| {
        error!(target: "rgsm::sound", "Failed to preview sound: {err:?}");
        err.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn stop_sound_playback(app: AppHandle) -> Result<(), String> {
    let manager = app.state::<SoundManager>();
    manager.stop().map_err(|err| {
        error!(target: "rgsm::sound", "Failed to stop sound: {err:?}");
        err.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn choose_quick_action_sound_file(app: AppHandle) -> Result<String, String> {
    match app
        .dialog()
        .file()
        .add_filter("Audio", &["mp3", "wav", "flac", "ogg"])
        .blocking_pick_file()
    {
        Some(path) => Ok(path.to_string()),
        None => Err("Failed to open dialog.".to_string()),
    }
}
