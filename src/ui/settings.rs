use chrono::{DateTime, Local};
use gpui::{
    actions, div, prelude::*, px, AnyView, App, AppContext, Bounds, Context, Entity, IntoElement,
    MouseButton, ParentElement, Point, Render, SharedString, Size, Styled, Window, WindowBounds,
    WindowOptions,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    label::Label,
    switch::Switch,
    ActiveTheme, Icon, IconName, Root, Selectable, Sizable, TitleBar,
};

use crate::app::AppHandle;
use crate::config::OutputMode;
use crate::storage::Recording;

actions!(whispr, [OpenSettings, SaveSettings]);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    General,
    Recordings,
    About,
}

pub fn open(handle: AppHandle, cx: &mut App) {
    let bounds = Bounds {
        origin: Point::new(px(160.0), px(120.0)),
        size: Size {
            width: px(960.0),
            height: px(640.0),
        },
    };
    cx.open_window(
        WindowOptions {
            titlebar: Some(TitleBar::title_bar_options()),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|cx| SettingsView::new(handle.clone(), window, cx));
            cx.new(|cx| Root::new(AnyView::from(view), window, cx))
        },
    )
    .expect("open settings window");
}

pub struct SettingsView {
    handle: AppHandle,
    tab: Tab,
    api_key: Entity<InputState>,
    cleanup_prompt: Entity<InputState>,
    vocabulary: Entity<InputState>,
    cleanup_enabled: bool,
    output_mode: OutputMode,
    recordings: Vec<Recording>,
    save_status: Option<SharedString>,
}

