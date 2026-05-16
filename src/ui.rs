//! GPUI views: pill HUD + native-feel settings window.

#[path = "ui/skeleton.rs"]
mod skeleton;
#[path = "ui/theme.rs"]
mod theme;

use crossbeam_channel::{Receiver, Sender};
use gpui::{
    AnyElement, Context, Entity, Hsla, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Window, div, hsla, px,
};
use gpui_component::{
    Disableable,
    button::{Button, ButtonVariants},
    h_flex, v_flex,
};
use std::collections::VecDeque;
use std::time::Duration;

use crate::config::{Hotkey, SharedSettings, parakeet_dir};
use crate::history::{RecordingSummary, history_path};
use crate::state::{CoreCmd, CoreEvent, Phase};
use crate::updater::{self, UpdateInfo};

const VU_BARS: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    General,
    Model,
    History,
    About,
}

impl SettingsSection {
    const ALL: [Self; 4] = [Self::General, Self::Model, Self::History, Self::About];

    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Model => "Model",
            Self::History => "History",
            Self::About => "About",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::General => "Input behavior and shortcuts.",
            Self::Model => "Local speech recognition model.",
            Self::History => "Recent dictation sessions.",
            Self::About => "Build information.",
        }
    }
}

pub struct AppModel {
    pub phase: Phase,
    pub level: f32,
    pub levels: VecDeque<f32>,
    pub last_transcript: String,
    pub log: String,
    pub download_bytes: u64,
    pub download_total: Option<u64>,
    pub downloading: bool,
    pub extracting: bool,
    pub history: Vec<RecordingSummary>,
    pub settings: SharedSettings,
    pub section: SettingsSection,
    pub update_available: Option<UpdateInfo>,
    core_cmd_tx: Sender<CoreCmd>,
}

impl AppModel {
    pub fn new(core_cmd_tx: Sender<CoreCmd>, settings: SharedSettings) -> Self {
        Self {
            phase: Phase::LoadingModel,
            level: 0.0,
            levels: VecDeque::from(vec![0.0; VU_BARS]),
            last_transcript: String::new(),
            log: String::new(),
            download_bytes: 0,
            download_total: None,
            downloading: false,
            extracting: false,
            history: Vec::new(),
            settings,
            section: SettingsSection::General,
            update_available: None,
            core_cmd_tx,
        }
    }

    pub fn apply(&mut self, ev: CoreEvent) {
        match ev {
            CoreEvent::PhaseChanged(p) => {
                if !matches!(p, Phase::Recording) {
                    self.levels.iter_mut().for_each(|v| *v = 0.0);
                }
                self.phase = p;
            }
            CoreEvent::Level(l) => {
                self.level = l;
                self.levels.pop_back();
                self.levels.push_front(l);
            }
            CoreEvent::Transcript(t) => self.last_transcript = t,
            CoreEvent::History(rows) => self.history = rows,
            CoreEvent::DownloadProgress { bytes, total } => {
                self.downloading = true;
                self.download_bytes = bytes;
                self.download_total = total;
            }
            CoreEvent::DownloadExtracting => {
                self.downloading = false;
                self.extracting = true;
            }
            CoreEvent::DownloadDone => {
                self.downloading = false;
                self.extracting = false;
                self.phase = Phase::Idle;
                let _ = self.core_cmd_tx.send(CoreCmd::ReloadModel);
            }
            CoreEvent::DownloadFailed(e) => {
                self.downloading = false;
                self.extracting = false;
                self.log = format!("download failed: {e}");
            }
            CoreEvent::Log(s) => self.log = s,
            CoreEvent::UpdateAvailable(info) => {
                tracing::info!("update available: {} ({})", info.version, info.url);
                self.update_available = Some(info);
            }
        }
    }

    pub fn start_download(&self) {
        let _ = self.core_cmd_tx.send(CoreCmd::StartDownload);
    }

    pub fn set_hotkey(&mut self, hotkey: Hotkey) {
        let result = {
            let mut settings = self.settings.write();
            settings.hotkey = hotkey;
            settings.save()
        };
        if let Err(e) = result {
            self.log = format!("settings save failed: {e}");
        } else {
            self.log = format!("Hotkey set to {}", hotkey.label());
        }
    }

