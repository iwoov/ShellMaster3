// Terminal 会话页面组件（占位）

use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::services::storage;
use crate::state::SessionTab;

/// 渲染终端页面（占位）
pub fn render_terminal_page(tab: &SessionTab, cx: &App) -> impl IntoElement {
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let server_label = tab.server_label.clone();
    let bg_color = crate::theme::background_color(cx);
    let primary = cx.theme().primary;
    let foreground = cx.theme().foreground;
    let muted_foreground = cx.theme().muted_foreground;

    div()
        .flex_1()
        .h_full()
        .bg(bg_color)
        .flex()
        .flex_col()
        .justify_center()
        .items_center()
        .gap_6()
        // Terminal 图标
        .child(
            div()
                .w_20()
                .h_20()
                .rounded_2xl()
                .bg(primary.opacity(0.1))
                .flex()
                .items_center()
                .justify_center()
                .child(render_icon(icons::TERMINAL, primary.into())),
        )
        // 状态信息
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(foreground)
                        .child(format!(
                            "{} \"{}\"",
                            i18n::t(&lang, "session.connected"),
                            server_label
                        )),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted_foreground)
                        .child(i18n::t(&lang, "session.terminal_placeholder")),
                ),
        )
}
