use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub enum TrayEvent {
    OpenSettings,
    Quit,
}

pub struct Tray {
    _icon: TrayIcon,
}

pub fn spawn(tx: Sender<TrayEvent>) -> Result<Tray> {
    let menu = Menu::new();

    let open = MenuItem::new("Open Settings", true, None);
    let separator = PredefinedMenuItem::separator();
    let quit = MenuItem::new("Quit Whispr", true, None);

    let open_id = open.id().clone();
    let quit_id = quit.id().clone();

    menu.append_items(&[&open, &separator, &quit])
        .context("build tray menu")?;

    let icon = TrayIconBuilder::new()
        .with_tooltip("Whispr")
        .with_icon(build_icon())
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(true)
        .build()
        .context("build tray icon")?;

    std::thread::Builder::new()
        .name("whispr-tray".into())
        .spawn(move || {
            let receiver = MenuEvent::receiver();
            while let Ok(event) = receiver.recv() {
                let send = if event.id == open_id {
                    tx.send(TrayEvent::OpenSettings)
                } else if event.id == quit_id {
                    tx.send(TrayEvent::Quit)
                } else {
                    Ok(())
                };
                if send.is_err() {
                    break;
                }
            }
        })?;

    Ok(Tray { _icon: icon })
}

fn build_icon() -> Icon {
    let size = 22u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let cx = size as f32 / 2.0;
    let cy = size as f32 / 2.0;
    let r_outer = size as f32 / 2.0 - 1.0;
    let r_inner = r_outer - 4.0;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            let idx = ((y * size + x) * 4) as usize;
            if d <= r_outer && d >= r_inner {
                rgba[idx] = 0xf9;
                rgba[idx + 1] = 0xfa;
                rgba[idx + 2] = 0xfb;
                rgba[idx + 3] = 0xff;
            } else if d <= r_inner - 2.0 {
                rgba[idx] = 0xef;
                rgba[idx + 1] = 0x44;
                rgba[idx + 2] = 0x44;
                rgba[idx + 3] = 0xff;
            }
        }
    }

    Icon::from_rgba(rgba, size, size).expect("valid icon rgba")
}