    pub fn set_trailing_space(&mut self, enabled: bool) {
        let result = {
            let mut settings = self.settings.write();
            settings.trailing_space = enabled;
            settings.save()
        };
        if let Err(e) = result {
            self.log = format!("settings save failed: {e}");
        }
    }

    pub fn set_section(&mut self, section: SettingsSection) {
        self.section = section;
    }
}

pub fn spawn_event_pump(app: Entity<AppModel>, core_rx: Receiver<CoreEvent>, cx: &mut gpui::App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(33))
                .await;
            let drained: Vec<CoreEvent> = std::iter::from_fn(|| core_rx.try_recv().ok()).collect();
            if drained.is_empty() {
                continue;
            }
            let _ = app.update(cx, |model, cx| {
                for ev in drained {
                    model.apply(ev);
                }
                cx.notify();
            });
        }
    })
    .detach();
}

// ---------------- Pill ----------------

pub struct PillView {
    app: Entity<AppModel>,
}

impl PillView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self { app }
    }
}

impl Render for PillView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let model = self.app.read(cx);
        if matches!(model.phase, Phase::LoadingModel) {
            return skeleton::voice_box_skeleton().into_any_element();
        }

        let (label, dot) = match model.phase {
            Phase::Idle => ("hold fn to dictate", theme::text_tertiary()),
            Phase::Recording => ("listening", hsla(0.0, 0.85, 0.60, 1.0)),
            Phase::Transcribing => ("transcribing…", theme::warn()),
            Phase::NeedsModel => ("model missing — see Settings", theme::text_tertiary()),
            Phase::LoadingModel => unreachable!("handled above"),
        };
        let label: SharedString = label.into();
        let recording = matches!(model.phase, Phase::Recording);
        let max_h = 20.0_f32;
        let bars: Vec<f32> = model.levels.iter().copied().collect();

        h_flex()
            .id("pill")
            .size_full()
            .items_center()
            .justify_center()
            .child(
                h_flex()
                    .gap_3()
                    .px_4()
                    .py_2()
                    .mx_3()
                    .my_3()
                    .w_full()
                    .h(px(52.0))
                    .items_center()
                    .overflow_hidden()
                    .rounded_full()
                    .bg(theme::pill_bg())
                    .border_1()
                    .border_color(theme::pill_border())
                    .text_color(theme::text_primary())
                    .text_sm()
                    .child(div().size(px(8.0)).rounded_full().bg(dot))
                    .child(label)
                    .child(
                        h_flex()
                            .ml_auto()
                            .gap_1()
                            .h(px(max_h))
                            .items_center()
                            .children(bars.into_iter().enumerate().map(|(i, l)| {
                                let amp = (l * 3.5).clamp(0.0, 1.0);
                                let base = if recording { 0.18 } else { 0.10 };
                                let h = (base + amp * (1.0 - base)) * max_h;
                                let hue = 0.0 + (i as f32) * 0.005;
                                div()
                                    .w(px(3.0))
                                    .h(px(h.max(2.0)))
                                    .rounded_sm()
                                    .bg(if recording {
                                        hsla(hue, 0.85, 0.62, 1.0)
                                    } else {
                                        theme::vu_idle()
                                    })
                            })),
                    ),
            )
            .into_any_element()
    }
}

// ---------------- Settings ----------------

pub struct SettingsView {
    app: Entity<AppModel>,
}

impl SettingsView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self { app }
    }
}

struct DetailData {
    section: SettingsSection,
    log: String,
    hotkey: Hotkey,
    trailing_space: bool,
    phase: Phase,
    downloading: bool,
    extracting: bool,
    download_bytes: u64,
    download_total: Option<u64>,
    last_transcript: String,
    history: Vec<RecordingSummary>,
    update_available: Option<UpdateInfo>,
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let data = {
            let model = self.app.read(cx);
            let settings = model.settings.read();
            DetailData {
                section: model.section,
                log: model.log.clone(),
                hotkey: settings.hotkey,
                trailing_space: settings.trailing_space,
                phase: model.phase,
                downloading: model.downloading,
                extracting: model.extracting,
                download_bytes: model.download_bytes,
                download_total: model.download_total,
                last_transcript: model.last_transcript.clone(),
                history: model.history.clone(),
                update_available: model.update_available.clone(),
            }
        };

