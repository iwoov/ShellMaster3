// SFTP 面板：左列目录树（与上方监控同宽、同底色），右列文件列表，各自带 36px 表头
//
// ┌──────────────────┬───────────────────────────────────────────────────┐
// │ 远程文件  ⌂ ⟳ 👁   │ ← → ↑ [ / › var › www › app      ] 🔍 ⊕ ⇧ ⇩   │
// ├──────────────────┼───────────────────────────────────────────────────┤
// │ ▾ var            │ 名称                大小   修改时间   权限   所有者 │
// │   ▾ www          │ …                                                 │
// └──────────────────┴───────────────────────────────────────────────────┘

use gpui_kit::*;

use crate::components::common::text::TextFitter;
use crate::components::sftp::{
    render_folder_tree, render_sftp_list_header, render_sftp_tree_header, FileListView,
    FolderTreeEvent, PathBarState, SftpToolbarEvent,
};
use crate::models::settings::Language;
use crate::models::sftp::{FileType, SftpState};
use crate::state::SessionState;

/// 渲染 SFTP 面板。`tree_width` 是左列宽度（含右边线），`list_width` 是右列可用宽度
#[allow(clippy::too_many_arguments)]
pub fn render_sftp_panel(
    sftp_state: Option<&SftpState>,
    file_list_view: Option<Entity<FileListView>>,
    path_bar_state: Option<Entity<PathBarState>>,
    session_state: Entity<SessionState>,
    tab_id: String,
    tree_width: Pixels,
    list_width: Pixels,
    lang: &Language,
    fit: &TextFitter,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);

    // 根据右列可用宽度自适应调整名称列宽（用户手动拖动过列宽时保持不变）
    if let Some(ref view) = file_list_view {
        view.update(cx, |v, cx| {
            v.sync_available_width(list_width, cx);
        });
    }

    // === 工具栏事件 ===
    let session_for_toolbar = session_state.clone();
    let tab_id_for_toolbar = tab_id.clone();
    let file_list_for_toolbar = file_list_view.clone();

    let on_toolbar_event = move |event: SftpToolbarEvent, window: &mut Window, cx: &mut App| {
        if let SftpToolbarEvent::ToggleFilter = event {
            if let Some(ref file_list) = file_list_for_toolbar {
                file_list.update(cx, |view, cx| {
                    view.toggle_filter(window, cx);
                });
            }
            return;
        }

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
            SftpToolbarEvent::UploadFile => {
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
            SftpToolbarEvent::UploadFolder => {
                if let Some(current_path) = state
                    .tabs
                    .iter()
                    .find(|t| t.id == tab_id_for_toolbar)
                    .and_then(|t| t.sftp_state.as_ref())
                    .map(|s| s.current_path.clone())
                {
                    state.sftp_upload_folder_with_picker(&tab_id_for_toolbar, current_path, cx);
                }
            }
            SftpToolbarEvent::Download => {
                // 下载选中的文件或文件夹（没有选中时按钮是禁用的）
                if let Some(ref file_list) = file_list_for_toolbar {
                    if let Some(file) = file_list.read(cx).get_selected_file(cx) {
                        if file.file_type == FileType::Directory {
                            state.sftp_download_folder_with_picker(
                                &tab_id_for_toolbar,
                                file.path.clone(),
                                cx,
                            );
                        } else {
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
            SftpToolbarEvent::ToggleFilter => {}
        });
    };

    // === 目录树事件 ===
    let session_for_tree = session_state.clone();
    let tab_id_for_tree = tab_id.clone();
    let on_folder_tree_event = move |event: FolderTreeEvent, cx: &mut App| match event {
        FolderTreeEvent::CopyPath(path) => {
            cx.write_to_clipboard(ClipboardItem::new_string(path));
        }
        other => {
            session_for_tree.update(cx, |state, cx| match other {
                FolderTreeEvent::ToggleExpand(path) => {
                    state.sftp_toggle_expand(&tab_id_for_tree, path, cx)
                }
                FolderTreeEvent::SelectDir(path) => {
                    state.sftp_navigate_to(&tab_id_for_tree, path, cx)
                }
                FolderTreeEvent::RefreshDir(path) => {
                    state.sftp_refresh_dir(&tab_id_for_tree, path, cx)
                }
                FolderTreeEvent::NewFolder(path) => {
                    state.sftp_open_new_folder_dialog_in(&tab_id_for_tree, path, cx)
                }
                FolderTreeEvent::DownloadFolder(path) => {
                    state.sftp_download_folder_with_picker(&tab_id_for_tree, path, cx)
                }
                FolderTreeEvent::OpenInTerminal(path) => {
                    state.sftp_open_in_terminal(&tab_id_for_tree, path, cx)
                }
                FolderTreeEvent::CopyPath(_) => {}
            });
        }
    };

    let has_selection = file_list_view
        .as_ref()
        .is_some_and(|view| view.read(cx).get_selected_file(cx).is_some());
    let filter_open = file_list_view
        .as_ref()
        .is_some_and(|view| view.read(cx).is_filter_open());

    // === 左列：表头 + 目录树 ===
    let tree_column = div()
        .flex_none()
        .w(tree_width)
        .h_full()
        .bg(p.sidebar)
        .border_r_1()
        .border_color(p.border)
        .flex()
        .flex_col()
        .child(render_sftp_tree_header(
            sftp_state,
            on_toolbar_event.clone(),
            lang,
            &p,
        ))
        .child(render_folder_tree(
            &tab_id,
            sftp_state,
            tree_width,
            lang,
            fit,
            on_folder_tree_event,
            window,
            cx,
        ));

    // === 右列：表头 + 文件列表 ===
    let file_list: AnyElement = match file_list_view {
        Some(view) => view.into_any_element(),
        // 视图在会话页每帧确保创建，这里只是兜底
        None => div().size_full().bg(p.background).into_any_element(),
    };
    let list_column = div()
        .flex_1()
        .min_w_0()
        .h_full()
        .bg(p.background)
        .flex()
        .flex_col()
        .child(render_sftp_list_header(
            sftp_state,
            path_bar_state,
            has_selection,
            filter_open,
            on_toolbar_event,
            lang,
            &p,
            cx,
        ))
        .child(
            div()
                .flex_1()
                .min_h(px(0.))
                .overflow_hidden()
                .child(file_list),
        );

    div()
        .size_full()
        .flex()
        .flex_row()
        .child(tree_column)
        .child(list_column)
}
