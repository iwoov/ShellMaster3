// SFTP 文件夹树组件
// 层级展示目录结构，支持懒加载、展开/折叠、自动滚动定位当前目录与右键快捷菜单

use std::sync::Arc;

use gpui_kit::component::menu::ContextMenuExt;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::file_list::menu_item_element;
use crate::components::common::icon::icon_sized;
use crate::components::common::panel::{panel_state, spinner, StateIcon, LIST_ROW_HEIGHT};
use crate::components::common::text::TextFitter;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::sftp::SftpState;
use crate::theme::Palette;

/// 每一级缩进的宽度
const INDENT_WIDTH: f32 = 12.0;
/// 树和列两侧的留白（行的圆角底色不贴边）
const TREE_INSET: f32 = 6.0;
/// 行内左右留白
const ROW_PADDING: f32 = 6.0;
/// 展开箭头所在的方框、文件夹图标的尺寸，以及它们之间的间距
const CHEVRON_BOX: f32 = 16.0;
const FOLDER_ICON: f32 = 16.0;
const ICON_GAP: f32 = 6.0;
/// 行内用于联动悬停样式的分组名
const ROW_GROUP: &str = "sftp-tree-row";

/// 文件夹树事件
#[derive(Clone, Debug)]
pub enum FolderTreeEvent {
    /// 切换目录展开状态
    ToggleExpand(String),
    /// 选择目录（导航到）
    SelectDir(String),
    /// 刷新指定目录
    RefreshDir(String),
    /// 在指定目录下新建文件夹
    NewFolder(String),
    /// 下载指定文件夹
    DownloadFolder(String),
    /// 在终端中打开该目录
    OpenInTerminal(String),
    /// 复制目录路径
    CopyPath(String),
}

/// 子目录的情况：决定箭头怎么画
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Children {
    /// 还没读取过，可能有子目录
    Unknown,
    /// 已展开但正在读取
    Loading,
    /// 读取过，有子目录
    Some,
    /// 读取过，没有子目录
    None,
}

#[derive(Clone, Debug)]
struct FolderTreeRow {
    path: String,
    name: String,
    depth: usize,
    is_expanded: bool,
    children: Children,
}

struct FolderTreeViewState {
    scroll_handle: UniformListScrollHandle,
    rows: Arc<Vec<FolderTreeRow>>,
    last_dir_cache_revision: u64,
    last_expanded_revision: u64,
    last_show_hidden: bool,
    /// 上一次自动滚动定位到的目录路径（目录变化或异步加载完成后定位到可见区）
    last_scrolled_path: String,
}

impl FolderTreeViewState {
    fn new() -> Self {
        Self {
            scroll_handle: UniformListScrollHandle::new(),
            rows: Arc::new(Vec::new()),
            last_dir_cache_revision: 0,
            last_expanded_revision: 0,
            last_show_hidden: true,
            last_scrolled_path: String::new(),
        }
    }

    fn sync_rows(&mut self, state: &SftpState) {
        let needs_rebuild = self.last_dir_cache_revision != state.dir_cache_revision
            || self.last_expanded_revision != state.expanded_dirs_revision
            || self.last_show_hidden != state.show_hidden
            || self.rows.is_empty();

        if needs_rebuild {
            let mut rows = Vec::new();
            let root_expanded = state.is_expanded("/");

            rows.push(FolderTreeRow {
                path: "/".to_string(),
                name: "/".to_string(),
                depth: 0,
                is_expanded: root_expanded,
                children: children_of("/", root_expanded, state),
            });

            if root_expanded {
                collect_tree_rows("/", 1, state, &mut rows);
            }

            self.rows = Arc::new(rows);
            self.last_dir_cache_revision = state.dir_cache_revision;
            self.last_expanded_revision = state.expanded_dirs_revision;
            self.last_show_hidden = state.show_hidden;
        }

        // 当前目录变化且节点已在树中就绪时，自动滚动至可见区域
        if !state.current_path.is_empty() && self.last_scrolled_path != state.current_path {
            if let Some(ix) = self.rows.iter().position(|r| r.path == state.current_path) {
                self.scroll_handle
                    .scroll_to_item(ix, ScrollStrategy::Center);
                self.last_scrolled_path = state.current_path.clone();
            }
        }
    }
}

/// 目录在树里要不要显示：隐藏目录跟随「显示隐藏文件」开关
fn visible_dir(entry: &crate::models::sftp::FileEntry, show_hidden: bool) -> bool {
    entry.is_dir() && (show_hidden || !entry.is_hidden())
}

fn children_of(path: &str, is_expanded: bool, state: &SftpState) -> Children {
    match state.dir_cache.get(path) {
        Some(cached) => {
            if cached
                .entries
                .iter()
                .any(|entry| visible_dir(entry, state.show_hidden))
            {
                Children::Some
            } else {
                Children::None
            }
        }
        None if is_expanded => Children::Loading,
        None => Children::Unknown,
    }
}

