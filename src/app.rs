use anyhow::Result;
use chrono::Utc;
use crossbeam_channel::{Receiver, Sender};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::runtime::Handle;

use crate::audio::{wav_bytes, AudioLevels, AudioRecorder, TARGET_SAMPLE_RATE};
use crate::config::{Config, OutputMode};
use crate::hotkey::HotkeyEvent;
use crate::llm::LlmClient;
use crate::output;
use crate::storage::{Recording, Store};
use crate::stt::SttClient;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Recording,
    Transcribing,
    Cleaning,
    Outputting,
}

#[derive(Clone, Debug, Default)]
pub struct AppState {
    pub phase: Phase,
    pub last_transcript: String,
    pub last_error: Option<String>,
}

#[derive(Clone)]
pub struct AppHandle {
    inner: Arc<Inner>,
}

struct Inner {
    state: RwLock<AppState>,
    config: RwLock<Config>,
    ui_tx: Sender<AppState>,
    levels: Arc<AudioLevels>,
    store: Store,
    rt: Handle,
}

impl AppHandle {
    pub fn new(config: Config, ui_tx: Sender<AppState>, store: Store, rt: Handle) -> Self {
        Self {
            inner: Arc::new(Inner {
                state: RwLock::new(AppState::default()),
                config: RwLock::new(config),
                ui_tx,
                levels: AudioLevels::new(),
                store,
                rt,
            }),
        }
    }

    pub fn rt(&self) -> Handle {
        self.inner.rt.clone()
    }

    pub fn snapshot(&self) -> AppState {
        self.inner.state.read().clone()
    }

    pub fn config(&self) -> Config {
        self.inner.config.read().clone()
    }

    pub fn levels(&self) -> Arc<AudioLevels> {
        self.inner.levels.clone()
    }

    pub fn store(&self) -> Store {
        self.inner.store.clone()
    }

    pub fn update_config(&self, new_cfg: Config) -> Result<()> {
        new_cfg.save()?;
        *self.inner.config.write() = new_cfg;
        Ok(())
    }

    fn set_state(&self, mutator: impl FnOnce(&mut AppState)) {
        let next = {
            let mut g = self.inner.state.write();
            mutator(&mut g);
            g.clone()
        };
        let _ = self.inner.ui_tx.try_send(next);
    }
}

pub fn run_event_loop(handle: AppHandle, rx: Receiver<HotkeyEvent>, rt: Handle) {
    let mut recorder: Option<AudioRecorder> = None;

    while let Ok(event) = rx.recv() {
        match event {
            HotkeyEvent::Pressed => {
                if recorder.is_some() {
                    continue;
                }
                match AudioRecorder::start(handle.levels()) {
                    Ok(r) => {
                        recorder = Some(r);
                        handle.set_state(|s| {
                            s.phase = Phase::Recording;
                            s.last_error = None;
                        });
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "audio start failed");
                        handle.set_state(|s| {
                            s.phase = Phase::Idle;
                            s.last_error = Some(format!("mic error: {e}"));
                        });
                    }
                }
            }
            HotkeyEvent::Released => {
                let Some(rec) = recorder.take() else { continue };
                let samples = match rec.stop() {
                    Ok(s) => s,
                    Err(e) => {
                        handle.set_state(|s| {
                            s.phase = Phase::Idle;
                            s.last_error = Some(format!("audio stop: {e}"));
                        });
                        continue;
                    }
                };
                if samples.len() < 1600 {
                    handle.set_state(|s| s.phase = Phase::Idle);
                    continue;
                }
                let handle_cloned = handle.clone();
                rt.spawn(async move {
                    if let Err(e) = pipeline(handle_cloned.clone(), samples).await {
                        tracing::error!(error = ?e, "pipeline failed");
                        handle_cloned.set_state(|s| {
                            s.phase = Phase::Idle;
                            s.last_error = Some(e.to_string());
                        });
                    }
                });
            }
        }
    }
}

async fn pipeline(handle: AppHandle, samples: Vec<i16>) -> Result<()> {
    let cfg = handle.config();
    let store = handle.store();
    let duration_ms = (samples.len() as i64 * 1000) / TARGET_SAMPLE_RATE as i64;

    handle.set_state(|s| s.phase = Phase::Transcribing);
    let wav = wav_bytes(&samples)?;

    let (rec_id, wav_path) = store.new_recording_path();
    let wav_path_str = wav_path.display().to_string();
    let wav_for_disk = wav.clone();
    tokio::task::spawn_blocking(move || std::fs::write(&wav_path, wav_for_disk)).await??;

    let stt = SttClient::new(cfg.openai_api_key.clone(), cfg.stt_model.clone());
    let raw = stt.transcribe(wav, None).await?;

    let cleaned = if cfg.cleanup_enabled && !raw.is_empty() {
        handle.set_state(|s| s.phase = Phase::Cleaning);
        let llm = LlmClient::new(cfg.openai_api_key.clone(), cfg.cleanup_model.clone());
        llm.cleanup(&cfg.cleanup_prompt, &raw, &cfg.vocabulary)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(error = ?e, "cleanup failed, using raw transcript");
                raw.clone()
            })
    } else {
        raw.clone()
    };

    let final_text = if !cleaned.is_empty() { cleaned.clone() } else { raw.clone() };

    handle.set_state(|s| {
        s.phase = Phase::Outputting;
        s.last_transcript = final_text.clone();
    });
    deliver_blocking(cfg.output_mode, final_text).await?;

    let recording = Recording {
        id: rec_id,
        created_at: Utc::now(),
        duration_ms,
        raw_text: raw,
        cleaned_text: cleaned,
        wav_path: wav_path_str,
    };
    if let Err(e) = store.insert(&recording).await {
        tracing::warn!(error = ?e, "store insert failed");
    }

    handle.set_state(|s| s.phase = Phase::Idle);
    Ok(())
}

async fn deliver_blocking(mode: OutputMode, text: String) -> Result<()> {
    tokio::task::spawn_blocking(move || output::deliver(mode, &text)).await??;
    Ok(())
}
