use std::{
    f32::consts::PI,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result as AnyResult};
use log::{error, warn};
use rodio::{Decoder, OutputStream, Sink, buffer::SamplesBuffer};
use tauri::async_runtime;
use tokio::sync::{mpsc, oneshot};

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
pub struct PlaybackError {
    primary: String,
    fallback: Option<String>,
}

impl PlaybackError {
    fn new(primary: String, fallback: Option<String>) -> Self {
        Self { primary, fallback }
    }

    pub fn primary_message(&self) -> &str {
        &self.primary
    }

    pub fn fallback_message(&self) -> Option<&str> {
        self.fallback.as_deref()
    }

    pub fn to_user_message(&self) -> String {
        let mut message = format!("Failed to play the selected sound: {}", self.primary);
        if let Some(fallback) = &self.fallback {
            message.push_str(&format!("\nFallback also failed: {fallback}"));
        }
        message
    }

    pub fn log_event_error(&self) {
        error!(
            target: "rgsm::quick_action::sound",
            "Failed to play quick action sound: {}",
            self.primary
        );
        if let Some(fallback) = &self.fallback {
            error!(
                target: "rgsm::quick_action::sound",
                "Fallback sound also failed: {fallback}"
            );
        }
    }

    pub fn log_preview_error(&self) {
        error!(
            target: "rgsm::quick_action::sound",
            "Failed to preview quick action sound: {}",
            self.primary
        );
        if let Some(fallback) = &self.fallback {
            error!(
                target: "rgsm::quick_action::sound",
                "Preview fallback also failed: {fallback}"
            );
        }
    }
}

#[derive(Debug, Clone)]
enum PlaybackStart {
    Primary,
    Fallback { primary_error: String },
}

#[derive(Debug)]
struct PendingPlayback {
    stream: OutputStream,
    sink: Sink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackOrigin {
    Event,
    Preview(QuickActionSoundEvent),
}

#[derive(Debug)]
struct ActivePlayback {
    origin: PlaybackOrigin,
    cancel_flag: Arc<AtomicBool>,
    join_handle: async_runtime::JoinHandle<()>,
}

enum SoundCommand {
    PlayEvent {
        event: QuickActionSoundEvent,
        variant: QuickActionSoundVariant,
    },
    TogglePreview {
        event: QuickActionSoundEvent,
        variant: QuickActionSoundVariant,
        reply: oneshot::Sender<Result<PreviewSoundStatus, PlaybackError>>,
    },
}

struct SoundWorker {
    receiver: mpsc::UnboundedReceiver<SoundCommand>,
    active: Option<ActivePlayback>,
}

impl SoundWorker {
    fn new(receiver: mpsc::UnboundedReceiver<SoundCommand>) -> Self {
        Self {
            receiver,
            active: None,
        }
    }

    async fn run(mut self) {
        while let Some(cmd) = self.receiver.recv().await {
            self.cleanup_finished().await;
            match cmd {
                SoundCommand::PlayEvent { event, variant } => {
                    self.handle_event(event, variant).await;
                }
                SoundCommand::TogglePreview {
                    event,
                    variant,
                    reply,
                } => {
                    let result = self.handle_preview(event, variant).await;
                    let _ = reply.send(result);
                }
            }
        }

        if let Some(active) = self.active.take() {
            active.cancel_flag.store(true, Ordering::SeqCst);
            if let Err(err) = active.join_handle.await {
                error!(
                    target: "rgsm::quick_action::sound",
                    "Sound playback task failed to shut down: {err}"
                );
            }
        }
    }

    async fn handle_event(
        &mut self,
        event: QuickActionSoundEvent,
        variant: QuickActionSoundVariant,
    ) {
        self.stop_current().await;
        match self
            .start_playback(PlaybackOrigin::Event, event, variant)
            .await
        {
            Ok(PlaybackStart::Primary) => {}
            Ok(PlaybackStart::Fallback { primary_error }) => {
                warn!(
                    target: "rgsm::quick_action::sound",
                    "Failed to play selected sound, falling back to default: {primary_error}"
                );
            }
            Err(err) => {
                err.log_event_error();
            }
        }
    }

    async fn handle_preview(
        &mut self,
        event: QuickActionSoundEvent,
        variant: QuickActionSoundVariant,
    ) -> Result<PreviewSoundStatus, PlaybackError> {
        if let Some(active) = &self.active {
            if matches!(active.origin, PlaybackOrigin::Preview(prev) if prev == event) {
                self.stop_current().await;
                return Ok(PreviewSoundStatus::Stopped);
            }
        }

        self.stop_current().await;
        match self
            .start_playback(PlaybackOrigin::Preview(event), event, variant)
            .await
        {
            Ok(PlaybackStart::Primary) => Ok(PreviewSoundStatus::Started {
                fallback_warning: None,
            }),
            Ok(PlaybackStart::Fallback { primary_error }) => {
                warn!(
                    target: "rgsm::quick_action::sound",
                    "Failed to play selected sound, falling back to default: {primary_error}"
                );
                Ok(PreviewSoundStatus::Started {
                    fallback_warning: Some(format!(
                        "Failed to play the selected sound: {primary_error}"
                    )),
                })
            }
            Err(err) => Err(err),
        }
    }

    async fn cleanup_finished(&mut self) {
        let should_join = matches!(
            self.active.as_ref(),
            Some(active) if active.join_handle.is_finished()
        );
        if should_join {
            if let Some(active) = self.active.take() {
                if let Err(err) = active.join_handle.await {
                    error!(
                        target: "rgsm::quick_action::sound",
                        "Sound playback task finished with error: {err}"
                    );
                }
            }
        }
    }

