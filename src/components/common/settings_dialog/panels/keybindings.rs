// 按键绑定面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::i18n;

use super::super::SettingsDialogState;

/// 渲染按键绑定面板
pub fn render_keybindings_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let lang = &state.read(cx).settings.theme.language;
    div().flex().flex_col().gap_6().child(
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .h(px(200.))
            .child(
                div()
                    .text_base()
                    .text_color(cx.theme().muted_foreground)
                    .child(i18n::t(lang, "settings.keybindings.coming_soon")),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .mt_2()
                    .child(i18n::t(lang, "settings.keybindings.description")),
            ),
    )
}
