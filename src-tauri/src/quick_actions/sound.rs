use std::{
    f32::consts::PI,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use anyhow::{Context, Result as AnyResult};
use log::{debug, error, warn};
use rodio::{Decoder, OutputStream, Sink, buffer::SamplesBuffer};
use tauri::async_runtime;
use tokio::sync::{Mutex, oneshot};

use crate::config::{QuickActionSoundVariant, get_config};

const SAMPLE_RATE: u32 = 44_100;
const NOTE_VOLUME: f32 = 0.32;
const FADE_DURATION_MS: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickActionSoundEvent {
    Success,
    Failure,
}

#[derive(Debug, Clone)]
pub enum PreviewSoundStatus {
    Started { fallback_warning: Option<String> },
    Stopped,
}

#[derive(Debug, Clone)]
pub enum PlaybackStart {
    Primary,
    FallbackUsed { primary_error: String },
}

#[derive(Debug, Clone)]
pub struct PlaybackError {
    primary_error: String,
    fallback_error: Option<String>,
}

impl PlaybackError {
    fn new(primary_error: String, fallback_error: Option<String>) -> Self {
        Self {
            primary_error,
            fallback_error,
        }
    }

    pub fn to_user_message(&self) -> String {
        let mut message = format!("Failed to play the selected sound: {}", self.primary_error);
        if let Some(fallback_error) = &self.fallback_error {
            message.push_str(&format!("\nFallback also failed: {fallback_error}"));
        }
        message
    }

    fn log_for_playback(&self, target: &str) {
        error!(target: target, "Failed to play quick action sound: {}", self.primary_error);
        if let Some(fallback_error) = &self.fallback_error {
            error!(
                target: target,
                "Failed to play fallback quick action sound: {fallback_error}"
            );
        }
    }

    pub fn log_for_preview(&self) {
        error!(
            target: "rgsm::quick_action::sound",
            "Failed to play the selected sound: {}",
            self.primary_error
        );
        if let Some(fallback_error) = &self.fallback_error {
            error!(
                target: "rgsm::quick_action::sound",
                "Fallback sound also failed: {fallback_error}"
            );
        }
    }

    pub fn primary_error(&self) -> &str {
        &self.primary_error
    }

    pub fn fallback_error(&self) -> Option<&str> {
        self.fallback_error.as_deref()
    }
}

#[derive(Debug)]
struct PendingPlayback {
    stream: OutputStream,
    sink: Sink,
}

impl PendingPlayback {
    fn control_sink(&self) -> Sink {
        self.sink.clone()
    }

    fn stop(&mut self) {
        self.sink.stop();
    }

    fn wait(self) {
        self.sink.sleep_until_end();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackOrigin {
    Preview(QuickActionSoundEvent),
    Event(QuickActionSoundEvent),
}

#[derive(Debug)]
struct ActivePlayback {
    id: u64,
    origin: PlaybackOrigin,
    sink: Sink,
    cancelled: Arc<AtomicBool>,
    join_handle: async_runtime::JoinHandle<()>,
}

struct PlaybackManager {
    state: Mutex<Option<ActivePlayback>>,
    next_id: AtomicU64,
}

impl PlaybackManager {
    fn new() -> Self {
        Self {
            state: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    async fn replace_playback(
        self: &Arc<Self>,
        origin: PlaybackOrigin,
        event: QuickActionSoundEvent,
        variant: QuickActionSoundVariant,
    ) -> Result<PlaybackStart, PlaybackError> {
        self.stop_current().await;
        self.start_playback(origin, event, variant).await
    }

    async fn toggle_preview(
        self: &Arc<Self>,
        event: QuickActionSoundEvent,
        variant: QuickActionSoundVariant,
    ) -> Result<PreviewSoundStatus, PlaybackError> {
        if self.is_preview_running(event).await {
            self.stop_current().await;
            Ok(PreviewSoundStatus::Stopped)
        } else {
            let start = self
                .replace_playback(PlaybackOrigin::Preview(event), event, variant)
                .await?;
            let fallback_warning = match &start {
                PlaybackStart::Primary => None,
                PlaybackStart::FallbackUsed { primary_error } => {
                    warn!(
                        target: "rgsm::quick_action::sound",
                        "Failed to play selected sound variant, falling back to default: {primary_error}"
                    );
                    Some(format!(
                        "Failed to play the selected sound: {primary_error}"
                    ))
                }
            };
            Ok(PreviewSoundStatus::Started { fallback_warning })
        }
    }

    async fn is_preview_running(&self, event: QuickActionSoundEvent) -> bool {
        let state = self.state.lock().await;
        matches!(
            state.as_ref().map(|active| active.origin),
            Some(PlaybackOrigin::Preview(active_event)) if active_event == event
        )
    }

    async fn start_playback(
        self: &Arc<Self>,
        origin: PlaybackOrigin,
        event: QuickActionSoundEvent,
        variant: QuickActionSoundVariant,
    ) -> Result<PlaybackStart, PlaybackError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancelled = Arc::new(AtomicBool::new(false));
        let (start_tx, start_rx) = oneshot::channel();

        let manager = Arc::clone(self);
        let join = async_runtime::spawn({
            let cancelled = Arc::clone(&cancelled);
            async move {
                let spawn_result = async_runtime::spawn_blocking(move || {
                    blocking_playback(event, variant, start_tx)
                })
                .await;
                manager
                    .handle_join_result(id, spawn_result, cancelled)
                    .await;
            }
        });

        let start_result = match start_rx.await {
            Ok(result) => result,
            Err(_) => {
                if let Err(err) = join.await {
                    error!(
                        target: "rgsm::quick_action::sound",
                        "Sound playback task failed to start: {err}"
                    );
                }
                return Err(PlaybackError::new(
                    "playback task failed before starting".to_string(),
                    None,
                ));
            }
        };

        match start_result {
            Ok((start_kind, sink_control)) => {
                let mut state = self.state.lock().await;
                *state = Some(ActivePlayback {
                    id,
                    origin,
                    sink: sink_control,
                    cancelled,
                    join_handle: join,
                });
                Ok(start_kind)
            }
            Err(err) => {
                if let Err(join_err) = join.await {
                    error!(
                        target: "rgsm::quick_action::sound",
                        "Sound playback task join failed: {join_err}"
                    );
                }
                Err(err)
            }
        }
    }

    async fn stop_current(&self) -> Option<PlaybackOrigin> {
        let active = {
            let mut state = self.state.lock().await;
            state.take()
        };

        if let Some(active) = active {
            active.cancelled.store(true, Ordering::SeqCst);
            active.sink.stop();
            if let Err(err) = active.join_handle.await {
                error!(
                    target: "rgsm::quick_action::sound",
                    "Failed to join sound playback task: {err}"
                );
            }
            Some(active.origin)
        } else {
            None
        }
    }

    async fn handle_join_result(
        self: Arc<Self>,
        id: u64,
        result: Result<Result<(), PlaybackError>, async_runtime::JoinError>,
        cancelled: Arc<AtomicBool>,
    ) {
        let was_cancelled = cancelled.load(Ordering::SeqCst);

        let mut state = self.state.lock().await;
        let is_current = state.as_ref().map(|active| active.id) == Some(id);
        if is_current {
            *state = None;
        }
        drop(state);

        match result {
            Ok(Ok(())) => {
                if was_cancelled {
                    debug!(
                        target: "rgsm::quick_action::sound",
                        "Sound playback task cancelled"
                    );
                }
            }
            Ok(Err(err)) => {
                if is_current && !was_cancelled {
                    err.log_for_playback("rgsm::quick_action::sound");
                }
            }
            Err(join_err) => {
                error!(
                    target: "rgsm::quick_action::sound",
                    "Sound playback task panicked: {join_err}"
                );
            }
        }
    }
}

static PLAYBACK_MANAGER: OnceLock<Arc<PlaybackManager>> = OnceLock::new();

fn playback_manager() -> Arc<PlaybackManager> {
    Arc::clone(PLAYBACK_MANAGER.get_or_init(|| Arc::new(PlaybackManager::new())))
}

type StartSender = oneshot::Sender<Result<(PlaybackStart, Sink), PlaybackError>>;

fn blocking_playback(
    event: QuickActionSoundEvent,
    variant: QuickActionSoundVariant,
    start_tx: StartSender,
) -> Result<(), PlaybackError> {
    match prepare_playback(event, variant) {
        Ok((pending, start_kind)) => {
            let mut pending = pending;
            let control = pending.control_sink();
            if start_tx.send(Ok((start_kind.clone(), control))).is_err() {
                pending.stop();
                return Ok(());
            }
            pending.wait();
            Ok(())
        }
        Err(err) => {
            let _ = start_tx.send(Err(err.clone()));
            Err(err)
        }
    }
}

fn prepare_playback(
    event: QuickActionSoundEvent,
    variant: QuickActionSoundVariant,
) -> Result<(PendingPlayback, PlaybackStart), PlaybackError> {
    let needs_fallback = !matches!(variant, QuickActionSoundVariant::Default);

    match build_pending(event, variant) {
        Ok(pending) => Ok((pending, PlaybackStart::Primary)),
        Err(primary_err) => {
            let primary_error = format!("{primary_err:?}");

            if !needs_fallback {
                return Err(PlaybackError::new(primary_error, None));
            }

            match build_pending(event, QuickActionSoundVariant::Default) {
                Ok(pending) => Ok((pending, PlaybackStart::FallbackUsed { primary_error })),
                Err(fallback_err) => Err(PlaybackError::new(
                    primary_error,
                    Some(format!("{fallback_err:?}")),
                )),
            }
        }
    }
}

fn build_pending(
    event: QuickActionSoundEvent,
    variant: QuickActionSoundVariant,
) -> AnyResult<PendingPlayback> {
    let (stream, handle) =
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

    Ok(PendingPlayback { stream, sink })
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

    let manager = playback_manager();
    async_runtime::spawn(async move {
        match manager
            .replace_playback(PlaybackOrigin::Event(event), event, variant)
            .await
        {
            Ok(PlaybackStart::Primary) => {}
            Ok(PlaybackStart::FallbackUsed { primary_error }) => {
                warn!(
                    target: "rgsm::quick_action::sound",
                    "Failed to play quick action sound variant, falling back to default: {primary_error}"
                );
            }
            Err(err) => {
                err.log_for_playback("rgsm::quick_action::sound");
            }
        }
    });
}

pub async fn preview_sound(
    event: QuickActionSoundEvent,
    variant: QuickActionSoundVariant,
) -> Result<PreviewSoundStatus, PlaybackError> {
    playback_manager().toggle_preview(event, variant).await
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