fn collect_tree_rows(path: &str, depth: usize, state: &SftpState, rows: &mut Vec<FolderTreeRow>) {
    let entries = match state.dir_cache.get(path) {
        Some(cached) => &cached.entries,
        None => return,
    };

    // 只显示目录，并按名称字母顺序排序（A-Z，不区分大小写）
    let mut dirs: Vec<_> = entries
        .iter()
        .filter(|entry| visible_dir(entry, state.show_hidden))
        .collect();
    dirs.sort_by_cached_key(|entry| entry.name.to_lowercase());

    for entry in dirs {
        let is_expanded = state.is_expanded(&entry.path);
        rows.push(FolderTreeRow {
            path: entry.path.clone(),
            name: entry.name.clone(),
            depth,
            is_expanded,
            children: children_of(&entry.path, is_expanded, state),
        });

        if is_expanded {
            collect_tree_rows(&entry.path, depth + 1, state, rows);
        }
    }
}

/// 某一级目录的名称可用宽度：列宽减去两侧留白、行内留白、缩进、箭头和图标
fn label_width(column_width: Pixels, depth: usize) -> Pixels {
    let used = 1. // 列的右边线
        + TREE_INSET * 2.
        + ROW_PADDING * 2.
        + INDENT_WIDTH * depth as f32
        + CHEVRON_BOX
        + 2.
        + FOLDER_ICON
        + ICON_GAP;
    (column_width - px(used)).max(px(24.))
}

/// 渲染单个树节点（支持右键上下文菜单）
#[allow(clippy::too_many_arguments)]
fn render_tree_row<F>(
    row: &FolderTreeRow,
    is_selected: bool,
    column_width: Pixels,
    lang: &Language,
    fit: &TextFitter,
    p: &Palette,
    on_event: F,
) -> AnyElement
where
    F: Fn(FolderTreeEvent, &mut App) + Clone + 'static,
{
    let hover_bg = p.hover;
    let hover_text = p.text;

    let chevron: AnyElement = match row.children {
        Children::Loading => spinner(
            SharedString::from(format!("sftp-tree-loading-{}", row.path)),
            11.,
            p.text_faint,
        )
        .into_any_element(),
        Children::None => div().into_any_element(),
        Children::Unknown | Children::Some => icon_sized(
            if row.is_expanded {
                icons::CHEVRON_DOWN
            } else {
                icons::CHEVRON_RIGHT
            },
            12.,
            p.text_faint,
        )
        .group_hover(ROW_GROUP, move |s| s.text_color(hover_text))
        .into_any_element(),
    };

    let toggle_path = row.path.clone();
    let on_toggle = on_event.clone();
    let toggle = div()
        .id(SharedString::from(format!("sftp-tree-expand-{}", row.path)))
        .flex_none()
        .size(px(CHEVRON_BOX))
        .rounded(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .when(row.children != Children::None, |toggle| {
            toggle
                .cursor_pointer()
                .hover(move |s| s.bg(p.hover_strong))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    on_toggle(FolderTreeEvent::ToggleExpand(toggle_path.clone()), cx);
                })
        })
        .child(chevron);

    let folder_icon = icon_sized(
        if row.is_expanded {
            icons::FOLDER_OPEN
        } else {
            icons::FOLDER
        },
        FOLDER_ICON,
        if is_selected { p.primary } else { p.text_muted },
    );

    let select_path = row.path.clone();
    let on_select = on_event.clone();
    let menu_path = row.path.clone();
    let lang_for_menu = lang.clone();

    div()
        .px(px(TREE_INSET))
        .child(
            div()
                .id(SharedString::from(format!("sftp-tree-item-{}", row.path)))
                .group(ROW_GROUP)
                .h(LIST_ROW_HEIGHT)
                .pl(px(ROW_PADDING + INDENT_WIDTH * row.depth as f32))
                .pr(px(ROW_PADDING))
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap(px(2.))
                .cursor_pointer()
                .when(is_selected, |row| row.bg(p.selected))
                .when(!is_selected, |row| row.hover(move |s| s.bg(hover_bg)))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    on_select(FolderTreeEvent::SelectDir(select_path.clone()), cx);
                })
                .child(toggle)
                .child(div().flex_none().mr(px(ICON_GAP - 2.)).child(folder_icon))
                .child(
                    fit.label_with_tooltip(
                        SharedString::from(format!("sftp-tree-label-{}", row.path)),
                        &row.name,
                        label_width(column_width, row.depth),
                        13.,
                        if is_selected {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        },
                    )
                    .flex_none()
                    .text_color(if is_selected { p.text } else { p.text_muted })
                    .group_hover(ROW_GROUP, move |s| s.text_color(hover_text)),
                )
                .context_menu(move |menu, _, _| {
                    let open_label = i18n::t(&lang_for_menu, "sftp.context_menu.open_folder");
                    let refresh_label = i18n::t(&lang_for_menu, "sftp.context_menu.refresh");
                    let new_folder_label =
                        i18n::t(&lang_for_menu, "sftp.context_menu.new_folder");
                    let download_label =
                        i18n::t(&lang_for_menu, "sftp.context_menu.download_folder");
                    let terminal_label =
                        i18n::t(&lang_for_menu, "sftp.context_menu.open_in_terminal");
                    let copy_path_label =
                        i18n::t(&lang_for_menu, "sftp.context_menu.copy_path");

                    let p_open = menu_path.clone();
                    let p_refresh = menu_path.clone();
                    let p_new = menu_path.clone();
                    let p_download = menu_path.clone();
                    let p_term = menu_path.clone();
                    let p_copy = menu_path.clone();

                    let e_open = on_event.clone();
                    let e_refresh = on_event.clone();
                    let e_new = on_event.clone();
                    let e_download = on_event.clone();
                    let e_term = on_event.clone();
                    let e_copy = on_event.clone();

                    menu.item(
                        menu_item_element(icons::FOLDER_OPEN, open_label).on_click(
                            move |_, _, cx| {
                                e_open(FolderTreeEvent::SelectDir(p_open.clone()), cx);
                            },
                        ),
                    )
                    .item(
                        menu_item_element(icons::REFRESH, refresh_label).on_click(
                            move |_, _, cx| {
                                e_refresh(FolderTreeEvent::RefreshDir(p_refresh.clone()), cx);
                            },
                        ),
                    )
                    .separator()
                    .item(
                        menu_item_element(icons::FOLDER_PLUS, new_folder_label).on_click(
                            move |_, _, cx| {
                                e_new(FolderTreeEvent::NewFolder(p_new.clone()), cx);
                            },
                        ),
                    )
                    .item(
                        menu_item_element(icons::DOWNLOAD, download_label).on_click(
                            move |_, _, cx| {
                                e_download(
                                    FolderTreeEvent::DownloadFolder(p_download.clone()),
                                    cx,
                                );
                            },
                        ),
                    )
                    .separator()
                    .item(
                        menu_item_element(icons::TERMINAL, terminal_label).on_click(
                            move |_, _, cx| {
                                e_term(FolderTreeEvent::OpenInTerminal(p_term.clone()), cx);
                            },
                        ),
                    )
                    .item(
                        menu_item_element(icons::COPY, copy_path_label).on_click(
                            move |_, _, cx| {
                                e_copy(FolderTreeEvent::CopyPath(p_copy.clone()), cx);
                            },
                        ),
                    )
                }),
        )
        .into_any_element()
}

