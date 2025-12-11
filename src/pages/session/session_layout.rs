// Session 主布局组件 - 使用可拖动分隔条

use gpui::*;
use gpui_component::resizable::{h_resizable, resizable_panel, v_resizable};

use super::monitor_panel::render_monitor_panel;
use super::session_sidebar::render_session_sidebar;
use super::sftp_panel::render_sftp_panel;
use super::terminal_page::render_terminal_panel;
use crate::state::SessionTab;

/// 渲染 Session 主布局
/// 布局结构:
/// ┌─────────────────────────────────────┬─────────────────┐
/// │           左侧区域                   │    右侧边栏     │
/// │  ┌────────────────┬────────────────┐│                 │
/// │  │    Monitor     │   Terminal     ││   (Session     │
/// │  │    面板        │   终端          ││    Sidebar)    │
/// │  ├────────────────┴────────────────┤│                 │
/// │  │           SFTP 面板              ││                 │
/// │  └──────────────────────────────────┘│                 │
/// └─────────────────────────────────────┴─────────────────┘
pub fn render_session_layout(tab: &SessionTab, cx: &App) -> impl IntoElement {
    // 上方区域：Monitor | Terminal （水平分隔）
    let top_area = h_resizable("session-top-h")
        .child(
            resizable_panel()
                .size(px(220.)) // 与 home 按钮区域宽度一致
                .child(render_monitor_panel(cx)),
        )
        .child(resizable_panel().child(render_terminal_panel(tab, cx)));

    // 左侧区域：上方区域 | SFTP （垂直分隔，约 60% : 40%）
    let left_area = v_resizable("session-left-v")
        .child(resizable_panel().child(top_area)) // 上方 ~60%
        .child(
            resizable_panel()
                .size(px(300.))
                .child(render_sftp_panel(cx)),
        ); // SFTP ~40%

    // 主布局：左侧区域 | 右侧边栏 （水平分隔）
    // 使用 div 包装确保填满全高
    div().size_full().flex().child(
        h_resizable("session-main-h")
            .child(resizable_panel().child(left_area))
            .child(
                resizable_panel()
                    .size(px(200.))
                    .child(render_session_sidebar(tab, cx)),
            ),
    )
}
