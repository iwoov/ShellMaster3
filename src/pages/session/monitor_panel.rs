// Monitor 面板组件

use gpui::*;

use crate::components::monitor::render_monitor_view;
use crate::models::monitor::MonitorState;

/// 渲染 Monitor 面板
pub fn render_monitor_panel(cx: &App) -> impl IntoElement {
    let bg_color = crate::theme::sidebar_color(cx);

    // 使用 mock 数据进行 UI 开发
    let state = MonitorState::with_mock_data();

    div()
        .size_full()
        .bg(bg_color)
        .child(render_monitor_view(&state, cx))
}
