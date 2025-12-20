// SFTP 文件列表组件 - 使用 gpui_component::Table 实现
// 支持列排序和手动调整列宽

use std::cmp::Ordering;
use std::collections::HashMap;

use gpui::*;
use gpui_component::table::{Column, ColumnSort, Table, TableDelegate, TableEvent, TableState};
use gpui_component::ActiveTheme;

use crate::constants::icons;
use crate::i18n::t;
use crate::models::settings::Language;
use crate::models::sftp::{FileEntry, FileType, SftpState};

/// 图标尺寸
const ICON_SIZE: f32 = 16.0;

/// 列索引常量
const COL_NAME: usize = 0;
const COL_PERMISSIONS: usize = 1;
const COL_OWNER: usize = 2;
const COL_SIZE: usize = 3;
const COL_MODIFIED: usize = 4;

/// 文件列表事件
#[derive(Clone, Debug)]
pub enum FileListEvent {
    Select(String),
    Open(String),
    ContextMenu(String),
}

/// 文件列表 Delegate - 实现 TableDelegate trait
pub struct FileListDelegate {
    /// 文件列表数据
    pub file_list: Vec<FileEntry>,
    /// 列定义
    columns: Vec<Column>,
    /// uid -> username 缓存
    pub user_cache: HashMap<u32, String>,
    /// gid -> groupname 缓存
    pub group_cache: HashMap<u32, String>,
    /// 语言设置
    lang: Language,
    /// 当前排序列
    current_sort_col: usize,
    /// 当前排序方向
    current_sort: ColumnSort,
}

impl FileListDelegate {
    /// 创建新的 FileListDelegate
    pub fn new(lang: Language) -> Self {
        let columns = Self::create_columns(&lang);
        Self {
            file_list: Vec::new(),
            columns,
            user_cache: HashMap::new(),
            group_cache: HashMap::new(),
            lang,
            current_sort_col: COL_NAME,
            current_sort: ColumnSort::Ascending,
        }
    }

    /// 创建列定义
    fn create_columns(lang: &Language) -> Vec<Column> {
        vec![
            Column::new("name", t(lang, "sftp.header.name"))
                .width(px(200.))
                .sortable(),
            Column::new("permissions", t(lang, "sftp.header.permissions"))
                .width(px(90.))
                .resizable(true),
            Column::new("owner", t(lang, "sftp.header.owner"))
                .width(px(100.))
                .resizable(true),
            Column::new("size", t(lang, "sftp.header.size"))
                .width(px(70.))
                .sortable(),
            Column::new("modified", t(lang, "sftp.header.modified"))
                .width(px(135.))
                .sortable(),
        ]
    }

    /// 更新文件列表
    pub fn update_file_list(&mut self, entries: Vec<FileEntry>) {
        self.file_list = entries;
        // 使用当前排序状态进行排序
        self.apply_current_sort();
    }

    /// 更新用户缓存
    pub fn update_user_cache(&mut self, cache: HashMap<u32, String>) {
        self.user_cache = cache;
    }

    /// 更新组缓存
    pub fn update_group_cache(&mut self, cache: HashMap<u32, String>) {
        self.group_cache = cache;
    }

    /// 设置并应用排序
    fn sort_file_list(&mut self, col_ix: usize, sort: ColumnSort) {
        // 保存当前排序状态
        self.current_sort_col = col_ix;
        self.current_sort = sort;
        // 应用排序
        self.apply_current_sort();
    }

    /// 应用当前排序状态
    fn apply_current_sort(&mut self) {
        let col_ix = self.current_sort_col;
        let sort = self.current_sort;

        self.file_list.sort_by(|a, b| {
            // 目录始终在前
            match (a.is_dir(), b.is_dir()) {
                (true, false) => return Ordering::Less,
                (false, true) => return Ordering::Greater,
                _ => {}
            }

            let cmp = match col_ix {
                COL_NAME => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                COL_SIZE => a.size.cmp(&b.size),
                COL_MODIFIED => a.modified.cmp(&b.modified),
                _ => Ordering::Equal,
            };

            match sort {
                ColumnSort::Ascending => cmp,
                ColumnSort::Descending => cmp.reverse(),
                ColumnSort::Default => cmp,
            }
        });
    }

