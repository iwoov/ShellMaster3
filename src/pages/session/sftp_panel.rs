// SFTP 面板组件（简化版）

use gpui::*;
use gpui_component::ActiveTheme;

/// 渲染 SFTP 面板（占位）
pub fn render_sftp_panel(cx: &App) -> impl IntoElement {
    let muted_foreground = cx.theme().muted_foreground;
    let bg_color = crate::theme::sidebar_color(cx);

    div()
        .size_full()
        .bg(bg_color)
        .flex()
        .items_center()
        .justify_center()
        .child(div().text_sm().text_color(muted_foreground).child("SFTP"))
}
