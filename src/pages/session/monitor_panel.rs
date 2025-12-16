// Monitor 面板组件

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme, StyledExt};

use crate::components::monitor::{render_monitor_view, DetailDialogState};
use crate::models::monitor::MonitorState;

/// 渲染 Monitor 面板
pub fn render_monitor_panel(
    detail_dialog_state: Option<Entity<DetailDialogState>>,
    cx: &App,
) -> impl IntoElement {
    let bg_color = crate::theme::sidebar_color(cx);
    let muted_color = cx.theme().muted_foreground;

    // 使用 mock 数据进行 UI 开发
    let state = MonitorState::with_mock_data();

    // 如果没有 detail_dialog_state，只渲染基本视图
    if let Some(dialog_state) = detail_dialog_state {
        div()
            .size_full()
            .bg(bg_color)
            .child(render_monitor_view(&state, dialog_state, cx))
    } else {
        // 没有 dialog state 时的降级渲染（不应该发生）
        div()
            .size_full()
            .bg(bg_color)
            .flex()
            .items_center()
            .justify_center()
            .child(div().text_sm().text_color(muted_color).child("Loading..."))
    }
}