    /// 格式化 uid:gid 为 username:groupname
    fn format_owner(&self, uid: Option<u32>, gid: Option<u32>) -> String {
        let user = uid
            .and_then(|u| self.user_cache.get(&u).cloned())
            .unwrap_or_else(|| {
                uid.map(|u| u.to_string())
                    .unwrap_or_else(|| "-".to_string())
            });
        let group = gid
            .and_then(|g| self.group_cache.get(&g).cloned())
            .unwrap_or_else(|| {
                gid.map(|g| g.to_string())
                    .unwrap_or_else(|| "-".to_string())
            });
        format!("{}:{}", user, group)
    }

    /// 获取指定行的文件路径
    pub fn get_file_path(&self, row_ix: usize) -> Option<String> {
        self.file_list.get(row_ix).map(|e| e.path.clone())
    }
}

/// 获取文件类型图标
fn get_file_icon(entry: &FileEntry) -> &'static str {
    match entry.file_type {
        FileType::Directory => icons::FOLDER,
        FileType::Symlink => icons::LINK,
        FileType::File => match entry.extension() {
            Some("txt" | "md" | "log") => icons::FILE_TEXT,
            Some("rs" | "js" | "ts" | "py" | "go" | "c" | "cpp" | "h") => icons::FILE_CODE,
            Some("json" | "yaml" | "yml" | "toml" | "xml") => icons::FILE_JSON,
            Some("jpg" | "jpeg" | "png" | "gif" | "svg" | "webp") => icons::IMAGE,
            Some("zip" | "tar" | "gz" | "7z" | "rar") => icons::ARCHIVE,
            _ => icons::FILE,
        },
        FileType::Other => icons::FILE,
    }
}

/// 格式化修改时间
fn format_modified_time(entry: &FileEntry) -> String {
    match entry.modified {
        Some(time) => {
            let duration = time
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            let secs = duration.as_secs();
            let days = secs / 86400;
            let years = 1970 + (days / 365);
            let remaining_days = days % 365;
            let months = remaining_days / 30 + 1;
            let day = remaining_days % 30 + 1;
            let hours = (secs % 86400) / 3600;
            let mins = (secs % 3600) / 60;
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                years, months, day, hours, mins
            )
        }
        None => "-".to_string(),
    }
}

impl TableDelegate for FileListDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.file_list.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> &Column {
        &self.columns[col_ix]
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        self.sort_file_list(col_ix, sort);
        cx.notify();
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let name = self.columns[col_ix].name.clone();
        div()
            .size_full()
            .flex()
            .items_center()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(name)
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(entry) = self.file_list.get(row_ix) else {
            return div().into_any_element();
        };

        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;

        match col_ix {
            COL_NAME => {
                let icon = get_file_icon(entry);
                let icon_color = if entry.is_dir() {
                    cx.theme().link
                } else {
                    muted
                };

                div()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .overflow_hidden()
                    .child(svg().path(icon).size(px(ICON_SIZE)).text_color(icon_color))
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(foreground)
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(entry.name.clone()),
                    )
                    .into_any_element()
            }
            COL_PERMISSIONS => div()
                .h_full()
                .flex()
                .items_center()
                .text_xs()
                .text_color(muted)
                .font_family("monospace")
                .child(entry.format_permissions())
                .into_any_element(),
            COL_OWNER => {
                let owner = self.format_owner(entry.uid, entry.gid);
                div()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_xs()
                    .text_color(muted)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(owner)
                    .into_any_element()
            }
            COL_SIZE => div()
                .h_full()
                .flex()
                .items_center()
                .text_xs()
                .text_color(muted)
                .child(entry.format_size())
                .into_any_element(),
            COL_MODIFIED => div()
                .h_full()
                .flex()
                .items_center()
                .text_xs()
                .text_color(muted)
                .child(format_modified_time(entry))
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        div().id(("file-row", row_ix))
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let lang = &self.lang;
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t(lang, "sftp.empty_directory")),
            )
    }

    fn loading(&self, _cx: &App) -> bool {
        false
    }
}

