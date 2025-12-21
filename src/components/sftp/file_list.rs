// SFTP 文件列表组件 - 使用 gpui_component::Table 实现
// 支持列排序和手动调整列宽

use std::cmp::Reverse;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

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

/// 文件列表 Delegate - 实现 TableDelegate trait
pub struct FileListDelegate {
    /// 文件列表数据
    pub file_list: Vec<FileEntry>,
    /// 当前显示行顺序（索引）
    row_order: Vec<usize>,
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
        let mut delegate = Self {
            file_list: Vec::new(),
            row_order: Vec::new(),
            columns,
            user_cache: HashMap::new(),
            group_cache: HashMap::new(),
            lang,
            current_sort_col: COL_NAME,
            current_sort: ColumnSort::Ascending,
        };
        delegate.sync_column_sort_state();
        delegate
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

    /// 更新语言与列标题（保留列宽与排序状态）
    pub fn update_language(&mut self, lang: &Language, column_widths: Option<&[Pixels]>) {
        self.lang = lang.clone();
        self.update_column_titles(lang);
        if let Some(widths) = column_widths {
            if widths.len() == self.columns.len() {
                for (col, width) in self.columns.iter_mut().zip(widths.iter()) {
                    col.width = *width;
                }
            }
        }
        self.sync_column_sort_state();
    }

    /// 更新列标题文本
    fn update_column_titles(&mut self, lang: &Language) {
        if let Some(column) = self.columns.get_mut(COL_NAME) {
            column.name = t(lang, "sftp.header.name").into();
        }
        if let Some(column) = self.columns.get_mut(COL_PERMISSIONS) {
            column.name = t(lang, "sftp.header.permissions").into();
        }
        if let Some(column) = self.columns.get_mut(COL_OWNER) {
            column.name = t(lang, "sftp.header.owner").into();
        }
        if let Some(column) = self.columns.get_mut(COL_SIZE) {
            column.name = t(lang, "sftp.header.size").into();
        }
        if let Some(column) = self.columns.get_mut(COL_MODIFIED) {
            column.name = t(lang, "sftp.header.modified").into();
        }
    }

    /// 同步列排序状态（用于保留 UI 选中状态）
    fn sync_column_sort_state(&mut self) {
        for (ix, column) in self.columns.iter_mut().enumerate() {
            if column.sort.is_some() {
                column.sort = Some(ColumnSort::Default);
            }
            if ix == self.current_sort_col && column.sort.is_some() {
                column.sort = Some(self.current_sort);
            }
        }
    }

    /// 重置行顺序为原始顺序
    fn reset_row_order(&mut self) {
        self.row_order.clear();
        self.row_order.extend(0..self.file_list.len());
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
        self.sync_column_sort_state();
        // 应用排序
        self.apply_current_sort();
    }

    /// 应用当前排序状态
    fn apply_current_sort(&mut self) {
        if self.current_sort == ColumnSort::Default {
            self.reset_row_order();
            return;
        }

        if self.row_order.len() != self.file_list.len() {
            self.reset_row_order();
        }

        let entries = &self.file_list;
        match (self.current_sort_col, self.current_sort) {
            (COL_NAME, ColumnSort::Ascending) => {
                self.row_order.sort_by_cached_key(|&ix| {
                    let entry = &entries[ix];
                    let dir_rank = if entry.is_dir() { 0u8 } else { 1u8 };
                    (dir_rank, entry.name.to_lowercase(), ix)
                });
            }
            (COL_NAME, ColumnSort::Descending) => {
                self.row_order.sort_by_cached_key(|&ix| {
                    let entry = &entries[ix];
                    let dir_rank = if entry.is_dir() { 0u8 } else { 1u8 };
                    (dir_rank, Reverse(entry.name.to_lowercase()), ix)
                });
            }
            (COL_SIZE, ColumnSort::Ascending) => {
                self.row_order.sort_by_cached_key(|&ix| {
                    let entry = &entries[ix];
                    let dir_rank = if entry.is_dir() { 0u8 } else { 1u8 };
                    (dir_rank, entry.size, ix)
                });
            }
            (COL_SIZE, ColumnSort::Descending) => {
                self.row_order.sort_by_cached_key(|&ix| {
                    let entry = &entries[ix];
                    let dir_rank = if entry.is_dir() { 0u8 } else { 1u8 };
                    (dir_rank, Reverse(entry.size), ix)
                });
            }
            (COL_MODIFIED, ColumnSort::Ascending) => {
                self.row_order.sort_by_cached_key(|&ix| {
                    let entry = &entries[ix];
                    let dir_rank = if entry.is_dir() { 0u8 } else { 1u8 };
                    (dir_rank, entry.modified.clone(), ix)
                });
            }
            (COL_MODIFIED, ColumnSort::Descending) => {
                self.row_order.sort_by_cached_key(|&ix| {
                    let entry = &entries[ix];
                    let dir_rank = if entry.is_dir() { 0u8 } else { 1u8 };
                    (dir_rank, Reverse(entry.modified.clone()), ix)
                });
            }
            _ => {}
        }
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
        self.row_order
            .get(row_ix)
            .and_then(|&ix| self.file_list.get(ix))
            .map(|e| e.path.clone())
    }
}

