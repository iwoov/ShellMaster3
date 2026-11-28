// Monitor 面板组件（会话视图左上，与左列同宽）

use gpui_kit::*;

use crate::components::common::text::TextFitter;
use crate::components::monitor::{render_monitor_view, DetailDialogState};
use crate::models::settings::Language;
use crate::state::{SessionState, SessionTab};

/// 渲染 Monitor 面板。`width` 是左列宽度（含右边线）
pub fn render_monitor_panel(
    tab: &SessionTab,
    width: Pixels,
    detail_dialog_state: Option<Entity<DetailDialogState>>,
    session_state: Entity<SessionState>,
    lang: &Language,
    fit: &TextFitter,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    match detail_dialog_state {
        Some(dialog_state) => {
            render_monitor_view(tab, width, dialog_state, session_state, lang, fit, cx)
                .into_any_element()
        }
        // 详情弹窗状态在会话创建时就有，这里只是兜底
        None => div().size_full().bg(p.sidebar).into_any_element(),
    }
}
