// 会话主布局
//
// ┌──────────┬──────────────────────────────┬───────────┬────┐
// │ 监控      │ 终端 / 内置文件编辑器          │ 右侧侧栏   │ 图 │
// │ (左列)    │                              │           │ 标 │
// ├──────────┼──────────────────────────────┤           │ 栏 │
// │ 目录树    │ 文件列表                      │           │    │
// └──────────┴──────────────────────────────┴───────────┴────┘
//
// 左列（监控、目录树）和右侧侧栏是固定宽度，由自己的分隔条拖动；标题栏品牌区与左列同宽。
// 上下方向（终端/编辑器 / SFTP 的高度）用组件库的 `v_resizable`。

use gpui_kit::component::resizable::{resizable_panel, v_resizable};
use gpui_kit::*;

use super::activity_rail::render_activity_rail;
use super::columns::{
    left_width_from_drag, sidebar_width_from_drag, SessionColumns, CENTER_MIN, RAIL_WIDTH,
};
use super::monitor_panel::render_monitor_panel;
use super::session_sidebar::render_session_sidebar;
use super::sftp_panel::render_sftp_panel;
use super::terminal_page::render_terminal_panel;
use crate::components::common::dialog::{render_confirm_dialog, ConfirmDialogText};
use crate::components::common::panel::splitter;
use crate::components::common::text::TextFitter;
use crate::components::monitor::render_detail_dialog;
use crate::components::sftp::{
    render_editor_confirm_dialogs, render_new_file_dialog_overlay,
    render_new_folder_dialog_overlay, render_properties_dialog_overlay, render_sftp_editor_panel,
};
use crate::i18n::t;
use crate::models::settings::Language;
use crate::state::{CenterViewMode, SessionState, SessionTab};

/// SFTP 面板的初始高度
const SFTP_INITIAL_HEIGHT: Pixels = px(300.);

