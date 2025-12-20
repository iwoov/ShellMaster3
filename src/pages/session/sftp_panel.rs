// SFTP 面板组件 - 包含文件夹树和文件列表

use gpui::*;
use gpui_component::resizable::{h_resizable, resizable_panel};
use gpui_component::ActiveTheme;

/// 渲染 SFTP 面板
/// 左侧为文件夹树，右侧为文件列表，通过可拖动分隔条划分
pub fn render_sftp_panel(cx: &App) -> impl IntoElement {
    let muted_foreground = cx.theme().muted_foreground;
    let bg_color = crate::theme::sidebar_color(cx);

    // 左侧：文件夹树（初始宽度 219px，与上方 Monitor 面板对齐）
    let folder_tree = div()
        .size_full()
        .bg(bg_color)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_sm()
                .text_color(muted_foreground)
                .child("文件夹树"),
        );

    // 右侧：文件列表
    let file_list = div()
        .size_full()
        .bg(bg_color)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_sm()
                .text_color(muted_foreground)
                .child("文件列表"),
        );

    // 使用水平可调整大小布局
    h_resizable("sftp-panel-h")
        .child(
            resizable_panel()
                .size(px(229.)) // 文件夹树初始宽度 230px
                .child(folder_tree),
        )
        .child(resizable_panel().child(file_list))
}
