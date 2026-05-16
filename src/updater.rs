//! GitHub releases poller — surfaces "newer version available" to the UI.
//!
//! Polls on startup and every `POLL_INTERVAL`. Best-effort: any failure is
//! logged at debug and silently retried later, never user-visible.

use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use serde::Deserialize;
use std::thread;
use std::time::Duration;

use crate::state::CoreEvent;

const API_URL: &str = "https://api.github.com/repos/lassejlv/yap/releases/latest";
const POLL_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

pub fn spawn(ev_tx: Sender<CoreEvent>) {
    thread::Builder::new()
        .name("yap-updater".into())
        .spawn(move || run(ev_tx))
        .expect("spawn updater");
}

fn run(ev_tx: Sender<CoreEvent>) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::warn!("updater runtime failed: {e:#}");
            return;
        }
    };

    rt.block_on(async move {
        loop {
            if let Err(e) = check(&ev_tx).await {
                tracing::debug!("update check failed: {e:#}");
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

async fn check(ev_tx: &Sender<CoreEvent>) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("yap-updater/", env!("CARGO_PKG_VERSION")))
        .timeout(REQUEST_TIMEOUT)
        .build()?;

    let body = client
        .get(API_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("GET releases/latest")?
        .error_for_status()?
        .text()
        .await
        .context("read release body")?;
    let rel: GhRelease = serde_json::from_str(&body).context("parse release JSON")?;

    if rel.draft || rel.prerelease {
        return Ok(());
    }

    let latest = rel.tag_name.strip_prefix('v').unwrap_or(&rel.tag_name);
    let current = env!("CARGO_PKG_VERSION");
    if version_tuple(latest) > version_tuple(current) {
        let _ = ev_tx.send(CoreEvent::UpdateAvailable(UpdateInfo {
            version: latest.to_string(),
            url: rel.html_url,
        }));
    }
    Ok(())
}

fn version_tuple(s: &str) -> (u32, u32, u32) {
    let mut it = s.split('.').map(|p| {
        // tolerate suffixes like "0.1.0-rc1" by truncating at non-digits
        let digits: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse::<u32>().unwrap_or(0)
    });
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

/// Best-effort URL opener (macOS `open` shells out).
pub fn open_url(url: &str) {
    let _ = std::process::Command::new("open").arg(url).spawn();
}

#[cfg(test)]
mod tests {
    use super::version_tuple;

    #[test]
    fn parses_basic() {
        assert_eq!(version_tuple("0.1.0"), (0, 1, 0));
        assert_eq!(version_tuple("1.2.3"), (1, 2, 3));
    }

    #[test]
    fn parses_partial() {
        assert_eq!(version_tuple("1"), (1, 0, 0));
        assert_eq!(version_tuple("1.2"), (1, 2, 0));
    }

    #[test]
    fn parses_with_suffix() {
        assert_eq!(version_tuple("0.2.0-rc1"), (0, 2, 0));
    }

    #[test]
    fn ordering() {
        assert!(version_tuple("0.2.0") > version_tuple("0.1.9"));
        assert!(version_tuple("1.0.0") > version_tuple("0.99.0"));
        assert!(version_tuple("0.1.0") == version_tuple("0.1.0"));
    }
}
