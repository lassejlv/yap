pub mod pill;
pub mod settings;
pub mod theme;

use crossbeam_channel::{Receiver, Sender};
use gpui::{
    actions, px, App, AppContext, Bounds, KeyBinding, Menu, MenuItem, Pixels, Point, Size,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
};
use std::time::Duration;

use crate::app::{AppHandle, AppState};
use crate::tray;
use crate::tray::TrayEvent;

actions!(whispr, [Quit]);

const PILL_WIDTH: f32 = 420.0;
const PILL_HEIGHT: f32 = 48.0;
const BOTTOM_PADDING: f32 = 36.0;

pub fn boot(
    cx: &mut App,
    handle: AppHandle,
    ui_rx: Receiver<AppState>,
    tray_rx: Receiver<TrayEvent>,
    tray_tx: Sender<TrayEvent>,
) {
    theme::init(cx);

    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    cx.on_action::<Quit>(|_, cx| cx.quit());
    cx.set_menus(vec![Menu::new("Whispr").items([
        MenuItem::action("Open Settings", settings::OpenSettings),
        MenuItem::separator(),
        MenuItem::action("Quit Whispr", Quit),
    ])]);

    match tray::spawn(tray_tx) {
        Ok(t) => std::mem::forget(t),
        Err(e) => tracing::error!(error = ?e, "tray init failed"),
    }

    let bounds = pill_bounds(cx);
    let pill_handle = handle.clone();
    let pill_rx = ui_rx;

    cx.open_window(
        WindowOptions {
            kind: WindowKind::PopUp,
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            is_movable: true,
            is_resizable: false,
            focus: false,
            show: true,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        move |_window, cx| cx.new(|cx| pill::Pill::new(pill_handle.clone(), pill_rx.clone(), cx)),
    )
    .expect("open pill window");

    let tray_handle = handle.clone();
    let bg = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        loop {
            bg.timer(Duration::from_millis(100)).await;
            while let Ok(event) = tray_rx.try_recv() {
                match event {
                    TrayEvent::OpenSettings => {
                        let h = tray_handle.clone();
                        cx.update(|cx| settings::open(h, cx));
                    }
                    TrayEvent::Quit => {
                        cx.update(|cx| cx.quit());
                    }
                }
            }
        }
    })
    .detach();

    cx.on_action::<settings::OpenSettings>({
        let handle = handle.clone();
        move |_action, cx| settings::open(handle.clone(), cx)
    });
}

fn pill_bounds(cx: &App) -> Bounds<Pixels> {
    let display = cx.primary_display();
    let screen = display
        .map(|d| d.bounds())
        .unwrap_or_else(|| Bounds {
            origin: Point::new(px(0.0), px(0.0)),
            size: Size {
                width: px(1440.0),
                height: px(900.0),
            },
        });

    let width = px(PILL_WIDTH);
    let height = px(PILL_HEIGHT);
    let pad = px(BOTTOM_PADDING);

    let origin = Point::new(
        screen.origin.x + (screen.size.width - width) / 2.0,
        screen.origin.y + screen.size.height - height - pad,
    );

    Bounds {
        origin,
        size: Size { width, height },
    }
}
