// SFTP 文件夹树组件
// 层级展示目录结构，支持懒加载和展开/折叠

use gpui::*;
use gpui_component::ActiveTheme;

use crate::constants::icons;
use crate::models::sftp::{FileEntry, SftpState};

/// 树节点缩进宽度
const INDENT_WIDTH: f32 = 16.0;
/// 树项高度
const ITEM_HEIGHT: f32 = 24.0;

/// 文件夹树事件
#[derive(Clone, Debug)]
pub enum FolderTreeEvent {
    /// 切换目录展开状态
    ToggleExpand(String),
    /// 选择目录（导航到）
    SelectDir(String),
}

/// 渲染单个树节点
fn render_tree_item(
    entry: &FileEntry,
    depth: usize,
    is_expanded: bool,
    is_selected: bool,
    cx: &App,
) -> AnyElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let selected_bg = cx.theme().list_active;
    let hover_bg = cx.theme().list_active_border;

    let indent = px(INDENT_WIDTH * depth as f32);

    // 展开/折叠图标
    let expand_icon = if entry.is_dir() {
        let icon = if is_expanded {
            icons::CHEVRON_DOWN
        } else {
            icons::CHEVRON_RIGHT
        };
        div()
            .size(px(14.))
            .flex()
            .items_center()
            .justify_center()
            .child(svg().path(icon).size(px(10.)).text_color(muted))
    } else {
        div().size(px(14.))
    };

    // 文件夹图标
    let folder_icon = svg()
        .path(if is_expanded {
            icons::FOLDER_OPEN
        } else {
            icons::FOLDER
        })
        .size(px(14.))
        .text_color(muted);

    let mut el = div()
        .w_full()
        .h(px(ITEM_HEIGHT))
        .flex()
        .items_center()
        .pl(indent)
        .pr_2()
        .gap_1()
        .cursor_pointer()
        .hover(|s| s.bg(hover_bg))
        .child(expand_icon)
        .child(folder_icon)
        .child(
            div()
                .flex_1()
                .text_xs()
                .text_color(foreground)
                .overflow_hidden()
                .text_ellipsis()
                .child(entry.name.clone()),
        );

    if is_selected {
        el = el.bg(selected_bg);
    }

    el.into_any_element()
}

/// 递归渲染目录树（收集到 Vec 中）
fn collect_tree_items(
    path: &str,
    depth: usize,
    state: &SftpState,
    cx: &App,
    items: &mut Vec<AnyElement>,
) {
    // 获取该路径下的缓存目录
    let entries = match state.dir_cache.get(path) {
        Some(cached) => &cached.entries,
        None => return,
    };

    // 只显示目录
    let dirs: Vec<_> = entries.iter().filter(|e| e.is_dir()).collect();

    for entry in dirs {
        let is_expanded = state.is_expanded(&entry.path);
        let is_selected = state.current_path == entry.path;

        items.push(render_tree_item(entry, depth, is_expanded, is_selected, cx));

        // 如果展开了，递归渲染子目录
        if is_expanded {
            collect_tree_items(&entry.path, depth + 1, state, cx, items);
        }
    }
}

/// 渲染文件夹树
pub fn render_folder_tree(state: Option<&SftpState>, cx: &App) -> impl IntoElement {
    let bg_color = crate::theme::sidebar_color(cx);
    let muted_foreground = cx.theme().muted_foreground;

    match state {
        Some(s) => {
            let mut items: Vec<AnyElement> = Vec::new();

            // 根目录
            let is_root_expanded = s.is_expanded("/");
            items.push(
                div()
                    .w_full()
                    .h(px(ITEM_HEIGHT))
                    .flex()
                    .items_center()
                    .px_2()
                    .gap_1()
                    .cursor_pointer()
                    .hover(|el| el.bg(cx.theme().list_active_border))
                    .child(
                        div()
                            .size(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                svg()
                                    .path(if is_root_expanded {
                                        icons::CHEVRON_DOWN
                                    } else {
                                        icons::CHEVRON_RIGHT
                                    })
                                    .size(px(10.))
                                    .text_color(muted_foreground),
                            ),
                    )
                    .child(
                        svg()
                            .path(if is_root_expanded {
                                icons::FOLDER_OPEN
                            } else {
                                icons::FOLDER
                            })
                            .size(px(14.))
                            .text_color(muted_foreground),
                    )
                    .child(div().text_xs().text_color(cx.theme().foreground).child("/"))
                    .into_any_element(),
            );

            // 递归收集（如果根目录展开）
            if is_root_expanded {
                collect_tree_items("/", 1, s, cx, &mut items);
            }

            div()
                .id("sftp-folder-tree-scroll")
                .flex_1()
                .min_h(px(0.))
                .bg(bg_color)
                .overflow_y_scroll()
                .child(div().flex().flex_col().children(items))
                .into_any_element()
        }
        None => {
            // 加载中或未连接
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
                        .child("正在加载..."),
                )
                .into_any_element()
        }
    }
}
