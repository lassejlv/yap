use gpui::App;
use gpui_component::theme::{Theme, ThemeMode};

pub fn init(cx: &mut App) {
    gpui_component::init(cx);
    gpui_component::theme::init(cx);
    Theme::change(ThemeMode::Dark, None, cx);
}