fn hash_row_id(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
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
        self.row_order.len()
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
        let Some(entry_ix) = self.row_order.get(row_ix).copied() else {
            return div().into_any_element();
        };
        let Some(entry) = self.file_list.get(entry_ix) else {
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
        let row_id = self
            .row_order
            .get(row_ix)
            .and_then(|&ix| self.file_list.get(ix))
            .map(|entry| hash_row_id(&entry.path))
            .unwrap_or(row_ix as u64);
        div().id(("file-row", row_id))
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
    /// 记录列宽（用于语言切换时保留）
    column_widths: Vec<Pixels>,
    /// 文件列表版本
    last_file_list_revision: u64,
    /// 用户缓存版本
    last_user_cache_revision: u64,
    /// 组缓存版本
    last_group_cache_revision: u64,
}

impl FileListView {
    /// 创建新的文件列表视图
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let lang = crate::services::storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or_default();

        let delegate = FileListDelegate::new(lang.clone());

        let table_state = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .col_movable(false)
                .sortable(true)
                .col_resizable(true)
                .row_selectable(true)
        });

        // 订阅 TableState 的事件并转发
        cx.subscribe_in(&table_state, window, |this, _state, event: &TableEvent, _window, cx| {
            if let TableEvent::ColumnWidthsChanged(widths) = event {
                this.column_widths = widths.clone();
            }
            cx.emit(event.clone());
        })
        .detach();

        Self {
            table_state,
            loading: false,
            connected: false,
            lang,
            column_widths: Vec::new(),
            last_file_list_revision: 0,
            last_user_cache_revision: 0,
            last_group_cache_revision: 0,
        }
    }

    /// 从 SftpState 同步数据
    pub fn sync_from_sftp_state(&mut self, sftp_state: Option<&SftpState>, cx: &mut Context<Self>) {
        let mut needs_notify = false;

        let lang = crate::services::storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or_default();
        if lang != self.lang {
            self.lang = lang.clone();
            let widths = if self.column_widths.is_empty() {
                None
            } else {
                Some(self.column_widths.as_slice())
            };
            self.table_state.update(cx, |table_state, cx| {
                let delegate = table_state.delegate_mut();
                delegate.update_language(&lang, widths);
                table_state.refresh(cx);
            });
            needs_notify = true;
        }

        match sftp_state {
            Some(state) => {
                if !self.connected {
                    self.connected = true;
                    needs_notify = true;
                }
                if self.loading != state.loading {
                    self.loading = state.loading;
                    needs_notify = true;
                }

                if !state.loading {
                    let file_list_changed =
                        state.file_list_revision != self.last_file_list_revision;
                    let user_cache_changed =
                        state.user_cache_revision != self.last_user_cache_revision;
                    let group_cache_changed =
                        state.group_cache_revision != self.last_group_cache_revision;

                    if file_list_changed || user_cache_changed || group_cache_changed {
                        self.last_file_list_revision = state.file_list_revision;
                        self.last_user_cache_revision = state.user_cache_revision;
                        self.last_group_cache_revision = state.group_cache_revision;

                        self.table_state.update(cx, |table_state, cx| {
                            let delegate = table_state.delegate_mut();
                            if file_list_changed {
                                delegate.update_file_list(state.file_list.clone());
                            }
                            if user_cache_changed {
                                delegate.update_user_cache(state.user_cache.clone());
                            }
                            if group_cache_changed {
                                delegate.update_group_cache(state.group_cache.clone());
                            }
                            // 使用 notify 代替 refresh，避免重置列配置和排序状态
                            cx.notify();
                        });
                        needs_notify = true;
                    }
                }
            }
            None => {
                if self.connected || self.loading {
                    self.connected = false;
                    self.loading = false;
                    needs_notify = true;
                }
                self.last_file_list_revision = 0;
                self.last_user_cache_revision = 0;
                self.last_group_cache_revision = 0;
            }
        }
        if needs_notify {
            cx.notify();
        }
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