        let app = self.app.clone();

        h_flex()
            .id("settings-root")
            .size_full()
            .text_color(theme::text_primary())
            .child(render_sidebar(app.clone(), data.section))
            .child(render_detail(app, data))
    }
}

fn render_sidebar(app: Entity<AppModel>, current: SettingsSection) -> AnyElement {
    v_flex()
        .id("settings-sidebar")
        .w(px(196.0))
        .h_full()
        .pt(px(40.0))
        .px_2()
        .gap_1()
        .bg(theme::sidebar_tint())
        .border_r_1()
        .border_color(theme::divider())
        .child(
            div()
                .px_3()
                .py_2()
                .text_sm()
                .text_color(theme::text_secondary())
                .child("yap"),
        )
        .children(SettingsSection::ALL.into_iter().map(|s| {
            let app = app.clone();
            nav_item(s.label(), s == current, move |cx| {
                app.update(cx, |m, cx| {
                    m.set_section(s);
                    cx.notify();
                });
            })
        }))
        .into_any_element()
}

fn render_detail(app: Entity<AppModel>, data: DetailData) -> AnyElement {
    let section = data.section;
    let body: AnyElement = match section {
        SettingsSection::General => {
            render_general(app.clone(), data.hotkey, data.trailing_space)
        }
        SettingsSection::Model => render_model_section(
            app.clone(),
            data.phase,
            data.downloading,
            data.extracting,
            data.download_bytes,
            data.download_total,
            data.last_transcript,
        ),
        SettingsSection::History => render_history_section(data.history),
        SettingsSection::About => render_about_section(data.update_available),
    };

    v_flex()
        .id("settings-detail")
        .flex_1()
        .h_full()
        .bg(theme::detail_tint())
        .child(
            v_flex()
                .id("settings-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .pt(px(28.0))
                .px(px(28.0))
                .pb(px(16.0))
                .gap_5()
                .child(
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_lg()
                                .text_color(theme::text_primary())
                                .child(section.label()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme::text_tertiary())
                                .child(section.subtitle()),
                        ),
                )
                .child(body),
        )
        .child(
            div()
                .px(px(28.0))
                .py(px(10.0))
                .border_t_1()
                .border_color(theme::divider())
                .child(footer_log(&data.log)),
        )
        .into_any_element()
}

fn render_general(app: Entity<AppModel>, selected_hotkey: Hotkey, trailing_space: bool) -> AnyElement {
    let hotkey_row = h_flex().gap_2().children(Hotkey::ALL.map(|hotkey| {
        let app = app.clone();
        let mut btn =
            Button::new(SharedString::from(format!("hotkey-{}", hotkey.label())))
                .label(hotkey.label());
        if hotkey == selected_hotkey {
            btn = btn.primary();
        } else {
            btn = btn.outline();
        }
        btn.on_click(move |_, _, cx| {
            app.update(cx, |m, cx| {
                m.set_hotkey(hotkey);
                cx.notify();
            });
        })
    }));

    let trailing_btn = {
        let app = app.clone();
        let mut btn = Button::new("toggle-trailing")
            .label(if trailing_space { "On" } else { "Off" });
        if trailing_space {
            btn = btn.primary();
        } else {
            btn = btn.outline();
        }
        btn.on_click(move |_, _, cx| {
            app.update(cx, |m, cx| {
                m.set_trailing_space(!trailing_space);
                cx.notify();
            });
        })
    };

    group_card(
        "Input",
        v_flex()
            .gap_4()
            .child(form_row("Push-to-talk", hotkey_row))
            .child(divider_line())
            .child(form_row("Append trailing space", trailing_btn)),
    )
}

