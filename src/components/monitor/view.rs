// 系统监控主视图

use gpui::*;

use crate::models::monitor::MonitorState;

use super::disk_card::render_disk_card;
use super::load_card::render_load_card;
use super::network_card::render_network_card;
use super::system_card::render_system_card;

/// 渲染监控主视图
pub fn render_monitor_view(state: &MonitorState, cx: &App) -> impl IntoElement {
    let bg_color = crate::theme::sidebar_color(cx);

    div()
        .id("monitor-view-scroll")
        .size_full()
        .bg(bg_color)
        .overflow_y_scroll()
        .p_3()
        .flex()
        .flex_col()
        .gap_3()
        // 系统信息卡片
        .child(render_system_card(state, cx))
        // 系统负载卡片
        .child(render_load_card(state, cx))
        // 网络状态卡片
        .child(render_network_card(state, cx))
        // 磁盘状态卡片
        .child(render_disk_card(state, cx))
}
