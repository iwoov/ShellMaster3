// Session 主布局组件 - 使用可拖动分隔条

use gpui::*;
use gpui_component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_component::ActiveTheme;

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
pub fn render_session_layout(
    tab: &SessionTab,
    sidebar_collapsed: bool,
    cx: &App,
) -> impl IntoElement {
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

    // 获取主题颜色
    let border_color = cx.theme().border;
    let sidebar_bg = crate::theme::sidebar_color(cx);
    let muted_fg = cx.theme().muted_foreground;

    // 小侧栏组件 - 始终存在
    let mini_sidebar = div()
        .w(px(20.))
        .flex_shrink_0()
        .border_l_1()
        .border_color(border_color)
        .bg(sidebar_bg)
        .flex()
        .flex_col()
        .items_center()
        .pt_4()
        .child(div().text_xs().text_color(muted_fg).child("命"))
        .child(div().text_xs().text_color(muted_fg).child("令"));

    // 主布局：使用简单的 flex 容器
    if sidebar_collapsed {
        // 折叠时：直接使用 h_resizable 填满左侧 + 小侧栏在右侧
        div()
            .size_full()
            .flex()
            .flex_row()
            .child(h_resizable("session-main-collapsed").child(resizable_panel().child(left_area)))
            .child(mini_sidebar)
    } else {
        // 展开时：h_resizable 包含 left_area + session_sidebar，小侧栏在最右侧
        div()
            .size_full()
            .flex()
            .flex_row()
            .child(
                h_resizable("session-main-expanded")
                    .child(resizable_panel().child(left_area))
                    .child(
                        resizable_panel()
                            .size(px(230.))
                            .child(render_session_sidebar(tab, cx)),
                    ),
            )
            .child(mini_sidebar)
    }
}
