use crossbeam_channel::Receiver;
use gpui::{
    div, prelude::*, px, rgb, rgba, Context, Hsla, IntoElement, MouseButton, ParentElement, Render,
    SharedString, Styled, Window,
};
use std::sync::Arc;
use std::time::Duration;

use crate::app::{AppHandle, AppState, Phase};
use crate::audio::{AudioLevels, BARS};

pub struct Pill {
    rx: Receiver<AppState>,
    levels: Arc<AudioLevels>,
    state: AppState,
    bars: Vec<f32>,
}

impl Pill {
    pub fn new(handle: AppHandle, rx: Receiver<AppState>, cx: &mut Context<Self>) -> Self {
        let state = handle.snapshot();
        let levels = handle.levels();
        let bars = levels.snapshot();

        let bg = cx.background_executor().clone();
        cx.spawn(async move |view, cx| {
            loop {
                bg.timer(Duration::from_millis(33)).await;
                let result = view.update(cx, |v: &mut Pill, cx| {
                    while let Ok(s) = v.rx.try_recv() {
                        v.state = s;
                    }
                    if matches!(v.state.phase, Phase::Recording) {
                        v.bars = v.levels.snapshot();
                    } else if !v.bars.iter().all(|b| *b == 0.0) {
                        for b in v.bars.iter_mut() {
                            *b *= 0.7;
                            if *b < 0.01 {
                                *b = 0.0;
                            }
                        }
                    }
                    cx.notify();
                });
                if result.is_err() {
                    break;
                }
            }
        })
        .detach();

        let _ = handle;
        Self {
            rx,
            levels,
            state,
            bars,
        }
    }
}

impl Render for Pill {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (label, accent) = phase_style(self.state.phase);

        let recording = matches!(self.state.phase, Phase::Recording);
        let working = matches!(
            self.state.phase,
            Phase::Transcribing | Phase::Cleaning | Phase::Outputting
        );

        div()
            .size_full()
            .flex()
            .items_center()
            .gap_3()
            .px_5()
            .rounded_full()
            .bg(rgb(0x0b0f14))
            .border_1()
            .border_color(rgba(0xffffff14))
            .on_mouse_down(MouseButton::Left, |_event, window, _cx| {
                window.start_window_move();
            })
            .child(status_dot(accent, recording || working))
            .child(if recording {
                waveform(&self.bars, accent).into_any_element()
            } else {
                div()
                    .text_color(rgb(0xe5e7eb))
                    .text_size(px(13.))
                    .child(SharedString::from(label))
                    .into_any_element()
            })
    }
}

fn phase_style(phase: Phase) -> (&'static str, Hsla) {
    match phase {
        Phase::Idle => ("Ready", rgb(0x4b5563).into()),
        Phase::Recording => ("Listening", rgb(0xef4444).into()),
        Phase::Transcribing => ("Transcribing", rgb(0xf59e0b).into()),
        Phase::Cleaning => ("Cleaning", rgb(0x6366f1).into()),
        Phase::Outputting => ("Pasting", rgb(0x10b981).into()),
    }
}

fn status_dot(color: Hsla, glow: bool) -> impl IntoElement {
    let dot = div().size(px(10.)).rounded_full().bg(color);
    if glow {
        dot.shadow(vec![gpui::BoxShadow {
            color,
            offset: gpui::Point::new(px(0.), px(0.)),
            blur_radius: px(8.),
            spread_radius: px(1.),
        }])
        .into_any_element()
    } else {
        dot.into_any_element()
    }
}

fn waveform(bars: &[f32], color: Hsla) -> impl IntoElement {
    let row = div()
        .flex()
        .items_end()
        .gap(px(2.))
        .h(px(28.));

    let n = bars.len().max(BARS);
    let mut populated = row;
    for i in 0..n {
        let v = bars.get(i).copied().unwrap_or(0.0);
        let h = (v * 26.0 + 2.0).clamp(2.0, 28.0);
        populated = populated.child(
            div()
                .w(px(3.))
                .h(px(h))
                .rounded(px(2.))
                .bg(color),
        );
    }
    populated
}
