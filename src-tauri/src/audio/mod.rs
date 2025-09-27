use std::{
    f32::consts::PI,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use log::{error, warn};
use rodio::{OutputStream, OutputStreamHandle, Sink, decoder::Decoder};
use tokio::sync::mpsc;

use crate::config::{
    QuickActionSoundKind, QuickActionSoundProfile, QuickActionSoundSettings, QuickActionSoundSource,
};

const COMMAND_BUFFER: usize = 8;
const SAMPLE_RATE: u32 = 44_100;
const PREVIEW_VOLUME: f32 = 0.6;

#[derive(Clone, Debug)]
pub enum SoundRequest {
    PlayEffect {
        kind: QuickActionSoundKind,
        profile: QuickActionSoundProfile,
    },
    TogglePreview {
        kind: QuickActionSoundKind,
        profile: QuickActionSoundProfile,
    },
    Stop,
}

pub struct AudioManager {
    command_tx: mpsc::Sender<SoundRequest>,
}

impl AudioManager {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel(COMMAND_BUFFER);
        AudioWorker::spawn(command_rx);
        Self { command_tx }
    }

    pub fn play_effect(&self, kind: QuickActionSoundKind, profile: QuickActionSoundProfile) {
        self.enqueue(SoundRequest::PlayEffect { kind, profile });
    }

    pub fn toggle_preview(&self, kind: QuickActionSoundKind, profile: QuickActionSoundProfile) {
        self.enqueue(SoundRequest::TogglePreview { kind, profile });
    }

    pub fn stop(&self) {
        self.enqueue(SoundRequest::Stop);
    }

    fn enqueue(&self, request: SoundRequest) {
        let tx = self.command_tx.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = tx.send(request).await {
                warn!(
                    target: "rgsm::audio",
                    "Failed to send audio request: {err}"
                );
            }
        });
    }
}

struct AudioDevice {
    _stream: OutputStream,
    handle: OutputStreamHandle,
}

struct AudioWorker {
    device: Option<AudioDevice>,
    current_sink: Option<Sink>,
    current_preview: Option<QuickActionSoundKind>,
    command_rx: mpsc::Receiver<SoundRequest>,
}

impl AudioWorker {
    fn spawn(command_rx: mpsc::Receiver<SoundRequest>) {
        let device = match OutputStream::try_default() {
            Ok((stream, handle)) => Some(AudioDevice {
                _stream: stream,
                handle,
            }),
            Err(err) => {
                error!(target: "rgsm::audio", "Failed to initialize audio output: {err}");
                None
            }
        };

        let worker = Self {
            device,
            current_sink: None,
            current_preview: None,
            command_rx,
        };

        tauri::async_runtime::spawn(async move { worker.run().await });
    }

    async fn run(mut self) {
        while let Some(request) = self.command_rx.recv().await {
            if let Err(err) = self.handle_request(request) {
                error!(target: "rgsm::audio", "Audio request failed: {err:?}");
            }
        }

        self.stop_current();
    }

    fn handle_request(&mut self, request: SoundRequest) -> Result<()> {
        match request {
            SoundRequest::Stop => {
                self.stop_current();
                self.current_preview = None;
            }
            SoundRequest::PlayEffect { kind: _, profile } => {
                self.stop_current();
                self.current_preview = None;
                if profile.enabled {
                    self.play_profile(&profile)?;
                }
            }
            SoundRequest::TogglePreview { kind, profile } => {
                if self.current_preview == Some(kind) {
                    self.stop_current();
                    self.current_preview = None;
                } else {
                    self.stop_current();
                    self.play_profile(&profile)?;
                    self.current_preview = Some(kind);
                }
            }
        }

        Ok(())
    }

    fn stop_current(&mut self) {
        if let Some(sink) = self.current_sink.take() {
            sink.stop();
        }
    }

    fn play_profile(&mut self, profile: &QuickActionSoundProfile) -> Result<()> {
        let Some(device) = self.device.as_ref() else {
            return Ok(());
        };

        let mut sink = Sink::try_new(&device.handle)
            .context("failed to create rodio sink for quick action sound")?;
        sink.set_volume(PREVIEW_VOLUME);

        match &profile.source {
            QuickActionSoundSource::SuccessTone => {
                sink.append(synthesize_success_tone());
            }
            QuickActionSoundSource::ErrorTone => {
                sink.append(synthesize_error_tone());
            }
            QuickActionSoundSource::File { path } => {
                let resolved = resolve_path(path)?;
                let file = File::open(&resolved).with_context(|| {
                    format!("failed to open sound file: {}", resolved.display())
                })?;
                let decoder =
                    Decoder::new(BufReader::new(file)).context("failed to decode sound file")?;
                sink.append(decoder);
            }
        }

        sink.play();
        self.current_sink = Some(sink);
        Ok(())
    }
}

fn resolve_path(path: &str) -> Result<PathBuf> {
    let buf = PathBuf::from(path);
    if buf.is_absolute() {
        return Ok(buf);
    }

    let exe_dir = std::env::current_exe()
        .context("failed to determine executable path when resolving sound file")?
        .parent()
        .map(Path::to_path_buf)
        .context("executable has no parent directory")?;

    Ok(exe_dir.join(buf))
}

fn synthesize_success_tone() -> rodio::buffer::SamplesBuffer<f32> {
    synthesize_sequence(&[(987.77, 0.12), (1318.51, 0.16), (1567.98, 0.18)])
}

fn synthesize_error_tone() -> rodio::buffer::SamplesBuffer<f32> {
    synthesize_sequence(&[(392.0, 0.16), (311.13, 0.2), (196.0, 0.22)])
}

fn synthesize_sequence(notes: &[(f32, f32)]) -> rodio::buffer::SamplesBuffer<f32> {
    let mut samples = Vec::new();

    for (idx, (frequency, duration)) in notes.iter().enumerate() {
        let total_samples = (SAMPLE_RATE as f32 * duration) as usize;
        for n in 0..total_samples {
            let progress = n as f32 / total_samples as f32;
            let envelope = fade_envelope(progress);
            let sample = (2.0 * PI * frequency * n as f32 / SAMPLE_RATE as f32).sin();
            samples.push(sample * envelope * 0.4);
        }

        if idx + 1 != notes.len() {
            samples.extend(std::iter::repeat(0.0).take((SAMPLE_RATE as f32 * 0.04) as usize));
        }
    }

    rodio::buffer::SamplesBuffer::new(1, SAMPLE_RATE, samples)
}

fn fade_envelope(progress: f32) -> f32 {
    let fade_in = (progress / 0.1).clamp(0.0, 1.0);
    let fade_out = ((1.0 - progress) / 0.2).clamp(0.0, 1.0);
    fade_in * fade_out
}

pub fn resolve_profile<'a>(
    settings: &'a QuickActionSoundSettings,
    kind: QuickActionSoundKind,
) -> &'a QuickActionSoundProfile {
    match kind {
        QuickActionSoundKind::Success => &settings.success,
        QuickActionSoundKind::Error => &settings.error,
    }
}
