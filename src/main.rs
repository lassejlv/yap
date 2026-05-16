mod assets;
mod audio;
mod config;
mod core;
mod history;
mod hotkey;
mod model;
mod paste;
mod state;
mod stt;
mod text;
mod tray;
mod ui;
mod updater;

use crossbeam_channel::{Receiver, unbounded};
use gpui::{
    AppContext, AsyncApp, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions, point, px, size,
};
use gpui_component::Root;
use gpui_component::theme::{Theme, ThemeMode};
use std::time::Duration;
use tracing_subscriber::EnvFilter;

use crate::assets::Assets;
use crate::config::Settings;
use crate::state::{Bus, UiCmd};
use crate::ui::{AppModel, PillView, SettingsView, spawn_event_pump};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let settings = Settings::load().shared();
    let bus = Bus::new();

    let (hot_tx, hot_rx) = unbounded();
    hotkey::spawn(hot_tx, settings.clone());

    core::spawn(
        hot_rx,
        bus.core_cmd_rx.clone(),
        bus.core_ev_tx.clone(),
        settings.clone(),
    );

    updater::spawn(bus.core_ev_tx.clone());

    let core_ev_rx = bus.core_ev_rx.clone();
    let core_cmd_tx = bus.core_cmd_tx.clone();
    let ui_cmd_tx_for_tray = bus.ui_cmd_tx.clone();
    let ui_cmd_rx = bus.ui_cmd_rx.clone();

    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            gpui_component::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);

            // NSStatusBar must be touched only after GPUI installs its
            // NSApplication subclass — otherwise legacy `objc` panics trying
            // to add an ivar to an already-instantiated class.
            tray::install(ui_cmd_tx_for_tray);

            let app = cx.new(|_| AppModel::new(core_cmd_tx, settings.clone()));
            spawn_event_pump(app.clone(), core_ev_rx, cx);

            let pill_app = app.clone();

            cx.spawn(async move |cx| {
                open_pill(&pill_app, cx).await;
                pump_ui_cmds(app.clone(), ui_cmd_rx, cx).await;
            })
            .detach();
        });
}

async fn open_pill(app: &gpui::Entity<AppModel>, cx: &mut AsyncApp) {
    let window_size = size(px(384.0), px(80.0));
    let bounds = cx.update(|cx| {
        cx.primary_display()
            .map(|display| {
                let display = display.visible_bounds();
                let margin = px(28.0);
                Bounds {
                    origin: point(
                        display.origin.x + (display.size.width - window_size.width) / 2.0,
                        display.origin.y + display.size.height - window_size.height - margin,
                    ),
                    size: window_size,
                }
            })
            .unwrap_or(Bounds {
                origin: point(px(40.0), px(40.0)),
                size: window_size,
            })
    });
    let app = app.clone();
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            kind: WindowKind::PopUp,
            titlebar: None,
            is_movable: true,
            is_resizable: false,
            is_minimizable: false,
            focus: false,
            show: true,
            window_background: WindowBackgroundAppearance::Blurred,
            ..Default::default()
        },
        move |_window, cx| cx.new(|cx| PillView::new(app.clone(), cx)),
    );
}

async fn open_settings(app: &gpui::Entity<AppModel>, cx: &mut AsyncApp) {
    let bounds = Bounds {
        origin: point(px(360.0), px(120.0)),
        size: size(px(720.0), px(560.0)),
    };
    let app = app.clone();
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            kind: WindowKind::Normal,
            titlebar: Some(TitlebarOptions {
                title: Some("yap".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(14.0), px(14.0))),
            }),
            window_background: WindowBackgroundAppearance::Blurred,
            is_movable: true,
            is_resizable: true,
            is_minimizable: true,
            window_min_size: Some(size(px(620.0), px(440.0))),
            ..Default::default()
        },
        move |window, cx| {
            let v = cx.new(|cx| SettingsView::new(app.clone(), cx));
            cx.new(|cx| Root::new(v, window, cx))
        },
    );
}

async fn pump_ui_cmds(app: gpui::Entity<AppModel>, rx: Receiver<UiCmd>, cx: &mut AsyncApp) {
    loop {
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                UiCmd::OpenSettings => open_settings(&app, cx).await,
                UiCmd::Hide => {
                    let _ = cx.update(|cx| cx.hide());
                }
                UiCmd::Quit => {
                    let _ = cx.update(|cx| cx.quit());
                    return;
                }
            }
        }
    }
}
