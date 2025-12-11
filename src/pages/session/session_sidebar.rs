// Session 右侧边栏组件（简化版）

use gpui::*;
use gpui_component::ActiveTheme;

use crate::state::SessionTab;

/// 渲染会话右侧边栏（占位）
pub fn render_session_sidebar(_tab: &SessionTab, cx: &App) -> impl IntoElement {
    let muted_foreground = cx.theme().muted_foreground;
    let bg_color = crate::theme::sidebar_color(cx);

    div()
        .size_full()
        .bg(bg_color)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_sm()
                .text_color(muted_foreground)
                .child("Sidebar"),
        )
}