fn render_model_section(
    app: Entity<AppModel>,
    phase: Phase,
    downloading: bool,
    extracting: bool,
    download_bytes: u64,
    download_total: Option<u64>,
    last_transcript: String,
) -> AnyElement {
    let busy = downloading || extracting;
    let ready = !busy
        && matches!(
            phase,
            Phase::Idle | Phase::Recording | Phase::Transcribing
        );

    let status: SharedString = if extracting {
        "Extracting…".into()
    } else if downloading {
        match download_total {
            Some(total) if total > 0 => format!(
                "Downloading… {:.1} / {:.1} MB",
                download_bytes as f64 / 1_048_576.0,
                total as f64 / 1_048_576.0,
            )
            .into(),
            _ => format!(
                "Downloading… {:.1} MB",
                download_bytes as f64 / 1_048_576.0
            )
            .into(),
        }
    } else if matches!(phase, Phase::LoadingModel) {
        "Loading Parakeet…".into()
    } else if ready {
        "Installed".into()
    } else {
        "Not installed".into()
    };
    let dot_color: Hsla = if ready {
        theme::good()
    } else if busy {
        theme::warn()
    } else {
        theme::text_tertiary()
    };

    let path: SharedString = parakeet_dir().display().to_string().into();
    let btn_label = if busy {
        "Downloading…"
    } else if ready {
        "Re-download model"
    } else {
        "Download Parakeet (≈ 600 MB)"
    };
    let mut dl_btn = Button::new("dl").label(btn_label).disabled(busy);
    if ready {
        dl_btn = dl_btn.outline();
    } else {
        dl_btn = dl_btn.primary();
    }
    let dl_btn = dl_btn.on_click(move |_, _, cx| {
        app.update(cx, |m, _| m.start_download());
    });

    let last: SharedString = if last_transcript.is_empty() {
        "—".into()
    } else {
        last_transcript.into()
    };

    v_flex()
        .gap_5()
        .child(group_card(
            "Parakeet TDT v2",
            v_flex()
                .gap_3()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().size(px(8.0)).rounded_full().bg(dot_color))
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme::text_primary())
                                .child(status),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme::text_tertiary())
                        .child(path),
                )
                .child(h_flex().mt_2().child(dl_btn)),
        ))
        .child(group_card(
            "Last transcript",
            div()
                .text_sm()
                .text_color(theme::text_secondary())
                .child(last),
        ))
        .into_any_element()
}

fn render_history_section(history: Vec<RecordingSummary>) -> AnyElement {
    let db_path: SharedString = history_path().display().to_string().into();
    let inner: AnyElement = if history.is_empty() {
        v_flex()
            .py_4()
            .child(
                div()
                    .text_sm()
                    .text_color(theme::text_tertiary())
                    .child("No recordings yet."),
            )
            .into_any_element()
    } else {
        v_flex()
            .gap_2()
            .children(history.into_iter().map(|entry| {
                let meta: SharedString = format!(
                    "{} · {:.1}s · {}",
                    relative_time(entry.created_at),
                    entry.duration_ms as f64 / 1_000.0,
                    format_bytes(entry.audio_bytes),
                )
                .into();
                let text: SharedString = if entry.output_text.is_empty() {
                    "No transcript".into()
                } else {
                    preview_text(&entry.output_text).into()
                };

                v_flex()
                    .gap_1()
                    .p_3()
                    .rounded_md()
                    .bg(theme::card_bg())
                    .border_1()
                    .border_color(theme::divider())
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::text_tertiary())
                            .child(meta),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme::text_primary())
                            .child(text),
                    )
            }))
            .into_any_element()
    };

    v_flex()
        .gap_3()
        .child(
            div()
                .text_xs()
                .text_color(theme::text_tertiary())
                .child(db_path),
        )
        .child(inner)
        .into_any_element()
}

