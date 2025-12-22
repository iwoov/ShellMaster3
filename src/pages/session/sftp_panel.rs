// SFTP 面板组件 - 包含导航工具栏、文件夹树和文件列表

use gpui::*;
use gpui_component::resizable::{h_resizable, resizable_panel};
use gpui_component::ActiveTheme;

use crate::models::sftp::FileType;

use crate::components::sftp::{
    render_folder_tree, render_sftp_toolbar, FileListView, FolderTreeEvent, PathBarState,
    SftpToolbarEvent,
};
use crate::models::sftp::SftpState;
use crate::state::SessionState;

/// 渲染 SFTP 面板（使用 Table 组件）
/// 布局结构：
/// ┌─────────────────────────────────────────────────────────────────┐
/// │ [←][→][↑][🏠] │     /home/user/path/to/folder      │ [🔄][📁][👁] │
/// ├─────────────────┬───────────────────────────────────────────────┤
/// │                 │                                               │
/// │    文件夹树      │              文件列表                         │
/// │                 │                                               │
/// └─────────────────┴───────────────────────────────────────────────┘
pub fn render_sftp_panel(
    sftp_state: Option<&SftpState>,
    file_list_view: Option<Entity<FileListView>>,
    path_bar_state: Option<Entity<PathBarState>>,
    session_state: Entity<SessionState>,
    tab_id: String,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    // === 创建事件处理闭包 ===
    let session_for_toolbar = session_state.clone();
    let tab_id_for_toolbar = tab_id.clone();
    let file_list_for_toolbar = file_list_view.clone();

    let on_toolbar_event = move |event: SftpToolbarEvent, cx: &mut App| {
        session_for_toolbar.update(cx, |state, cx| match event {
            SftpToolbarEvent::GoBack => state.sftp_go_back(&tab_id_for_toolbar, cx),
            SftpToolbarEvent::GoForward => state.sftp_go_forward(&tab_id_for_toolbar, cx),
            SftpToolbarEvent::GoUp => state.sftp_go_up(&tab_id_for_toolbar, cx),
            SftpToolbarEvent::GoHome => state.sftp_go_home(&tab_id_for_toolbar, cx),
            SftpToolbarEvent::Refresh => state.sftp_refresh(&tab_id_for_toolbar, cx),
            SftpToolbarEvent::ToggleHidden => state.sftp_toggle_hidden(&tab_id_for_toolbar, cx),
            SftpToolbarEvent::NewFolder => {
                state.sftp_open_new_folder_dialog(&tab_id_for_toolbar, cx);
            }
            SftpToolbarEvent::Upload => {
                // 获取当前SFTP路径
                if let Some(current_path) = state
                    .tabs
                    .iter()
                    .find(|t| t.id == tab_id_for_toolbar)
                    .and_then(|t| t.sftp_state.as_ref())
                    .map(|s| s.current_path.clone())
                {
                    state.sftp_upload_file(&tab_id_for_toolbar, current_path, cx);
                }
            }
            SftpToolbarEvent::Download => {
                // 获取选中的文件
                if let Some(ref file_list) = file_list_for_toolbar {
                    if let Some(file) = file_list.read(cx).get_selected_file(cx) {
                        // 只允许下载文件，不支持目录
                        if file.file_type != FileType::Directory {
                            state.sftp_download_file(
                                &tab_id_for_toolbar,
                                file.path.clone(),
                                file.name.clone(),
                                file.size,
                                cx,
                            );
                        }
                    }
                }
            }
        });
    };

    let session_for_tree = session_state.clone();
    let tab_id_for_tree = tab_id.clone();

    let on_folder_tree_event = move |event: FolderTreeEvent, cx: &mut App| {
        session_for_tree.update(cx, |state, cx| match event {
            FolderTreeEvent::ToggleExpand(path) => {
                state.sftp_toggle_expand(&tab_id_for_tree, path, cx)
            }
            FolderTreeEvent::SelectDir(path) => state.sftp_navigate_to(&tab_id_for_tree, path, cx),
        });
    };

    // === 顶部工具栏 ===
    let toolbar: AnyElement = if let Some(path_bar) = path_bar_state {
        render_sftp_toolbar(sftp_state, path_bar, on_toolbar_event, cx).into_any_element()
    } else {
        // 回退：显示空占位
        let bg_color = crate::theme::sidebar_color(cx);
        let border_color = cx.theme().border;
        div()
            .w_full()
            .h(px(32.))
            .flex_shrink_0()
            .bg(bg_color)
            .border_b_1()
            .border_color(border_color)
            .into_any_element()
    };

    // === 左侧内容区：文件夹树 ===
    let folder_tree = render_folder_tree(&tab_id, sftp_state, on_folder_tree_event, window, cx);

    // === 右侧内容区：文件列表（使用 Table Entity） ===
    let file_list: AnyElement = if let Some(view) = file_list_view {
        // 直接使用已同步的 FileListView（数据同步在 page.rs 中完成）
        view.into_any_element()
    } else {
        // 回退：显示提示信息
        let bg_color = crate::theme::sidebar_color(cx);
        let muted_foreground = cx.theme().muted_foreground;
        div()
            .size_full()
            .bg(bg_color)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(muted_foreground)
                    .child("Loading..."),
            )
            .into_any_element()
    };

    // === 下方内容区：使用水平可调整大小布局分隔文件夹树和文件列表 ===
    let content_area = h_resizable("sftp-panel-h")
        .child(
            resizable_panel()
                .size(px(229.)) // 文件夹树初始宽度，与 Monitor 面板对齐
                .child(folder_tree),
        )
        .child(resizable_panel().child(file_list));

    // === 整体布局：工具栏 + 内容区 ===
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(toolbar)
        // 用 div 包装 content_area 以应用 flex_1 和 min_h，确保滚动正常工作
        .child(
            div()
                .flex_1()
                .min_h(px(0.))
                .overflow_hidden()
                .child(content_area),
        )
}
