// SFTP 面板组件 - 包含导航工具栏、文件夹树和文件列表

use gpui::*;
use gpui_component::resizable::{h_resizable, resizable_panel};
use gpui_component::table::TableEvent;
use gpui_component::ActiveTheme;

use crate::components::sftp::{
    render_folder_tree, render_sftp_toolbar, FileListView, FolderTreeEvent, SftpToolbarEvent,
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
    session_state: Entity<SessionState>,
    tab_id: String,
    cx: &App,
) -> impl IntoElement {
    // === 创建事件处理闭包 ===
    let session_for_toolbar = session_state.clone();
    let tab_id_for_toolbar = tab_id.clone();

    let on_toolbar_event = move |event: SftpToolbarEvent, cx: &mut App| {
        session_for_toolbar.update(cx, |state, cx| match event {
            SftpToolbarEvent::GoBack => state.sftp_go_back(&tab_id_for_toolbar, cx),
            SftpToolbarEvent::GoForward => state.sftp_go_forward(&tab_id_for_toolbar, cx),
            SftpToolbarEvent::GoUp => state.sftp_go_up(&tab_id_for_toolbar, cx),
            SftpToolbarEvent::GoHome => state.sftp_go_home(&tab_id_for_toolbar, cx),
            SftpToolbarEvent::Refresh => state.sftp_refresh(&tab_id_for_toolbar, cx),
            SftpToolbarEvent::ToggleHidden => state.sftp_toggle_hidden(&tab_id_for_toolbar, cx),
            SftpToolbarEvent::NavigateTo(path) => {
                state.sftp_navigate_to(&tab_id_for_toolbar, path, cx)
            }
            SftpToolbarEvent::NewFolder => {
                // TODO: 实现新建文件夹
            }
            SftpToolbarEvent::Upload => {
                // TODO: 实现上传
            }
            SftpToolbarEvent::Download => {
                // TODO: 实现下载
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
    let toolbar = render_sftp_toolbar(sftp_state, on_toolbar_event, cx);

    // === 左侧内容区：文件夹树 ===
    let folder_tree = render_folder_tree(sftp_state, on_folder_tree_event, cx);

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

/// SFTP 面板包装器 - 用于在 Entity 上下文中渲染 SFTP 面板
/// 这个组件负责订阅 TableEvent 并转发到 SessionState
#[allow(dead_code)]
pub struct SftpPanelWrapper {
    session_state: Entity<SessionState>,
    tab_id: String,
    file_list_view: Entity<FileListView>,
}

#[allow(dead_code)]
impl SftpPanelWrapper {
    pub fn new(
        session_state: Entity<SessionState>,
        tab_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // 创建 FileListView
        let file_list_view = cx.new(|cx| FileListView::new(window, cx));

        // 订阅 TableEvent
        let tab_id_for_event = tab_id.clone();
        let session_for_event = session_state.clone();
        let file_list_for_event = file_list_view.clone();
        cx.subscribe_in(
            &file_list_view,
            window,
            move |_this, _view, event: &TableEvent, _window, cx| {
                match event {
                    TableEvent::DoubleClickedRow(row_ix) => {
                        // 获取文件路径并触发打开事件
                        if let Some(path) = file_list_for_event.read(cx).get_file_path(*row_ix, cx)
                        {
                            let tab_id = tab_id_for_event.clone();
                            session_for_event.update(cx, |state, cx| {
                                state.sftp_open(&tab_id, path, cx);
                            });
                        }
                    }
                    TableEvent::SelectRow(_row_ix) => {
                        // TODO: 处理选择事件
                    }
                    _ => {}
                }
            },
        )
        .detach();

        Self {
            session_state,
            tab_id,
            file_list_view,
        }
    }
}

impl Render for SftpPanelWrapper {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 先克隆 sftp_state 以避免借用冲突
        let sftp_state_clone = self
            .session_state
            .read(cx)
            .tabs
            .iter()
            .find(|t| t.id == self.tab_id)
            .and_then(|t| t.sftp_state.clone());

        // 同步数据
        self.file_list_view.update(cx, |v, cx| {
            v.sync_from_sftp_state(sftp_state_clone.as_ref(), cx);
        });

        // 重新获取引用用于渲染
        let sftp_state = self
            .session_state
            .read(cx)
            .tabs
            .iter()
            .find(|t| t.id == self.tab_id)
            .and_then(|t| t.sftp_state.as_ref());

        render_sftp_panel(
            sftp_state,
            Some(self.file_list_view.clone()),
            self.session_state.clone(),
            self.tab_id.clone(),
            cx,
        )
    }
}
