// Session 右侧边栏组件

use gpui::*;
use gpui_component::{ActiveTheme, StyledExt};

use crate::state::{SessionTab, SidebarPanel};

/// 渲染会话右侧边栏
pub fn render_session_sidebar(
    _tab: &SessionTab,
    active_panel: SidebarPanel,
    cx: &App,
) -> impl IntoElement {
    let muted_foreground = cx.theme().muted_foreground;
    let bg_color = crate::theme::sidebar_color(cx);
    let foreground = cx.theme().foreground;

    // 获取语言设置
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    // 根据不同面板显示不同内容
    let (title, content) = match active_panel {
        SidebarPanel::Default => ("Sidebar", "默认面板"),
        SidebarPanel::Snippets => (
            crate::i18n::t(&lang, "mini_sidebar.snippets"),
            "快捷命令功能待完善",
        ),
        SidebarPanel::Transfer => (
            crate::i18n::t(&lang, "mini_sidebar.transfer"),
            "传输管理功能待完善",
        ),
    };

    div()
        .size_full()
        .bg(bg_color)
        .flex()
        .flex_col()
        .child(
            // 标题栏
            div()
                .w_full()
                .h(px(44.))
                .flex()
                .items_center()
                .px_4()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(foreground)
                        .child(title),
                ),
        )
        .child(
            // 内容区域
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(div().text_sm().text_color(muted_foreground).child(content)),
        )
}
