// SFTP 文件夹树组件
// 层级展示目录结构，支持懒加载和展开/折叠

use std::sync::Arc;

use gpui::*;
use gpui_component::scroll::ScrollableElement;
use gpui_component::ActiveTheme;

use crate::constants::icons;
use crate::models::sftp::SftpState;

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

#[derive(Clone, Debug)]
struct FolderTreeRow {
    path: String,
    name: String,
    depth: usize,
    is_expanded: bool,
}

struct FolderTreeViewState {
    scroll_handle: UniformListScrollHandle,
    rows: Arc<Vec<FolderTreeRow>>,
    last_dir_cache_revision: u64,
    last_expanded_revision: u64,
}

impl FolderTreeViewState {
    fn new() -> Self {
        Self {
            scroll_handle: UniformListScrollHandle::new(),
            rows: Arc::new(Vec::new()),
            last_dir_cache_revision: 0,
            last_expanded_revision: 0,
        }
    }

    fn sync_rows(&mut self, state: &SftpState) {
        let needs_rebuild = self.last_dir_cache_revision != state.dir_cache_revision
            || self.last_expanded_revision != state.expanded_dirs_revision
            || self.rows.is_empty();

        if !needs_rebuild {
            return;
        }

        let mut rows = Vec::new();
        let root_expanded = state.is_expanded("/");

        rows.push(FolderTreeRow {
            path: "/".to_string(),
            name: "/".to_string(),
            depth: 0,
            is_expanded: root_expanded,
        });

        if root_expanded {
            collect_tree_rows("/", 1, state, &mut rows);
        }

        self.rows = Arc::new(rows);
        self.last_dir_cache_revision = state.dir_cache_revision;
        self.last_expanded_revision = state.expanded_dirs_revision;
    }
}

/// 渲染单个树节点
fn render_tree_row<F1, F2>(
    row: &FolderTreeRow,
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

    let indent = px(INDENT_WIDTH * row.depth as f32);

    // 展开/折叠图标 - 可点击切换
    let expand_icon = {
        let icon = if row.is_expanded {
            icons::CHEVRON_DOWN
        } else {
            icons::CHEVRON_RIGHT
        };
        div()
            .id(SharedString::from(format!("sftp-tree-expand-{}", row.path)))
            .size(px(14.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, on_toggle)
            .child(svg().path(icon).size(px(10.)).text_color(muted))
    };

    // 文件夹图标
    let folder_icon = svg()
        .path(if row.is_expanded {
            icons::FOLDER_OPEN
        } else {
            icons::FOLDER
        })
        .size(px(14.))
        .text_color(muted);

    let mut el = div()
        .id(SharedString::from(format!("sftp-tree-item-{}", row.path)))
        .w_full()
        .h(px(ITEM_HEIGHT))
        .flex()
        .items_center()
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
                .child(row.name.clone()),
        );

    if row.depth == 0 {
        el = el.px_2();
    } else {
        el = el.pl(indent).pr_2();
    }

    if is_selected {
        el = el.bg(selected_bg);
    }

    el.into_any_element()
}

fn collect_tree_rows(
    path: &str,
    depth: usize,
    state: &SftpState,
    rows: &mut Vec<FolderTreeRow>,
) {
    // 获取该路径下的缓存目录
    let entries = match state.dir_cache.get(path) {
        Some(cached) => &cached.entries,
        None => return,
    };

    // 只显示目录
    for entry in entries.iter().filter(|e| e.is_dir()) {
        let is_expanded = state.is_expanded(&entry.path);
        rows.push(FolderTreeRow {
            path: entry.path.clone(),
            name: entry.name.clone(),
            depth,
            is_expanded,
        });

        // 如果展开了，递归渲染子目录
        if is_expanded {
            collect_tree_rows(&entry.path, depth + 1, state, rows);
        }
    }
}

pub fn render_folder_tree<F>(
    tab_id: &str,
    state: Option<&SftpState>,
    on_event: F,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement
where
    F: Fn(FolderTreeEvent, &mut App) + Clone + 'static,
{
    let bg_color = crate::theme::sidebar_color(cx);
    let muted_foreground = cx.theme().muted_foreground;

    let view_state = window.use_keyed_state(
        SharedString::from(format!("sftp-folder-tree-state-{}", tab_id)),
        cx,
        |_, _| FolderTreeViewState::new(),
    );

    match state {
        Some(s) => {
            view_state.update(cx, |view, _| view.sync_rows(s));

            let rows = view_state.read(cx).rows.clone();
            let scroll_handle = view_state.read(cx).scroll_handle.clone();
            let current_path = s.current_path.clone();
            let on_event = on_event.clone();

            let list = uniform_list(
                SharedString::from(format!("sftp-folder-tree-list-{}", tab_id)),
                rows.len(),
                move |range, _window, cx| {
                    let mut items = Vec::with_capacity(range.len());
                    for ix in range {
                        let row = &rows[ix];
                        let is_selected = row.path == current_path;
                        let on_toggle = on_event.clone();
                        let on_select = on_event.clone();
                        let toggle_path = row.path.clone();
                        let select_path = row.path.clone();

                        items.push(render_tree_row(
                            row,
                            is_selected,
                            move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                                on_toggle(FolderTreeEvent::ToggleExpand(toggle_path.clone()), cx);
                            },
                            move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                                on_select(FolderTreeEvent::SelectDir(select_path.clone()), cx);
                            },
                            cx,
                        ));
                    }
                    items
                },
            )
            .track_scroll(scroll_handle.clone())
            .with_sizing_behavior(ListSizingBehavior::Auto)
            .size_full();

            div()
                .id(SharedString::from(format!("sftp-folder-tree-scroll-{}", tab_id)))
                .flex_1()
                .min_h(px(0.))
                .bg(bg_color)
                .child(list)
                .vertical_scrollbar(&scroll_handle)
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
