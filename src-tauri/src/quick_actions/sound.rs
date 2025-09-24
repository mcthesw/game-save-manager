use std::{
    f32::consts::PI,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use log::error;
use rodio::{Decoder, OutputStream, Sink, buffer::SamplesBuffer};
use tauri::async_runtime;

use crate::config::{QuickActionSoundVariant, get_config};

const SAMPLE_RATE: u32 = 44_100;
const NOTE_VOLUME: f32 = 0.32;
const FADE_DURATION_MS: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickActionSoundEvent {
    Success,
    Failure,
}

pub fn play_sound(event: QuickActionSoundEvent) {
    let config = match get_config() {
        Ok(cfg) => cfg,
        Err(err) => {
            error!(
                target: "rgsm::quick_action::sound",
                "Failed to load config for sound playback: {err:?}"
            );
            return;
        }
    };

    let sound_settings = config.quick_action.sound;
    if !sound_settings.enabled {
        return;
    }

    let variant = match event {
        QuickActionSoundEvent::Success => sound_settings.success,
        QuickActionSoundEvent::Failure => sound_settings.failure,
    };

    let needs_fallback = !matches!(variant.clone(), QuickActionSoundVariant::Default);

    async_runtime::spawn_blocking(move || {
        if let Err(err) = play_variant(event, variant) {
            error!(
                target: "rgsm::quick_action::sound",
                "Failed to play quick action sound: {err:?}"
            );

            if needs_fallback {
                if let Err(fallback_err) = play_variant(event, QuickActionSoundVariant::Default) {
                    error!(
                        target: "rgsm::quick_action::sound",
                        "Failed to play fallback quick action sound: {fallback_err:?}"
                    );
                }
            }
        }
    });
}

fn play_variant(event: QuickActionSoundEvent, variant: QuickActionSoundVariant) -> Result<()> {
    let (_stream, handle) =
        OutputStream::try_default().context("failed to open default audio output")?;
    let sink = Sink::try_new(&handle).context("failed to create audio sink")?;

    match variant {
        QuickActionSoundVariant::Default => match event {
            QuickActionSoundEvent::Success => sink.append(synthesize_success_sound()),
            QuickActionSoundEvent::Failure => sink.append(synthesize_failure_sound()),
        },
        QuickActionSoundVariant::Custom { path } => {
            let resolved = resolve_audio_path(Path::new(&path));
            let reader =
                BufReader::new(File::open(&resolved).with_context(|| {
                    format!("failed to open audio file at {}", resolved.display())
                })?);
            let decoder = Decoder::new(reader).context("failed to decode audio file")?;
            sink.append(decoder);
        }
    }

    sink.sleep_until_end();
    Ok(())
}

fn resolve_audio_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    if let Some(str_path) = path.to_str() {
        if let Some(stripped) = str_path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        }
    }

    match std::env::current_exe() {
        Ok(exe) => exe
            .parent()
            .map(|parent| parent.join(path))
            .unwrap_or_else(|| path.to_path_buf()),
        Err(_) => path.to_path_buf(),
    }
}

fn synthesize_success_sound() -> SamplesBuffer<f32> {
    let mut samples = Vec::with_capacity((SAMPLE_RATE as f32 * 0.6) as usize);
    append_note(&mut samples, 1046.5, 140, 0.36);
    append_silence(&mut samples, 20);
    append_note(&mut samples, 1318.5, 140, 0.33);
    append_silence(&mut samples, 20);
    append_note(&mut samples, 1567.9, 220, 0.30);
    SamplesBuffer::new(1, SAMPLE_RATE, samples)
}

fn synthesize_failure_sound() -> SamplesBuffer<f32> {
    let mut samples = Vec::with_capacity((SAMPLE_RATE as f32 * 0.5) as usize);
    append_note(&mut samples, 440.0, 200, 0.34);
    append_silence(&mut samples, 30);
    append_note(&mut samples, 311.1, 260, 0.30);
    SamplesBuffer::new(1, SAMPLE_RATE, samples)
}

fn append_note(buffer: &mut Vec<f32>, frequency: f32, duration_ms: u32, amplitude: f32) {
    let total_samples = ((SAMPLE_RATE as u64 * duration_ms as u64) / 1000) as usize;
    let fade_samples = ((SAMPLE_RATE as f32 * FADE_DURATION_MS as f32) / 1000.0) as usize;

    for n in 0..total_samples {
        let t = n as f32 / SAMPLE_RATE as f32;
        let envelope = amplitude_envelope(n, total_samples, fade_samples);
        let sample = (2.0 * PI * frequency * t).sin() * (NOTE_VOLUME * amplitude) * envelope;
        buffer.push(sample);
    }
}

fn append_silence(buffer: &mut Vec<f32>, duration_ms: u32) {
    let samples = ((SAMPLE_RATE as u64 * duration_ms as u64) / 1000) as usize;
    buffer.extend(std::iter::repeat(0.0).take(samples));
}

fn amplitude_envelope(index: usize, total: usize, fade_samples: usize) -> f32 {
    if fade_samples == 0 || total <= fade_samples * 2 {
        return 1.0;
    }

    if index < fade_samples {
        index as f32 / fade_samples as f32
    } else if index > total.saturating_sub(fade_samples) {
        (total.saturating_sub(index)) as f32 / fade_samples as f32
    } else {
        1.0
    }
}
