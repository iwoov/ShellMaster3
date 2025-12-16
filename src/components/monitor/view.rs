// 系统监控主视图

use gpui::*;

use crate::models::monitor::MonitorState;

use super::detail_dialog::DetailDialogState;
use super::disk_card::render_disk_card;
use super::load_card::render_load_card;
use super::network_card::render_network_card;
use super::system_card::render_system_card;

/// 渲染监控主视图（不包含detail dialog overlay，由上层组件渲染）
pub fn render_monitor_view(
    state: &MonitorState,
    dialog_state: Entity<DetailDialogState>,
    cx: &App,
) -> impl IntoElement {
    let bg_color = crate::theme::sidebar_color(cx);

    div()
        .id("monitor-view-scroll")
        .flex_1()
        .min_h(px(0.))
        .bg(bg_color)
        .overflow_y_scroll()
        .px_1()
        .py_2()
        .flex()
        .flex_col()
        .gap_0()
        // 系统信息卡片
        .child(render_system_card(state, dialog_state.clone(), cx))
        // 系统负载卡片
        .child(render_load_card(state, dialog_state.clone(), cx))
        // 网络状态卡片
        .child(render_network_card(state, dialog_state.clone(), cx))
        // 磁盘状态卡片
        .child(render_disk_card(state, dialog_state, cx))
}
