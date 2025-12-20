// Session 主布局组件 - 使用可拖动分隔条

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_component::ActiveTheme;

use super::monitor_panel::render_monitor_panel;
use super::session_sidebar::render_session_sidebar;
use super::sftp_panel::render_sftp_panel;
use super::terminal_page::render_terminal_panel;
use crate::components::monitor::render_detail_dialog;
use crate::state::{SessionState, SessionTab, SidebarPanel};

/// 渲染 Session 主布局
pub fn render_session_layout(
    tab: &SessionTab,
    sidebar_collapsed: bool,
    session_state: Entity<SessionState>,
    cx: &App,
) -> impl IntoElement {
    // 获取命令输入状态和终端焦点句柄
    let command_input = session_state.read(cx).command_input.clone();
    let terminal_focus_handle = session_state.read(cx).get_terminal_focus_handle();

    // 获取 Monitor 详情弹窗状态
    let monitor_detail_dialog = session_state.read(cx).monitor_detail_dialog.clone();
    let monitor_detail_dialog_for_panel = monitor_detail_dialog.clone();

    // 获取 tab_id 用于网络接口选择
    let tab_id = tab.id.clone();

    // 上方区域：Monitor | Terminal （水平分隔）
    let top_area = h_resizable("session-top-h")
        .child(
            resizable_panel()
                .size(px(229.)) // Monitor 面板初始宽度 230px
                .child(render_monitor_panel(
                    &tab.monitor_state,
                    monitor_detail_dialog_for_panel,
                    session_state.clone(),
                    tab_id,
                    cx,
                )),
        )
        .child(resizable_panel().child(render_terminal_panel(
            tab,
            command_input,
            session_state.clone(),
            terminal_focus_handle,
            cx,
        )));

    // 左侧区域：上方区域 | SFTP （垂直分隔）
    let session_state_for_sftp = session_state.clone();
    let tab_id_for_sftp = tab.id.clone();
    // 获取 SFTP 文件列表视图（如果存在）
    let sftp_file_list_view = session_state.read(cx).get_sftp_file_list_view(&tab.id);
    let left_area = v_resizable("session-left-v")
        .child(resizable_panel().child(top_area))
        .child(resizable_panel().size(px(300.)).child(render_sftp_panel(
            tab.sftp_state.as_ref(),
            sftp_file_list_view,
            session_state_for_sftp,
            tab_id_for_sftp,
            cx,
        ))); // SFTP ~40%

    // 获取主题颜色
    let border_color = cx.theme().border;
    let sidebar_bg = crate::theme::sidebar_color(cx);
    let hover_bg = cx.theme().list_active; // 使用更明显的悬停颜色
    let icon_color = cx.theme().muted_foreground;
    let active_icon_color = cx.theme().foreground;

    // 获取当前激活的面板
    let active_panel = session_state.read(cx).active_sidebar_panel;

    // 图标路径
    use crate::constants::icons;

    // 小侧栏宽度固定 28px
    let sidebar_width = 28.;

    // 创建命令行图标按钮
    let is_snippets_active = active_panel == SidebarPanel::Snippets;
    let snippets_session_state = session_state.clone();
    let snippets_button = div()
        .id("mini-sidebar-snippets")
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .rounded(px(4.))
        .when(is_snippets_active, |s| s.bg(hover_bg))
        .hover(|s| s.bg(hover_bg))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            snippets_session_state.update(cx, |state, _| {
                state.load_snippets_config();
                state.set_sidebar_panel(SidebarPanel::Snippets);
            });
        })
        .child(
            svg()
                .path(icons::COMMAND)
                .size(px(16.))
                .text_color(if is_snippets_active {
                    active_icon_color
                } else {
                    icon_color
                }),
        );

    // 创建传输管理图标按钮
    let is_transfer_active = active_panel == SidebarPanel::Transfer;
    let transfer_session_state = session_state.clone();
    let transfer_button = div()
        .id("mini-sidebar-transfer")
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .rounded(px(4.))
        .when(is_transfer_active, |s| s.bg(hover_bg))
        .hover(|s| s.bg(hover_bg))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            transfer_session_state.update(cx, |state, _| {
                state.set_sidebar_panel(SidebarPanel::Transfer);
            });
        })
        .child(
            svg()
                .path(icons::TRANSFER)
                .size(px(16.))
                .text_color(if is_transfer_active {
                    active_icon_color
                } else {
                    icon_color
                }),
        );

    // 小侧栏组件 - 始终存在，包含两个图标按钮
    let mini_sidebar = div()
        .w(px(sidebar_width))
        .flex_shrink_0()
        .border_l_1()
        .border_color(border_color)
        .bg(sidebar_bg)
        .flex()
        .flex_col()
        .items_center()
        .pt_3()
        .gap_2()
        .child(snippets_button)
        .child(transfer_button);

    // 主布局：使用简单的 flex 容器
    // 包装在 relative 容器中以支持 dialog overlay
    let main_content = if sidebar_collapsed {
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
                            .child(render_session_sidebar(
                                tab,
                                active_panel,
                                session_state.clone(),
                                cx,
                            )),
                    ),
            )
            .child(mini_sidebar)
    };

    // 返回带有 dialog overlay 的容器
    if let Some(dialog_state) = monitor_detail_dialog {
        div()
            .relative()
            .size_full()
            .child(main_content)
            .child(render_detail_dialog(dialog_state, &tab.monitor_state, cx))
    } else {
        div().size_full().child(main_content)
    }
}