fn render_about_section(update: Option<UpdateInfo>) -> AnyElement {
    let version: SharedString = env!("CARGO_PKG_VERSION").into();

    let version_row: AnyElement = match update {
        Some(info) => {
            let label: SharedString = format!("Yap {} available", info.version).into();
            let url = info.url.clone();
            form_row(
                "Version",
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme::text_secondary())
                            .child(version),
                    )
                    .child(
                        div()
                            .id("update-badge")
                            .px_2()
                            .py(px(2.0))
                            .rounded_md()
                            .bg(hsla(11.0 / 360.0, 0.90, 0.55, 0.18))
                            .border_1()
                            .border_color(hsla(11.0 / 360.0, 0.90, 0.55, 0.45))
                            .text_xs()
                            .text_color(hsla(11.0 / 360.0, 0.95, 0.70, 1.0))
                            .hover(|s| s.bg(hsla(11.0 / 360.0, 0.90, 0.55, 0.28)))
                            .on_click(move |_, _, _| updater::open_url(&url))
                            .child(label),
                    ),
            )
        }
        None => form_row(
            "Version",
            div()
                .text_sm()
                .text_color(theme::text_secondary())
                .child(version),
        ),
    };

    group_card(
        "About",
        v_flex()
            .gap_2()
            .child(version_row)
            .child(divider_line())
            .child(form_row(
                "Engine",
                div()
                    .text_sm()
                    .text_color(theme::text_secondary())
                    .child("sherpa-onnx · Parakeet TDT 0.6B v2 (int8)"),
            ))
            .child(divider_line())
            .child(form_row(
                "Made with",
                div()
                    .text_sm()
                    .text_color(theme::text_secondary())
                    .child("Rust · GPUI"),
            )),
    )
}

// ---------------- helpers ----------------

fn nav_item(
    label: &'static str,
    selected: bool,
    on_click: impl Fn(&mut gpui::App) + 'static,
) -> AnyElement {
    let bg = if selected {
        theme::row_active()
    } else {
        hsla(0.0, 0.0, 0.0, 0.0)
    };
    let fg = if selected {
        theme::text_primary()
    } else {
        theme::text_secondary()
    };
    div()
        .id(SharedString::from(format!("nav-{}", label)))
        .px_3()
        .py_2()
        .rounded_md()
        .text_sm()
        .text_color(fg)
        .bg(bg)
        .hover(|s| s.bg(theme::row_hover()))
        .on_click(move |_, _, cx| on_click(cx))
        .child(label)
        .into_any_element()
}

fn group_card(title: &'static str, body: impl IntoElement) -> AnyElement {
    v_flex()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(theme::text_tertiary())
                .child(title),
        )
        .child(
            v_flex()
                .p_4()
                .rounded_lg()
                .bg(theme::card_bg())
                .border_1()
                .border_color(theme::divider())
                .child(body),
        )
        .into_any_element()
}

fn form_row(label: &'static str, control: impl IntoElement) -> AnyElement {
    h_flex()
        .items_center()
        .gap_4()
        .child(
            div()
                .w(px(140.0))
                .text_sm()
                .text_color(theme::text_secondary())
                .child(label),
        )
        .child(h_flex().ml_auto().items_center().child(control))
        .into_any_element()
}

fn divider_line() -> AnyElement {
    div()
        .h(px(1.0))
        .w_full()
        .bg(theme::divider())
        .into_any_element()
}

fn footer_log(log: &str) -> AnyElement {
    if log.is_empty() {
        return div().into_any_element();
    }
    div()
        .text_xs()
        .text_color(theme::text_tertiary())
        .child(SharedString::from(log.to_string()))
        .into_any_element()
}

fn relative_time(created_at: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(created_at);
    let age = now.saturating_sub(created_at);
    if age < 60 {
        "just now".into()
    } else if age < 3_600 {
        format!("{}m ago", age / 60)
    } else if age < 86_400 {
        format!("{}h ago", age / 3_600)
    } else {
        format!("{}d ago", age / 86_400)
    }
}

fn format_bytes(bytes: usize) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{:.1} KB", bytes as f64 / 1_024.0)
    } else {
        format!("{bytes} B")
    }
}

fn preview_text(text: &str) -> String {
    const LIMIT: usize = 140;
    let trimmed = text.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_string();
    }
    let mut preview: String = trimmed.chars().take(LIMIT).collect();
    preview.push('…');
    preview
}
