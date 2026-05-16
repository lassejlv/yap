//! Cross-thread event bus + UI-facing state.

use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::history::RecordingSummary;
use crate::updater::UpdateInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    LoadingModel,
    Idle,
    Recording,
    Transcribing,
    NeedsModel,
}

#[derive(Debug, Clone)]
pub enum CoreEvent {
    PhaseChanged(Phase),
    Level(f32),
    Transcript(String),
    History(Vec<RecordingSummary>),
    DownloadProgress { bytes: u64, total: Option<u64> },
    DownloadExtracting,
    DownloadDone,
    DownloadFailed(String),
    Log(String),
    UpdateAvailable(UpdateInfo),
}

/// Commands to the core worker thread (audio/STT/download).
#[derive(Debug, Clone)]
pub enum CoreCmd {
    StartDownload,
    ReloadModel,
}

/// Commands to the UI thread (windowing).
#[derive(Debug, Clone)]
pub enum UiCmd {
    OpenSettings,
    Hide,
    Quit,
}

#[derive(Clone)]
pub struct Bus {
    pub core_ev_tx: Sender<CoreEvent>,
    pub core_ev_rx: Receiver<CoreEvent>,
    pub core_cmd_tx: Sender<CoreCmd>,
    pub core_cmd_rx: Receiver<CoreCmd>,
    pub ui_cmd_tx: Sender<UiCmd>,
    pub ui_cmd_rx: Receiver<UiCmd>,
}

impl Bus {
    pub fn new() -> Self {
        let (core_ev_tx, core_ev_rx) = unbounded();
        let (core_cmd_tx, core_cmd_rx) = unbounded();
        let (ui_cmd_tx, ui_cmd_rx) = unbounded();
        Self {
            core_ev_tx,
            core_ev_rx,
            core_cmd_tx,
            core_cmd_rx,
            ui_cmd_tx,
            ui_cmd_rx,
        }
    }
}
