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
fn render_tree_item<F1, F2>(
    entry: &FileEntry,
    depth: usize,
    is_expanded: bool,
    is_selected: bool,
    on_toggle: F1,
    on_select: F2,
    cx: &App,
) -> AnyElement
where
    F1: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    F2: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let selected_bg = cx.theme().list_active;
    let hover_bg = cx.theme().list_active_border;

    let indent = px(INDENT_WIDTH * depth as f32);

    // 展开/折叠图标 - 可点击切换
    let expand_icon = if entry.is_dir() {
        let icon = if is_expanded {
            icons::CHEVRON_DOWN
        } else {
            icons::CHEVRON_RIGHT
        };
        div()
            .id(SharedString::from(format!(
                "sftp-tree-expand-{}",
                entry.path
            )))
            .size(px(14.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, on_toggle)
            .child(svg().path(icon).size(px(10.)).text_color(muted))
    } else {
        div()
            .id(SharedString::from(format!(
                "sftp-tree-expand-empty-{}",
                entry.path
            )))
            .size(px(14.))
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
        .id(SharedString::from(format!("sftp-tree-item-{}", entry.path)))
        .w_full()
        .h(px(ITEM_HEIGHT))
        .flex()
        .items_center()
        .pl(indent)
        .pr_2()
        .gap_1()
        .cursor_pointer()
        .hover(|s| s.bg(hover_bg))
        .on_mouse_down(MouseButton::Left, on_select)
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

fn collect_tree_items<F>(
    path: &str,
    depth: usize,
    state: &SftpState,
    on_event: &F,
    cx: &App,
    items: &mut Vec<AnyElement>,
) where
    F: Fn(FolderTreeEvent, &mut App) + Clone + 'static,
{
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

        let entry_path = entry.path.clone();
        let on_toggle = on_event.clone();
        let toggle_path = entry_path.clone();

        let on_select = on_event.clone();
        let select_path = entry_path.clone();

        items.push(render_tree_item(
            entry,
            depth,
            is_expanded,
            is_selected,
            move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                on_toggle(FolderTreeEvent::ToggleExpand(toggle_path.clone()), cx);
            },
            move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                on_select(FolderTreeEvent::SelectDir(select_path.clone()), cx);
            },
            cx,
        ));

        // 如果展开了，递归渲染子目录
        if is_expanded {
            collect_tree_items(&entry.path, depth + 1, state, on_event, cx, items);
        }
    }
}

pub fn render_folder_tree<F>(state: Option<&SftpState>, on_event: F, cx: &App) -> impl IntoElement
where
    F: Fn(FolderTreeEvent, &mut App) + Clone + 'static,
{
    let bg_color = crate::theme::sidebar_color(cx);
    let muted_foreground = cx.theme().muted_foreground;

    match state {
        Some(s) => {
            let mut items: Vec<AnyElement> = Vec::new();

            // 根目录
            let is_root_expanded = s.is_expanded("/");
            let on_root_toggle = on_event.clone();
            let on_root_select = on_event.clone();

            items.push(
                div()
                    .id("sftp-tree-root")
                    .w_full()
                    .h(px(ITEM_HEIGHT))
                    .flex()
                    .items_center()
                    .px_2()
                    .gap_1()
                    .cursor_pointer()
                    .hover(|el| el.bg(cx.theme().list_active_border))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        on_root_select(FolderTreeEvent::SelectDir("/".to_string()), cx);
                    })
                    .child(
                        div()
                            .id("sftp-tree-root-expand")
                            .size(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                on_root_toggle(FolderTreeEvent::ToggleExpand("/".to_string()), cx);
                            })
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
                collect_tree_items("/", 1, s, &on_event, cx, &mut items);
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
