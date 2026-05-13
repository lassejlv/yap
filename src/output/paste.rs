use anyhow::{Context, Result};
use arboard::Clipboard;
use std::thread;
use std::time::Duration;

#[cfg(target_os = "macos")]
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
#[cfg(target_os = "macos")]
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

const KEY_V: u16 = 0x09;

pub fn paste(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new().context("init clipboard")?;
    let previous = clipboard.get_text().ok();

    clipboard.set_text(text.to_string()).context("write clipboard")?;
    thread::sleep(Duration::from_millis(30));

    #[cfg(target_os = "macos")]
    post_cmd_v()?;

    let restore_text = previous;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(250));
        if let Ok(mut cb) = Clipboard::new() {
            if let Some(prev) = restore_text {
                let _ = cb.set_text(prev);
            }
        }
    });

    Ok(())
}

#[cfg(target_os = "macos")]
fn post_cmd_v() -> Result<()> {
    let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| anyhow::anyhow!("failed to create event source"))?;

    let down = CGEvent::new_keyboard_event(src.clone(), KEY_V as CGKeyCode, true)
        .map_err(|_| anyhow::anyhow!("keyboard event down"))?;
    down.set_flags(CGEventFlags::CGEventFlagCommand);
    down.post(CGEventTapLocation::HID);

    let up = CGEvent::new_keyboard_event(src, KEY_V as CGKeyCode, false)
        .map_err(|_| anyhow::anyhow!("keyboard event up"))?;
    up.set_flags(CGEventFlags::CGEventFlagCommand);
    up.post(CGEventTapLocation::HID);

    Ok(())
}
