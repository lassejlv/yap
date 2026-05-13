use anyhow::{anyhow, Result};
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
};
use crossbeam_channel::Sender;
use std::cell::Cell;

use super::HotkeyEvent;

const NX_SECONDARYFNMASK: u64 = 0x80_0000;

pub fn run_event_tap(tx: Sender<HotkeyEvent>) -> Result<()> {
    let last_fn_down = Cell::new(false);

    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::FlagsChanged],
        |_proxy, _evt_type, event| {
            let fn_down = (event.get_flags().bits() & NX_SECONDARYFNMASK) != 0;
            if fn_down != last_fn_down.get() {
                last_fn_down.set(fn_down);
                let kind = if fn_down {
                    HotkeyEvent::Pressed
                } else {
                    HotkeyEvent::Released
                };
                if tx.send(kind).is_err() {
                    tracing::warn!("hotkey receiver dropped");
                }
            }
            None
        },
    )
    .map_err(|_| {
        anyhow!("failed to install CGEventTap — grant Accessibility permission in System Settings")
    })?;

    let source = tap
        .mach_port
        .create_runloop_source(0)
        .map_err(|_| anyhow!("failed to create CFRunLoopSource from CGEventTap"))?;

    CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
    tap.enable();

    tracing::info!("Fn-key event tap installed; entering CFRunLoop");
    CFRunLoop::run_current();
    Ok(())
}