/// 文件列表视图组件 - 管理 Table Entity
pub struct FileListView {
    /// Table 状态
    table_state: Entity<TableState<FileListDelegate>>,
    /// 是否正在加载
    loading: bool,
    /// 是否已连接
    connected: bool,
    /// 语言设置
    lang: Language,
}

impl FileListView {
    /// 创建新的文件列表视图
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let lang = crate::services::storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or_default();

        let delegate = FileListDelegate::new(lang);

        let table_state = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .col_movable(false)
                .sortable(true)
                .col_resizable(true)
                .row_selectable(true)
        });

        // 订阅 TableState 的事件并转发
        cx.subscribe_in(&table_state, window, |_this, _state, event: &TableEvent, _window, cx| {
            cx.emit(event.clone());
        })
        .detach();

        Self {
            table_state,
            loading: false,
            connected: false,
            lang: crate::services::storage::load_settings()
                .map(|s| s.theme.language)
                .unwrap_or_default(),
        }
    }

    /// 从 SftpState 同步数据
    pub fn sync_from_sftp_state(&mut self, sftp_state: Option<&SftpState>, cx: &mut Context<Self>) {
        match sftp_state {
            Some(state) => {
                self.connected = true;
                self.loading = state.loading;

                if !state.loading {
                    self.table_state.update(cx, |table_state, cx| {
                        let delegate = table_state.delegate_mut();
                        // 只更新数据，不调用 refresh 以保留排序状态
                        delegate.update_file_list(state.file_list.clone());
                        delegate.update_user_cache(state.user_cache.clone());
                        delegate.update_group_cache(state.group_cache.clone());
                        // 使用 notify 代替 refresh，避免重置列配置和排序状态
                        cx.notify();
                    });
                }
            }
            None => {
                self.connected = false;
                self.loading = false;
            }
        }
        cx.notify();
    }

    /// 获取指定行的文件路径
    pub fn get_file_path(&self, row_ix: usize, cx: &App) -> Option<String> {
        self.table_state.read(cx).delegate().get_file_path(row_ix)
    }
}

impl EventEmitter<TableEvent> for FileListView {}

impl Render for FileListView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg_color = crate::theme::sidebar_color(cx);
        let muted_foreground = cx.theme().muted_foreground;

        if !self.connected {
            // 未连接状态
            return div()
                .size_full()
                .bg(bg_color)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(muted_foreground)
                        .child(t(&self.lang, "sftp.not_connected")),
                )
                .into_any_element();
        }

        if self.loading {
            // 加载中状态
            return div()
                .size_full()
                .bg(bg_color)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(muted_foreground)
                        .child(t(&self.lang, "sftp.loading")),
                )
                .into_any_element();
        }

        // 正常状态 - 显示 Table
        Table::new(&self.table_state)
            .stripe(true)
            .bordered(false)
            .into_any_element()
    }
}

// ============================================================================
// 兼容旧版 API（用于过渡期）
// ============================================================================

/// 渲染文件列表（兼容旧版 API）
/// 注意：此函数将被废弃，请使用 FileListView 组件
#[allow(dead_code)]
pub fn render_file_list<F>(state: Option<&SftpState>, _on_event: F, cx: &App) -> impl IntoElement
where
    F: Fn(FileListEvent, &mut App) + Clone + 'static,
{
    let bg_color = crate::theme::sidebar_color(cx);
    let muted_foreground = cx.theme().muted_foreground;
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    match state {
        Some(s) if !s.loading => {
            if s.file_list.is_empty() {
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
                            .child(t(&lang, "sftp.empty_directory")),
                    )
                    .into_any_element()
            } else {
                // 临时：返回提示信息，需要在 sftp_panel 中使用 FileListView
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
                            .child("请使用 FileListView 组件"),
                    )
                    .into_any_element()
            }
        }
        Some(_) => div()
            .size_full()
            .bg(bg_color)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(muted_foreground)
                    .child(t(&lang, "sftp.loading")),
            )
            .into_any_element(),
        None => div()
            .size_full()
            .bg(bg_color)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(muted_foreground)
                    .child(t(&lang, "sftp.not_connected")),
            )
            .into_any_element(),
    }
}
