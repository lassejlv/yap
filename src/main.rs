mod app;
mod audio;
mod config;
mod hotkey;
mod llm;
mod output;
mod storage;
mod stt;
mod tray;
mod ui;

use anyhow::Result;
use crossbeam_channel::unbounded;
use tokio::runtime::Runtime;
use tracing_subscriber::EnvFilter;

use crate::app::AppHandle;
use crate::config::Config;
use crate::storage::Store;

fn main() -> Result<()> {
    let _ = dotenv_load();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("whispr=info")),
        )
        .init();

    let cfg = Config::load().unwrap_or_else(|e| {
        tracing::warn!(error = ?e, "config load failed, using defaults");
        Config::default()
    });

    let runtime = Runtime::new()?;
    let rt_handle = runtime.handle().clone();

    let store = runtime.block_on(Store::open())?;

    let (hotkey_tx, hotkey_rx) = unbounded();
    let (ui_tx, ui_rx) = unbounded();
    let (tray_tx, tray_rx) = unbounded();

    let handle = AppHandle::new(cfg, ui_tx, store, rt_handle.clone());

    hotkey::spawn(hotkey_tx);

    {
        let handle = handle.clone();
        let rt_handle = rt_handle.clone();
        std::thread::Builder::new()
            .name("whispr-pipeline".into())
            .spawn(move || app::run_event_loop(handle, hotkey_rx, rt_handle))
            .expect("spawn pipeline thread");
    }

    gpui_platform::application().run(move |cx| {
        ui::boot(cx, handle.clone(), ui_rx.clone(), tray_rx.clone(), tray_tx.clone());
    });

    drop(runtime);
    Ok(())
}

fn dotenv_load() -> std::io::Result<()> {
    let path = std::path::Path::new(".env");
    if !path.exists() {
        return Ok(());
    }
    let raw = std::fs::read_to_string(path)?;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            if std::env::var_os(k.trim()).is_none() {
                std::env::set_var(k.trim(), v);
            }
        }
    }
    Ok(())
}