impl SettingsView {
    pub fn new(handle: AppHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let cfg = handle.config();
        let api_key = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("sk-...")
                .masked(true)
                .default_value(cfg.openai_api_key.clone())
        });
        let cleanup_prompt = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .default_value(cfg.cleanup_prompt.clone())
        });
        let vocabulary = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Comma-separated terms to preserve")
                .default_value(cfg.vocabulary.join(", "))
        });

        let view = Self {
            handle,
            tab: Tab::General,
            api_key,
            cleanup_prompt,
            vocabulary,
            cleanup_enabled: cfg.cleanup_enabled,
            output_mode: cfg.output_mode,
            recordings: Vec::new(),
            save_status: None,
        };
        cx.spawn({
            let h = view.handle.clone();
            async move |this, cx| {
                let store = h.store();
                let rt = h.rt();
                let result = rt.spawn(async move { store.recent(100).await }).await;
                if let Ok(Ok(recs)) = result {
                    this.update(cx, |v: &mut SettingsView, cx| {
                        v.recordings = recs;
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
        view
    }

    fn reload_recordings(&self, cx: &mut Context<Self>) {
        let h = self.handle.clone();
        cx.spawn(async move |this, cx| {
            let recs = h
                .rt()
                .spawn(async move { h.store().recent(100).await })
                .await;
            if let Ok(Ok(recs)) = recs {
                this.update(cx, |v: &mut SettingsView, cx| {
                    v.recordings = recs;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let mut cfg = self.handle.config();
        cfg.openai_api_key = self.api_key.read(cx).value().to_string();
        cfg.cleanup_prompt = self.cleanup_prompt.read(cx).value().to_string();
        cfg.cleanup_enabled = self.cleanup_enabled;
        cfg.output_mode = self.output_mode.clone();
        cfg.vocabulary = self
            .vocabulary
            .read(cx)
            .value()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        match self.handle.update_config(cfg) {
            Ok(_) => self.save_status = Some(SharedString::from("Saved")),
            Err(e) => {
                tracing::error!(error = ?e, "save config failed");
                self.save_status = Some(SharedString::from(format!("Error: {e}")));
            }
        }
        cx.notify();
    }

    fn delete_recording(&mut self, id: String, cx: &mut Context<Self>) {
        let h = self.handle.clone();
        cx.spawn(async move |this, cx| {
            let _ = h
                .rt()
                .spawn(async move { h.store().delete(&id).await })
                .await;
            this.update(cx, |v: &mut SettingsView, cx| {
                v.reload_recordings(cx);
            })
            .ok();
        })
        .detach();
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(custom_title_bar(cx))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    .child(self.sidebar(cx))
                    .child(div().flex_1().min_w_0().child(match self.tab {
                        Tab::General => self.general_panel(cx).into_any_element(),
                        Tab::Recordings => self.recordings_panel(cx).into_any_element(),
                        Tab::About => self.about_panel(cx).into_any_element(),
                    })),
            )
    }
}

impl SettingsView {
    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .w(px(220.))
            .h_full()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.sidebar_border)
            .px_3()
            .pt_4()
            .pb_6()
            .flex()
            .flex_col()
            .gap_1()
            .child(brand_row(cx))
            .child(div().h(px(12.)))
            .child(section_label("Navigation", cx))
            .child(self.nav_item(cx, Tab::General, IconName::Settings2, "General"))
            .child(self.nav_item(cx, Tab::Recordings, IconName::Inbox, "Recordings"))
            .child(self.nav_item(cx, Tab::About, IconName::Info, "About"))
    }

    fn nav_item(
        &self,
        cx: &mut Context<Self>,
        tab: Tab,
        icon: IconName,
        label: &str,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let active = self.tab == tab;
        let (bg, fg) = if active {
            (theme.sidebar_accent, theme.sidebar_accent_foreground)
        } else {
            (gpui::transparent_black(), theme.sidebar_foreground)
        };
        div()
            .id(SharedString::from(format!("nav-{label}")))
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(bg)
            .text_color(fg)
            .hover(|s| s.bg(theme.sidebar_accent.opacity(0.5)))
            .child(Icon::new(icon).small())
            .child(Label::new(label.to_string()).text_sm())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _w, cx| {
                    this.tab = tab;
                    cx.notify();
                }),
            )
    }

    fn general_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let paste_selected = matches!(self.output_mode, OutputMode::Paste);
        let type_selected = matches!(self.output_mode, OutputMode::Type);

        div()
            .id("general-scroll")
            .size_full()
            .px_10()
            .py_8()
            .flex()
            .flex_col()
            .gap_6()
            .overflow_y_scroll()
            .child(panel_header(
                "General",
                "Configure dictation, models, and output.",
                cx,
            ))
            .child(card(cx).child(field(
                "OpenAI API Key",
                "Stored locally. Used for transcription and cleanup.",
                Input::new(&self.api_key).into_any_element(),
                cx,
            )))
            .child(card(cx).child(field(
                "Cleanup Prompt",
                "Instruction sent to the cleanup model.",
                Input::new(&self.cleanup_prompt).into_any_element(),
                cx,
            )))
            .child(card(cx).child(field(
                "Custom Vocabulary",
                "Terms preserved verbatim during cleanup.",
                Input::new(&self.vocabulary).into_any_element(),
                cx,
            )))
            .child(
                card(cx).child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(Label::new("LLM Cleanup").text_sm())
                                .child(
                                    Label::new(
                                        "Post-process raw transcript via gpt-4o-mini.",
                                    )
                                    .text_xs()
                                    .text_color(theme.muted_foreground),
                                ),
                        )
                        .child(Switch::new("cleanup-toggle").checked(self.cleanup_enabled).on_click(
                            cx.listener(|this, value: &bool, _w, cx| {
                                this.cleanup_enabled = *value;
                                cx.notify();
                            }),
                        )),
                ),
            )
            .child(
                card(cx).child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(Label::new("Output Mode").text_sm())
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    Button::new("mode-paste")
                                        .label("Paste (Cmd+V)")
                                        .selected(paste_selected)
                                        .on_click(cx.listener(|this, _, _w, cx| {
                                            this.output_mode = OutputMode::Paste;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("mode-type")
                                        .label("Simulated Typing")
                                        .selected(type_selected)
                                        .on_click(cx.listener(|this, _, _w, cx| {
                                            this.output_mode = OutputMode::Type;
                                            cx.notify();
                                        })),
                                ),
                        ),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_3()
                    .child(
                        Label::new(self.save_status.clone().unwrap_or_default())
                            .text_xs()
                            .text_color(theme.muted_foreground),
                    )
                    .child(
                        Button::new("save")
                            .primary()
                            .label("Save Changes")
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }

    fn recordings_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div().flex().flex_col().gap_3();

        if self.recordings.is_empty() {
            list = list.child(
                card(cx).child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .py_8()
                        .gap_2()
                        .child(Label::new("No recordings yet").text_sm())
                        .child(
                            Label::new("Hold Fn anywhere on the system to dictate.")
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        ),
                ),
            );
        } else {
            for rec in &self.recordings {
                list = list.child(recording_card(rec, cx));
            }
        }

        div()
            .id("recordings-scroll")
            .size_full()
            .px_10()
            .py_8()
            .flex()
            .flex_col()
            .gap_6()
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(panel_header(
                        "Recordings",
                        "Replay or copy past dictation.",
                        cx,
                    ))
                    .child(
                        Button::new("refresh")
                            .label("Refresh")
                            .on_click(cx.listener(|this, _, _w, cx| this.reload_recordings(cx))),
                    ),
            )
            .child(list)
    }

    fn about_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .size_full()
            .px_10()
            .py_8()
            .flex()
            .flex_col()
            .gap_6()
            .child(panel_header(
                "About",
                "Whispr — local dictation for macOS.",
                cx,
            ))
            .child(
                card(cx).child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(info_row("Version", env!("CARGO_PKG_VERSION"), theme))
                        .child(info_row("STT Model", "gpt-4o-transcribe", theme))
                        .child(info_row("Cleanup Model", "gpt-4o-mini", theme))
                        .child(info_row("Hotkey", "Hold Fn", theme)),
                ),
            )
    }
}