    async fn stop_current(&mut self) {
        if let Some(active) = self.active.take() {
            active.cancel_flag.store(true, Ordering::SeqCst);
            if let Err(err) = active.join_handle.await {
                error!(
                    target: "rgsm::quick_action::sound",
                    "Sound playback task join failed: {err}"
                );
            }
        }
    }

    async fn start_playback(
        &mut self,
        origin: PlaybackOrigin,
        event: QuickActionSoundEvent,
        variant: QuickActionSoundVariant,
    ) -> Result<PlaybackStart, PlaybackError> {
        let (start_tx, start_rx) = oneshot::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let join_handle = async_runtime::spawn_blocking({
            let cancel_flag = Arc::clone(&cancel_flag);
            move || blocking_playback(event, variant, start_tx, cancel_flag)
        });

        match start_rx.await {
            Ok(Ok(start_kind)) => {
                self.active = Some(ActivePlayback {
                    origin,
                    cancel_flag,
                    join_handle,
                });
                Ok(start_kind)
            }
            Ok(Err(err)) => {
                cancel_flag.store(true, Ordering::SeqCst);
                if let Err(join_err) = join_handle.await {
                    error!(
                        target: "rgsm::quick_action::sound",
                        "Sound playback task failed to start: {join_err}"
                    );
                }
                Err(err)
            }
            Err(_) => {
                cancel_flag.store(true, Ordering::SeqCst);
                if let Err(join_err) = join_handle.await {
                    error!(
                        target: "rgsm::quick_action::sound",
                        "Sound playback task panicked: {join_err}"
                    );
                }
                Err(PlaybackError::new(
                    "playback task failed before starting".to_string(),
                    None,
                ))
            }
        }
    }
}

static SOUND_CHANNEL: OnceLock<mpsc::UnboundedSender<SoundCommand>> = OnceLock::new();

fn sound_sender() -> mpsc::UnboundedSender<SoundCommand> {
    SOUND_CHANNEL
        .get_or_init(|| {
            let (sender, receiver) = mpsc::unbounded_channel();
            let worker = SoundWorker::new(receiver);
            async_runtime::spawn(worker.run());
            sender
        })
        .clone()
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

    if let Err(err) = sound_sender().send(SoundCommand::PlayEvent { event, variant }) {
        error!(
            target: "rgsm::quick_action::sound",
            "Failed to queue sound playback: {err}"
        );
    }
}

pub async fn preview_sound(
    event: QuickActionSoundEvent,
    variant: QuickActionSoundVariant,
) -> Result<PreviewSoundStatus, PlaybackError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    if sound_sender()
        .send(SoundCommand::TogglePreview {
            event,
            variant,
            reply: reply_tx,
        })
        .is_err()
    {
        return Err(PlaybackError::new(
            "audio worker is not available".to_string(),
            None,
        ));
    }

    match reply_rx.await {
        Ok(result) => result,
        Err(_) => Err(PlaybackError::new(
            "audio worker closed unexpectedly".to_string(),
            None,
        )),
    }
}

fn blocking_playback(
    event: QuickActionSoundEvent,
    variant: QuickActionSoundVariant,
    start_tx: oneshot::Sender<Result<PlaybackStart, PlaybackError>>,
    cancel_flag: Arc<AtomicBool>,
) {
    match prepare_playback(event, variant) {
        Ok((pending, start_kind)) => {
            if start_tx.send(Ok(start_kind)).is_ok() {
                play_pending(pending, cancel_flag);
            }
        }
        Err(err) => {
            let _ = start_tx.send(Err(err));
        }
    }
}

fn prepare_playback(
    event: QuickActionSoundEvent,
    variant: QuickActionSoundVariant,
) -> Result<(PendingPlayback, PlaybackStart), PlaybackError> {
    match build_pending(event, &variant) {
        Ok(pending) => Ok((pending, PlaybackStart::Primary)),
        Err(primary_err) => {
            let primary_message = format!("{primary_err:?}");
            if matches!(variant, QuickActionSoundVariant::Default) {
                return Err(PlaybackError::new(primary_message, None));
            }

            match build_pending(event, &QuickActionSoundVariant::Default) {
                Ok(pending) => Ok((
                    pending,
                    PlaybackStart::Fallback {
                        primary_error: primary_message,
                    },
                )),
                Err(fallback_err) => Err(PlaybackError::new(
                    primary_message,
                    Some(format!("{fallback_err:?}")),
                )),
            }
        }
    }
}

fn build_pending(
    event: QuickActionSoundEvent,
    variant: &QuickActionSoundVariant,
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
            let resolved = resolve_audio_path(Path::new(path));
            let file = File::open(&resolved)
                .with_context(|| format!("failed to open audio file at {}", resolved.display()))?;
            let reader = BufReader::new(file);
            let decoder = Decoder::new(reader).context("failed to decode audio file")?;
            sink.append(decoder);
        }
    }

    Ok(PendingPlayback { stream, sink })
}

fn play_pending(pending: PendingPlayback, cancel_flag: Arc<AtomicBool>) {
    let PendingPlayback { stream, mut sink } = pending;

    while !cancel_flag.load(Ordering::SeqCst) && !sink.empty() {
        std::thread::sleep(Duration::from_millis(20));
    }

    if cancel_flag.load(Ordering::SeqCst) {
        sink.stop();
    } else {
        sink.sleep_until_end();
    }

    drop(stream);
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
