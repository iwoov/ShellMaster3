// SFTP 面板组件 - 包含导航工具栏、文件夹树和文件列表

use gpui::*;
use gpui_component::resizable::{h_resizable, resizable_panel};

use crate::components::sftp::{render_file_list, render_folder_tree, render_sftp_toolbar};
use crate::models::sftp::SftpState;

/// 渲染 SFTP 面板
/// 布局结构：
/// ┌─────────────────────────────────────────────────────────────────┐
/// │ [←][→][↑][🏠] │     /home/user/path/to/folder      │ [🔄][📁][👁] │
/// ├─────────────────┬───────────────────────────────────────────────┤
/// │                 │                                               │
/// │    文件夹树      │              文件列表                         │
/// │                 │                                               │
/// └─────────────────┴───────────────────────────────────────────────┘
pub fn render_sftp_panel(sftp_state: Option<&SftpState>, cx: &App) -> impl IntoElement {
    // === 顶部工具栏 ===
    let toolbar = render_sftp_toolbar(sftp_state, cx);

    // === 左侧内容区：文件夹树 ===
    let folder_tree = render_folder_tree(sftp_state, cx);

    // === 右侧内容区：文件列表 ===
    let file_list = render_file_list(sftp_state, cx);

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