/// 渲染 Session 主布局
pub fn render_session_layout(
    tab: &SessionTab,
    columns: SessionColumns,
    session_state: Entity<SessionState>,
    lang: &Language,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);

    // 获取命令输入状态和终端焦点句柄
    let command_input = session_state.read(cx).command_input.clone();
    let terminal_focus_handle = session_state.read(cx).get_terminal_focus_handle();

    // 弹窗状态
    let monitor_detail_dialog = session_state.read(cx).monitor_detail_dialog.clone();
    let new_folder_dialog = session_state.read(cx).get_sftp_new_folder_dialog();
    let new_file_dialog = session_state.read(cx).get_sftp_new_file_dialog();
    let properties_dialog = session_state.read(cx).get_sftp_properties_dialog();
    let sftp_delete_confirm = session_state.read(cx).sftp_delete_confirm.clone();
    let pending_terminal_paste = session_state.read(cx).pending_terminal_paste.clone();
    let sidebar_collapsed = session_state.read(cx).sidebar_collapsed;
    let active_panel = session_state.read(cx).active_sidebar_panel;
    let fit = TextFitter::new(window, cx);

    // 中间主区上半部分：在终端与 SFTP 内置代码编辑器之间切换
    let center_top_panel: AnyElement =
        if tab.center_view == CenterViewMode::Editor && !tab.editor_tabs.is_empty() {
            render_sftp_editor_panel(tab, session_state.clone(), lang, cx).into_any_element()
        } else {
            render_terminal_panel(
                tab,
                command_input,
                session_state.clone(),
                terminal_focus_handle,
                cx,
            )
            .into_any_element()
        };

    // 上方一行：监控 | 终端(或内置文件编辑器)
    let top_row = div()
        .size_full()
        .flex()
        .flex_row()
        .child(
            div()
                .flex_none()
                .w(columns.left)
                .h_full()
                .border_r_1()
                .border_color(p.border)
                .child(render_monitor_panel(
                    tab,
                    columns.left,
                    monitor_detail_dialog.clone(),
                    session_state.clone(),
                    lang,
                    &fit,
                    cx,
                )),
        )
        .child(div().flex_1().min_w_0().h_full().child(center_top_panel));

    // 下方一行：SFTP（目录树与左列同宽 | 文件列表）
    let sftp_file_list_view = session_state.read(cx).get_sftp_file_list_view(&tab.id);
    let sftp_path_bar_state = session_state.read(cx).get_sftp_path_bar_state(&tab.id);
    let sftp_list_width =
        (columns.window - columns.left - columns.sidebar - RAIL_WIDTH).max(CENTER_MIN);
    let sftp_row = render_sftp_panel(
        tab.sftp_state.as_ref(),
        sftp_file_list_view,
        sftp_path_bar_state,
        session_state.clone(),
        tab.id.clone(),
        columns.left,
        sftp_list_width,
        lang,
        &fit,
        window,
        cx,
    );

    // 左边的主区域：上下两行可拖动高度；左列的分隔条盖在两行上面（左列右边线位于 left-1..left）
    let main_area = div()
        .flex_1()
        .min_w_0()
        .h_full()
        .relative()
        .child(
            v_resizable("session-left-v")
                .child(resizable_panel().child(top_row))
                .child(resizable_panel().size(SFTP_INITIAL_HEIGHT).child(sftp_row)),
        )
        .child(splitter(
            "session-left-column",
            columns.left - px(0.5),
            &p,
            {
                let session_state = session_state.clone();
                move |x, _, cx| {
                    session_state.update(cx, |state, cx| {
                        state.left_column_width = left_width_from_drag(x);
                        cx.notify();
                    });
                }
            },
        ));

    let transfers_running = tab
        .active_transfers
        .iter()
        .any(|transfer| transfer.status.is_active());
    let forwards_running = !tab.port_forwards.is_empty();

    let mut main_content = div()
        .size_full()
        .flex()
        .flex_row()
        .relative()
        .child(main_area);
    if !sidebar_collapsed {
        main_content = main_content.child(
            div()
                .flex_none()
                .w(columns.sidebar)
                .h_full()
                .border_l_1()
                .border_color(p.border)
                .child(render_session_sidebar(
                    tab,
                    active_panel,
                    columns.sidebar,
                    session_state.clone(),
                    lang,
                    &fit,
                    window,
                    cx,
                )),
        );
    }
    main_content = main_content.child(render_activity_rail(
        &session_state,
        transfers_running,
        forwards_running,
        lang,
        &p,
        cx,
    ));
    if !sidebar_collapsed {
        // 侧栏左边线位于 sidebar_left..sidebar_left+1
        let window_width = columns.window;
        let session_state = session_state.clone();
        main_content = main_content.child(splitter(
            "session-sidebar",
            columns.sidebar_left() + px(0.5),
            &p,
            move |x, _, cx| {
                session_state.update(cx, |state, cx| {
                    state.sidebar_width = sidebar_width_from_drag(window_width, x);
                    cx.notify();
                });
            },
        ));
    }

    // 返回带有 dialog overlay 的容器
    let mut result = div().relative().size_full().child(main_content);

    let viewport = window.viewport_size();

    // 添加 Monitor 详情弹窗
    if let Some(dialog_state) = monitor_detail_dialog {
        if dialog_state.read(cx).is_open() {
            dialog_state.update(cx, |state, cx| state.prepare(lang, window, cx));
        }
        result = result.child(render_detail_dialog(
            dialog_state,
            &tab.monitor_state,
            viewport,
            &fit,
            cx,
        ));
    }

    // 添加 SFTP 新建文件夹弹窗
    if let Some(dialog_state) = new_folder_dialog {
        let is_open = dialog_state.read(cx).is_open;
        if is_open {
            let session_state_for_create = session_state.clone();
            result = result.child(render_new_folder_dialog_overlay(
                dialog_state,
                move |path, tab_id, cx| {
                    session_state_for_create.update(cx, |state, cx| {
                        state.sftp_create_folder(path, tab_id, cx);
                    });
                },
                cx,
            ));
        }
    }

    // 添加 SFTP 新建文件弹窗
    if let Some(dialog_state) = new_file_dialog {
        let is_open = dialog_state.read(cx).is_open;
        if is_open {
            let session_state_for_create = session_state.clone();
            result = result.child(render_new_file_dialog_overlay(
                dialog_state,
                move |path, tab_id, cx| {
                    session_state_for_create.update(cx, |state, cx| {
                        state.sftp_create_file(path, tab_id, cx);
                    });
                },
                cx,
            ));
        }
    }

    // 添加 SFTP 属性弹窗
    if let Some(dialog_state) = properties_dialog {
        let is_open = dialog_state.read(cx).is_open;
        if is_open {
            dialog_state.update(cx, |state, cx| state.prepare(lang, window, cx));
            let session_for_chmod = session_state.clone();
            result = result.child(render_properties_dialog_overlay(
                dialog_state,
                &fit,
                move |tab_id, path, mode, cx| {
                    session_for_chmod.update(cx, |state, cx| {
                        state.sftp_chmod(&tab_id, path, mode, cx);
                    });
                },
                cx,
            ));
        }
    }

    // 添加 SFTP 删除二次确认弹窗
    if let Some(confirm) = sftp_delete_confirm {
        let msg_key = if confirm.is_dir {
            "sftp.delete.confirm_folder"
        } else {
            "sftp.delete.confirm_file"
        };
        let message = t(lang, msg_key).replace("{name}", &confirm.name);
        let session_confirm = session_state.clone();
        let session_cancel = session_state.clone();
        result = result.child(render_confirm_dialog(
            ConfirmDialogText {
                title: t(lang, "sftp.delete.confirm_title").into(),
                message: message.into(),
                confirm: t(lang, "sftp.delete.confirm_btn").into(),
                cancel: t(lang, "sftp.delete.cancel_btn").into(),
            },
            &confirm.focus_handle,
            move |_, cx| {
                session_confirm.update(cx, |state, cx| {
                    state.sftp_confirm_delete(cx);
                });
            },
            move |_, cx| {
                session_cancel.update(cx, |state, cx| {
                    state.sftp_cancel_delete(cx);
                });
            },
            cx,
        ));
    }

    // 添加终端多行/大段粘贴二次确认弹窗
    if let Some(paste) = pending_terminal_paste {
        let preview = paste
            .text
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect::<String>();
        let message = t(lang, "terminal.paste_confirm.message")
            .replace("{lines}", &paste.line_count.to_string())
            .replace("{preview}", &preview);
        let session_confirm = session_state.clone();
        let session_cancel = session_state.clone();
        result = result.child(render_confirm_dialog(
            ConfirmDialogText {
                title: t(lang, "terminal.paste_confirm.title").into(),
                message: message.into(),
                confirm: t(lang, "terminal.paste_confirm.confirm_btn").into(),
                cancel: t(lang, "sftp.delete.cancel_btn").into(),
            },
            &paste.focus_handle,
            move |_, cx| {
                session_confirm.update(cx, |state, cx| {
                    state.confirm_terminal_paste(cx);
                });
            },
            move |_, cx| {
                session_cancel.update(cx, |state, cx| {
                    state.cancel_terminal_paste(cx);
                });
            },
            cx,
        ));
    }

    // 添加内置编辑器未保存确认弹窗
    if let Some(dialog) = render_editor_confirm_dialogs(session_state, lang, cx) {
        result = result.child(dialog);
    }

    result
}