fn custom_title_bar(cx: &Context<SettingsView>) -> impl IntoElement {
    let theme = cx.theme();
    TitleBar::new()
        .child(
            div()
                .pl(px(82.))
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .size(px(8.))
                        .rounded_full()
                        .bg(gpui::rgb(0xef4444)),
                )
                .child(
                    Label::new("Whispr")
                        .text_sm()
                        .text_color(theme.foreground),
                ),
        )
}

fn brand_row(cx: &Context<SettingsView>) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_2()
        .child(
            div()
                .size(px(10.))
                .rounded_full()
                .bg(gpui::rgb(0xef4444))
                .shadow(vec![gpui::BoxShadow {
                    color: gpui::rgb(0xef4444).into(),
                    offset: Point::new(px(0.), px(0.)),
                    blur_radius: px(8.),
                    spread_radius: px(1.),
                }]),
        )
        .child(
            Label::new("Settings")
                .text_color(theme.sidebar_foreground)
                .text_sm(),
        )
}

fn section_label(name: &str, cx: &Context<SettingsView>) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .px_3()
        .pt_2()
        .pb_1()
        .child(
            Label::new(name.to_uppercase())
                .text_xs()
                .text_color(theme.muted_foreground),
        )
}

fn panel_header(
    title: &str,
    subtitle: &str,
    cx: &Context<SettingsView>,
) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(px(22.))
                .text_color(theme.foreground)
                .child(SharedString::from(title.to_string())),
        )
        .child(
            Label::new(subtitle.to_string())
                .text_sm()
                .text_color(theme.muted_foreground),
        )
}

fn card(cx: &Context<SettingsView>) -> gpui::Div {
    let theme = cx.theme();
    div()
        .p_5()
        .rounded_lg()
        .bg(theme.group_box)
        .border_1()
        .border_color(theme.border)
}

fn field(
    name: &str,
    hint: &str,
    child: gpui::AnyElement,
    cx: &Context<SettingsView>,
) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(Label::new(name.to_string()).text_sm())
                .child(
                    Label::new(hint.to_string())
                        .text_xs()
                        .text_color(theme.muted_foreground),
                ),
        )
        .child(child)
}

fn recording_card(rec: &Recording, cx: &mut Context<SettingsView>) -> impl IntoElement {
    let theme = cx.theme();
    let ts_local: DateTime<Local> = rec.created_at.with_timezone(&Local);
    let ts_label = ts_local.format("%b %d · %H:%M:%S").to_string();
    let duration = format!("{:.1}s", rec.duration_ms as f32 / 1000.0);
    let text = rec.display_text().to_string();
    let snippet = truncate(&text, 240);

    let wav_path = rec.wav_path.clone();
    let text_for_copy = text.clone();
    let id_for_delete = rec.id.clone();

    card(cx).child(
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(badge_neutral(&ts_label, theme))
                            .child(badge_accent(&duration, theme)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new(SharedString::from(format!("play-{}", rec.id)))
                                    .label("Play")
                                    .on_click(move |_, _w, _cx| {
                                        play_wav(&wav_path);
                                    }),
                            )
                            .child(
                                Button::new(SharedString::from(format!("copy-{}", rec.id)))
                                    .label("Copy")
                                    .on_click(move |_, _w, _cx| {
                                        copy_text(&text_for_copy);
                                    }),
                            )
                            .child(
                                Button::new(SharedString::from(format!("del-{}", rec.id)))
                                    .label("Delete")
                                    .danger()
                                    .on_click(cx.listener(move |this, _, _w, cx| {
                                        this.delete_recording(id_for_delete.clone(), cx);
                                    })),
                            ),
                    ),
            )
            .child(Label::new(snippet).text_sm()),
    )
}

fn badge_neutral(text: &str, theme: &gpui_component::theme::Theme) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(theme.muted)
        .child(
            Label::new(text.to_string())
                .text_xs()
                .text_color(theme.muted_foreground),
        )
}

fn badge_accent(text: &str, theme: &gpui_component::theme::Theme) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(theme.accent)
        .child(
            Label::new(text.to_string())
                .text_xs()
                .text_color(theme.accent_foreground),
        )
}

fn info_row(label: &str, value: &str, theme: &gpui_component::theme::Theme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py_1()
        .child(
            Label::new(label.to_string())
                .text_sm()
                .text_color(theme.muted_foreground),
        )
        .child(Label::new(value.to_string()).text_sm())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

fn play_wav(path: &str) {
    let path = path.to_string();
    std::thread::spawn(move || {
        let _ = std::process::Command::new("afplay").arg(&path).status();
    });
}

fn copy_text(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text.to_string());
    }
}
