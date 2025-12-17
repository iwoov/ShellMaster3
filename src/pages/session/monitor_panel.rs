// Monitor 面板组件

use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::monitor::{render_monitor_view, DetailDialogState};
use crate::models::monitor::MonitorState;
use crate::state::SessionState;

/// 渲染 Monitor 面板
pub fn render_monitor_panel(
    state: &MonitorState,
    detail_dialog_state: Option<Entity<DetailDialogState>>,
    session_state: Entity<SessionState>,
    tab_id: String,
    cx: &App,
) -> impl IntoElement {
    let bg_color = crate::theme::sidebar_color(cx);
    let muted_color = cx.theme().muted_foreground;

    // 如果没有 detail_dialog_state，只渲染基本视图
    if let Some(dialog_state) = detail_dialog_state {
        div()
            .size_full()
            .min_h(px(0.))
            .bg(bg_color)
            .flex()
            .flex_col()
            .child(render_monitor_view(
                state,
                dialog_state,
                session_state,
                tab_id,
                cx,
            ))
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