/// 渲染目录树。`column_width` 是目录树这一栏的宽度（含右边线）
#[allow(clippy::too_many_arguments)]
pub fn render_folder_tree<F>(
    tab_id: &str,
    state: Option<&SftpState>,
    column_width: Pixels,
    lang: &Language,
    fit: &TextFitter,
    on_event: F,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement
where
    F: Fn(FolderTreeEvent, &mut App) + Clone + 'static,
{
    let p = crate::theme::palette(cx);

    let view_state = window.use_keyed_state(
        SharedString::from(format!("sftp-folder-tree-state-{}", tab_id)),
        cx,
        |_, _| FolderTreeViewState::new(),
    );

    let Some(state) = state else {
        // SFTP 还没连上
        return div()
            .size_full()
            .bg(p.sidebar)
            .child(panel_state(
                "sftp-tree-connecting",
                StateIcon::Spinner,
                p.text_faint,
                i18n::t(lang, "sftp.connecting"),
                None,
                &p,
            ))
            .into_any_element();
    };

    view_state.update(cx, |view, _| view.sync_rows(state));

    let rows = view_state.read(cx).rows.clone();
    let scroll_handle = view_state.read(cx).scroll_handle.clone();
    let current_path = state.current_path.clone();
    let fit = fit.clone();
    let lang = lang.clone();

    let list = uniform_list(
        SharedString::from(format!("sftp-folder-tree-list-{}", tab_id)),
        rows.len(),
        move |range, _window, _cx| {
            range
                .map(|ix| {
                    let row = &rows[ix];
                    let is_selected = row.path == current_path;
                    render_tree_row(
                        row,
                        is_selected,
                        column_width,
                        &lang,
                        &fit,
                        &p,
                        on_event.clone(),
                    )
                })
                .collect()
        },
    )
    .track_scroll(&scroll_handle)
    .with_sizing_behavior(ListSizingBehavior::Auto)
    .size_full()
    .py(px(4.));

    div()
        .id(SharedString::from(format!(
            "sftp-folder-tree-scroll-{}",
            tab_id
        )))
        .flex_1()
        .min_h(px(0.))
        .bg(p.sidebar)
        .child(list)
        .vertical_scrollbar(&scroll_handle)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{
        label_width, CHEVRON_BOX, FOLDER_ICON, ICON_GAP, INDENT_WIDTH, ROW_PADDING, TREE_INSET,
    };
    use gpui_kit::px;

    #[test]
    fn deeper_rows_get_less_room_for_the_name() {
        let fixed =
            1. + TREE_INSET * 2. + ROW_PADDING * 2. + CHEVRON_BOX + 2. + FOLDER_ICON + ICON_GAP;
        assert_eq!(label_width(px(220.), 0), px(220. - fixed));
        assert_eq!(
            label_width(px(220.), 3),
            px(220. - fixed - INDENT_WIDTH * 3.)
        );
        // 再深也留出最小宽度
        assert_eq!(label_width(px(220.), 40), px(24.));
    }
}
