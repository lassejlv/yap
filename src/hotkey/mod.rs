use crossbeam_channel::Sender;

#[derive(Clone, Copy, Debug)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub fn spawn(tx: Sender<HotkeyEvent>) {
    std::thread::Builder::new()
        .name("whispr-hotkey".into())
        .spawn(move || {
            if let Err(e) = macos::run_event_tap(tx) {
                tracing::error!(error = ?e, "hotkey tap exited");
            }
        })
        .expect("spawn hotkey thread");
}

#[cfg(not(target_os = "macos"))]
pub fn spawn(_tx: Sender<HotkeyEvent>) {
    tracing::warn!("hotkey backend only implemented on macOS");
}
