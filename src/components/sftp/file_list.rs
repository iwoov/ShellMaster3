// SFTP 文件列表组件 - 使用 gpui_component::Table 实现
// 支持列排序和手动调整列宽

use std::cmp::Reverse;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_component::table::{Column, ColumnSort, Table, TableDelegate, TableEvent, TableState};
use gpui_component::{ActiveTheme, Sizable};

use crate::components::common::button::text_button;
use crate::components::common::button::ButtonKind;
use crate::components::common::format::format_bytes;
use crate::components::common::panel::{panel_state, spinner, StateIcon};
use crate::components::common::text::TextFitter;
use crate::constants::icons;
use crate::i18n::t;
use crate::models::settings::Language;
use crate::models::sftp::{FileEntry, FileType, SftpState};
use crate::theme::Palette;

/// SFTP 文件列表右键菜单事件
#[derive(Clone, Debug)]
pub enum FileListContextMenuEvent {
    // 文件操作
    Download(String),   // 文件路径
    EditFile(String),   // 文件路径
    CopyName(String),   // 文件名
    CopyPath(String),   // 完整路径
    Rename(String),     // 文件路径 - 开始重命名
    Delete(String),     // 文件路径
    Properties(String), // 文件路径

    // 文件夹操作
    OpenFolder(String),     // 文件夹路径
    DownloadFolder(String), // 文件夹路径
    OpenInTerminal(String), // 目录路径

    // 空白区域操作
    Refresh,
    NewFolder,
    NewFile,
    UploadFile,
    UploadFolder,
    SelectAll,

    // 重命名确认/取消
    RenameConfirmed {
        old_path: String,
        new_name: String,
    },
    RenameCancelled,

    // 拖放上传
    DropFiles {
        paths: Vec<std::path::PathBuf>, // 本地文件/文件夹路径
        target_dir: String,             // 目标远程目录
    },
}

/// 图标尺寸
const ICON_SIZE: f32 = 16.0;

/// 列索引常量：名称、大小、修改时间在前，权限和所有者放在最后（窄窗口里先被挤出视野）
const COL_NAME: usize = 0;
const COL_SIZE: usize = 1;
const COL_MODIFIED: usize = 2;
const COL_PERMISSIONS: usize = 3;
const COL_OWNER: usize = 4;
/// 名称列的初始宽度
const NAME_COLUMN_WIDTH: Pixels = px(300.);
/// 单元格左右内边距（`Table::small()`）
const CELL_PADDING_X: f32 = 6.;
/// 名称前图标与文字的间距
const NAME_GAP: f32 = 8.;

/// 行拖放回调类型
pub type RowDropCallback =
    std::sync::Arc<dyn Fn(Vec<std::path::PathBuf>, String) + Send + Sync + 'static>;

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
    /// 正在编辑的文件路径（用于内联重命名）
    pub editing_path: Option<String>,
    /// 重命名输入框状态（用于内联编辑）
    pub rename_input: Option<Entity<gpui_component::input::InputState>>,
    /// 行拖放回调（当文件拖放到文件夹行上时调用）
    pub on_row_drop: Option<RowDropCallback>,
    /// 名称列当前宽度（拖动列宽后由视图更新），用于按实际宽度截断文件名
    pub name_column_width: Pixels,
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
            editing_path: None,
            rename_input: None,
            on_row_drop: None,
            name_column_width: NAME_COLUMN_WIDTH,
        };
        delegate.sync_column_sort_state();
        delegate
    }

    /// 创建列定义
    fn create_columns(lang: &Language) -> Vec<Column> {
        vec![
            Column::new("name", t(lang, "sftp.header.name"))
                .width(NAME_COLUMN_WIDTH)
                .sortable(),
            Column::new("size", t(lang, "sftp.header.size"))
                .width(px(90.))
                .sortable(),
            Column::new("modified", t(lang, "sftp.header.modified"))
                .width(px(140.))
                .sortable(),
            Column::new("permissions", t(lang, "sftp.header.permissions"))
                .width(px(110.))
                .resizable(true),
            Column::new("owner", t(lang, "sftp.header.owner"))
                .width(px(120.))
                .resizable(true),
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

    /// 获取文件列表是否为空
    pub fn is_empty(&self) -> bool {
        self.file_list.is_empty()
    }
}

fn hash_row_id(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// 获取文件类型图标（扩展名不区分大小写）
fn get_file_icon(entry: &FileEntry) -> &'static str {
    match entry.file_type {
        FileType::Directory => icons::FOLDER,
        FileType::Symlink => icons::LINK,
        FileType::File => {
            let extension = entry.extension().map(str::to_ascii_lowercase);
            match extension.as_deref() {
                Some("txt" | "md" | "log") => icons::FILE_TEXT,
                Some("rs" | "js" | "ts" | "py" | "go" | "c" | "cpp" | "h" | "sh") => {
                    icons::FILE_CODE
                }
                Some("json" | "yaml" | "yml" | "toml" | "xml") => icons::FILE_JSON,
                Some("jpg" | "jpeg" | "png" | "gif" | "svg" | "webp") => icons::IMAGE,
                Some("zip" | "tar" | "gz" | "tgz" | "7z" | "rar" | "xz" | "bz2") => icons::ARCHIVE,
                _ => icons::FILE,
            }
        }
        FileType::Other => icons::FILE,
    }
}

/// 文件图标的颜色：文件夹用主色，符号链接稍亮，其余文件用弱色
fn file_icon_color(entry: &FileEntry, p: &Palette) -> Hsla {
    match entry.file_type {
        FileType::Directory => p.primary,
        FileType::Symlink => p.text_muted,
        _ => p.text_faint,
    }
}

/// 格式化修改时间
fn format_modified_time(entry: &FileEntry) -> String {
    use chrono::{Local, TimeZone};

    match entry.modified {
        Some(time) => {
            let duration = time
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            let secs = duration.as_secs() as i64;
            // 使用 chrono 进行正确的日期时间转换，并转为本地时区
            match Local.timestamp_opt(secs, 0) {
                chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
                _ => "—".to_string(),
            }
        }
        None => "—".to_string(),
    }
}

/// 单元格的外框：占满高度、竖直居中
fn cell() -> Div {
    div()
        .h_full()
        .w_full()
        .flex()
        .items_center()
        .overflow_hidden()
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
        let p = crate::theme::palette(cx);
        let name = self.columns[col_ix].name.clone();
        cell()
            .when(col_ix == COL_SIZE, |cell| cell.justify_end())
            .whitespace_nowrap()
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.text_muted)
            .child(name)
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(entry_ix) = self.row_order.get(row_ix).copied() else {
            return div().into_any_element();
        };
        let Some(entry) = self.file_list.get(entry_ix) else {
            return div().into_any_element();
        };

        let p = crate::theme::palette(cx);
        let mono = cx.theme().mono_font_family.clone();

        match col_ix {
            COL_NAME => {
                let icon = get_file_icon(entry);
                let icon_color = file_icon_color(entry, &p);

                // 正在内联重命名：输入框代替文件名
                if self.editing_path.as_ref() == Some(&entry.path) {
                    if let Some(input_state) = &self.rename_input {
                        let field = crate::components::common::input::text_field(input_state);
                        return cell()
                            .gap(px(NAME_GAP))
                            .child(
                                svg()
                                    .path(icon)
                                    .size(px(ICON_SIZE))
                                    .flex_none()
                                    .text_color(icon_color),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(gpui::Styled::h(field, px(24.)).rounded(px(6.))),
                            )
                            .into_any_element();
                    }
                }

                let name_width = (self.name_column_width
                    - px(CELL_PADDING_X * 2. + ICON_SIZE + NAME_GAP))
                .max(px(24.));
                let fit = TextFitter::new(window, cx);
                cell()
                    .gap(px(NAME_GAP))
                    .child(
                        svg()
                            .path(icon)
                            .size(px(ICON_SIZE))
                            .flex_none()
                            .text_color(icon_color),
                    )
                    .child(
                        fit.label_with_tooltip(
                            SharedString::from(format!("sftp-name-{}", entry.path)),
                            &entry.name,
                            name_width,
                            13.,
                            FontWeight::NORMAL,
                        )
                        .flex_none()
                        .text_color(p.text),
                    )
                    .into_any_element()
            }
            COL_SIZE => cell()
                .justify_end()
                .whitespace_nowrap()
                .font_family(mono)
                .text_size(px(12.))
                .text_color(if entry.is_dir() {
                    p.text_faint
                } else {
                    p.text_muted
                })
                .child(if entry.is_dir() {
                    "—".to_string()
                } else {
                    format_bytes(entry.size)
                })
                .into_any_element(),
            COL_MODIFIED => cell()
                .whitespace_nowrap()
                .text_size(px(12.))
                .text_color(p.text_muted)
                .child(format_modified_time(entry))
                .into_any_element(),
            COL_PERMISSIONS => cell()
                .whitespace_nowrap()
                .font_family(mono)
                .text_size(px(12.))
                .text_color(p.text_faint)
                .child(entry.format_permissions())
                .into_any_element(),
            COL_OWNER => cell()
                .whitespace_nowrap()
                .text_size(px(12.))
                .text_color(p.text_faint)
                .child(self.format_owner(entry.uid, entry.gid))
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
        // 获取当前行的文件信息
        let entry = self
            .row_order
            .get(row_ix)
            .and_then(|&ix| self.file_list.get(ix));

        let row_id = entry.map(|e| hash_row_id(&e.path)).unwrap_or(row_ix as u64);

        let is_dir = entry.map(|e| e.is_dir()).unwrap_or(false);
        let folder_path = entry.map(|e| e.path.clone());
        let on_row_drop = self.on_row_drop.clone();

        let base_div = div().id(("file-row", row_id));

        // 如果是文件夹，添加拖放处理器
        if is_dir {
            base_div
                .drag_over::<ExternalPaths>(|this, _, _, cx| {
                    // 文件夹行高亮：主色淡底
                    this.bg(crate::theme::palette(cx).primary.opacity(0.14))
                })
                .on_drop(move |external_paths: &ExternalPaths, _window, _cx| {
                    if let Some(ref callback) = on_row_drop {
                        if let Some(ref target_folder) = folder_path {
                            let paths: Vec<std::path::PathBuf> =
                                external_paths.paths().iter().cloned().collect();
                            if !paths.is_empty() {
                                callback(paths, target_folder.clone());
                            }
                        }
                    }
                })
        } else {
            base_div
        }
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let p = crate::theme::palette(cx);
        panel_state(
            "sftp-empty-folder",
            StateIcon::Icon(icons::FOLDER_OPEN),
            p.text_faint,
            t(&self.lang, "sftp.empty_folder"),
            None,
            &p,
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
    /// 读取当前目录失败时的错误信息
    error: Option<String>,
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
    /// 内联重命名输入框
    rename_input: Option<Entity<gpui_component::input::InputState>>,
    /// 正在编辑的文件路径
    editing_path: Option<String>,
    /// 当前远程路径（用于拖放上传）
    current_path: String,
    /// 待处理的行拖放事件队列
    pending_row_drops: std::sync::Arc<std::sync::Mutex<Vec<(Vec<std::path::PathBuf>, String)>>>,
}

impl FileListView {
    /// 创建新的文件列表视图
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let lang = crate::services::storage::cached_settings()
            .theme
            .language
            .clone();

        // 创建共享事件队列
        let pending_row_drops: std::sync::Arc<
            std::sync::Mutex<Vec<(Vec<std::path::PathBuf>, String)>>,
        > = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let pending_row_drops_for_callback = pending_row_drops.clone();

        // 创建 delegate 并设置回调
        let mut delegate = FileListDelegate::new(lang.clone());
        delegate.on_row_drop = Some(std::sync::Arc::new(move |paths, target_dir| {
            if let Ok(mut queue) = pending_row_drops_for_callback.lock() {
                queue.push((paths, target_dir));
            }
        }));

        let table_state = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .col_movable(false)
                .sortable(true)
                .col_resizable(true)
                .row_selectable(true)
        });

        // 订阅 TableState 的事件并转发
        cx.subscribe_in(
            &table_state,
            window,
            |this, _state, event: &TableEvent, _window, cx| {
                if let TableEvent::ColumnWidthsChanged(widths) = event {
                    this.column_widths = widths.clone();
                    // 名称按实际列宽截断
                    if let Some(width) = widths.get(COL_NAME).copied() {
                        this.table_state.update(cx, |table, _| {
                            table.delegate_mut().name_column_width = width;
                        });
                    }
                }
                cx.emit(event.clone());
            },
        )
        .detach();

        Self {
            table_state,
            loading: false,
            connected: false,
            error: None,
            lang,
            column_widths: Vec::new(),
            last_file_list_revision: 0,
            last_user_cache_revision: 0,
            last_group_cache_revision: 0,
            rename_input: None,
            editing_path: None,
            current_path: String::new(),
            pending_row_drops,
        }
    }

    /// 从 SftpState 同步数据
    pub fn sync_from_sftp_state(&mut self, sftp_state: Option<&SftpState>, cx: &mut Context<Self>) {
        let mut needs_notify = false;

        let lang = crate::services::storage::cached_settings()
            .theme
            .language
            .clone();
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
                // 同步当前路径
                if self.current_path != state.current_path {
                    self.current_path = state.current_path.clone();
                }
                if self.loading != state.loading {
                    self.loading = state.loading;
                    needs_notify = true;
                }
                if self.error != state.error {
                    self.error = state.error.clone();
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
                if self.connected || self.loading || self.error.is_some() {
                    self.connected = false;
                    self.loading = false;
                    self.error = None;
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

    /// 获取当前选中的文件条目
    pub fn get_selected_file(&self, cx: &App) -> Option<FileEntry> {
        let table_state = self.table_state.read(cx);
        let selected_row = table_state.selected_row()?;
        let delegate = table_state.delegate();
        let entry_ix = delegate.row_order.get(selected_row).copied()?;
        delegate.file_list.get(entry_ix).cloned()
    }

    /// 开始内联重命名
    pub fn start_rename(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        // 获取文件名
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();

        // 创建输入框
        let input = cx.new(|cx| {
            let mut state = gpui_component::input::InputState::new(window, cx);
            state.set_value(name, window, cx);
            state
        });

        // 订阅输入框事件
        use gpui_component::input::InputEvent;
        cx.subscribe(&input, |this, _input, event: &InputEvent, cx| {
            match event {
                InputEvent::PressEnter { .. } => {
                    // 按下回车，确认重命名
                    this.confirm_rename(cx);
                }
                InputEvent::Blur => {
                    // 失焦时取消编辑
                    this.cancel_rename(cx);
                }
                _ => {}
            }
        })
        .detach();

        // 聚焦输入框
        input.update(cx, |input, cx| {
            input.focus(window, cx);
        });

        self.rename_input = Some(input.clone());
        self.editing_path = Some(path.clone());

        // 更新 delegate 的编辑状态和输入框
        self.table_state.update(cx, |state, _| {
            let delegate = state.delegate_mut();
            delegate.editing_path = Some(path);
            delegate.rename_input = Some(input);
        });

        cx.notify();
    }

    /// 确认重命名
    pub fn confirm_rename(&mut self, cx: &mut Context<Self>) {
        if let (Some(input), Some(old_path)) = (&self.rename_input, &self.editing_path) {
            let new_name = input.read(cx).value().to_string().trim().to_string();
            if !new_name.is_empty() {
                let old_path = old_path.clone();
                cx.emit(FileListContextMenuEvent::RenameConfirmed { old_path, new_name });
            }
        }
        self.cancel_rename(cx);
    }

    /// 取消重命名
    pub fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.rename_input = None;
        self.editing_path = None;

        // 清除 delegate 的编辑状态
        self.table_state.update(cx, |state, _| {
            let delegate = state.delegate_mut();
            delegate.editing_path = None;
            delegate.rename_input = None;
        });

        cx.emit(FileListContextMenuEvent::RenameCancelled);
        cx.notify();
    }
}

impl EventEmitter<TableEvent> for FileListView {}
impl EventEmitter<FileListContextMenuEvent> for FileListView {}

impl Render for FileListView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 处理待处理的行拖放事件
        if let Ok(mut queue) = self.pending_row_drops.lock() {
            for (paths, target_dir) in queue.drain(..) {
                cx.emit(FileListContextMenuEvent::DropFiles { paths, target_dir });
            }
        }

        let p = crate::theme::palette(cx);
        let lang = self.lang.clone();
        let has_items = !self.table_state.read(cx).delegate().is_empty();

        // 未连接 / 首次加载 / 读取失败：整块显示状态
        if !self.connected {
            return div()
                .size_full()
                .bg(p.background)
                .child(panel_state(
                    "sftp-list-not-connected",
                    StateIcon::Spinner,
                    p.text_faint,
                    t(&lang, "sftp.connecting"),
                    None,
                    &p,
                ))
                .into_any_element();
        }
        if let Some(error) = self.error.clone().filter(|_| !has_items && !self.loading) {
            let this = cx.entity().clone();
            return div()
                .size_full()
                .bg(p.background)
                .child(
                    panel_state(
                        "sftp-list-error",
                        StateIcon::Icon(icons::X),
                        p.danger,
                        t(&lang, "sftp.load_failed"),
                        Some(error.into()),
                        &p,
                    )
                    .child(
                        div().mt(px(8.)).child(
                            text_button(
                                "sftp-list-retry",
                                ButtonKind::Secondary,
                                t(&lang, "sftp.retry"),
                                &p,
                            )
                            .on_click(move |_, _, cx| {
                                this.update(cx, |_, cx| {
                                    cx.emit(FileListContextMenuEvent::Refresh);
                                });
                            }),
                        ),
                    ),
                )
                .into_any_element();
        }
        if self.loading && !has_items {
            return div()
                .size_full()
                .bg(p.background)
                .child(panel_state(
                    "sftp-list-loading",
                    StateIcon::Spinner,
                    p.text_faint,
                    t(&lang, "sftp.loading"),
                    None,
                    &p,
                ))
                .into_any_element();
        }

        // 获取自身 Entity 用于在上下文菜单中发出事件
        let this = cx.entity().clone();
        let drop_border = p.primary;

        // 正常状态：表格。加载其他目录时保留旧内容并变淡，路径框里有加载图标，不再整块闪掉
        let table = Table::new(&self.table_state)
            .small()
            .stripe(false)
            .bordered(false);

        div()
            .id("sftp-file-list-container")
            .size_full()
            .relative()
            .bg(p.background)
            // 常态就有 1px 透明边框，拖入时只变色，不挤动内容
            .border_1()
            .border_color(gpui::transparent_black())
            .child(table)
            .when(self.loading, |list| {
                list.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(p.background.opacity(0.45))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(spinner("sftp-list-reloading", 18., p.text_faint)),
                )
            })
            // 拖放上传支持
            .drag_over::<ExternalPaths>(move |this, _, _, cx| {
                this.bg(cx.theme().drop_target).border_color(drop_border)
            })
            .on_drop(
                cx.listener(move |view, external_paths: &ExternalPaths, _window, cx| {
                    // 获取拖放的文件路径
                    let paths: Vec<std::path::PathBuf> =
                        external_paths.paths().iter().cloned().collect();
                    if !paths.is_empty() {
                        // 拖放到文件列表容器时，始终上传到当前目录
                        // 如果要上传到子目录，用户需要直接拖放到文件夹行上（会有高亮反馈）
                        let target_dir = view.current_path.clone();
                        cx.emit(FileListContextMenuEvent::DropFiles { paths, target_dir });
                    }
                }),
            )
            .context_menu(move |menu, _window, cx| {
                // 读取当前选中的文件条目
                let selected_entry = this.read(cx).get_selected_file(cx);

                // 根据选中的项目类型构建不同的菜单
                // 注意：这里使用"选中"而不是"悬停"，类似于 Windows 资源管理器的行为
                match &selected_entry {
                    Some(entry) if entry.is_dir() => {
                        build_folder_context_menu(menu, entry, &lang, this.clone())
                    }
                    Some(entry) => build_file_context_menu(menu, entry, &lang, this.clone()),
                    None => build_empty_area_context_menu(menu, &lang, this.clone()),
                }
            })
            .into_any_element()
    }
}

/// 构建文件右键菜单
fn build_file_context_menu(
    menu: gpui_component::menu::PopupMenu,
    entry: &FileEntry,
    lang: &Language,
    entity: Entity<FileListView>,
) -> gpui_component::menu::PopupMenu {
    let path = entry.path.clone();
    let name = entry.name.clone();
    let path_for_download = path.clone();
    let path_for_edit = path.clone();
    let name_for_copy = name.clone();
    let path_for_copy = path.clone();
    let path_for_rename = path.clone();
    let path_for_delete = path.clone();
    let path_for_terminal = std::path::Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string());
    let path_for_properties = path.clone();

    let download_label = t(lang, "sftp.context_menu.download").to_string();
    let edit_label = t(lang, "sftp.context_menu.edit_file").to_string();
    let copy_name_label = t(lang, "sftp.context_menu.copy_name").to_string();
    let copy_path_label = t(lang, "sftp.context_menu.copy_path").to_string();
    let rename_label = t(lang, "sftp.context_menu.rename").to_string();
    let delete_label = t(lang, "sftp.context_menu.delete").to_string();
    let terminal_label = t(lang, "sftp.context_menu.open_in_terminal").to_string();
    let properties_label = t(lang, "sftp.context_menu.properties").to_string();

    let e1 = entity.clone();
    let e2 = entity.clone();
    let e3 = entity.clone();
    let e4 = entity.clone();
    let e5 = entity.clone();
    let e6 = entity.clone();
    let e_copy_name = entity.clone();
    let e_copy_path = entity.clone();

    menu.item(
        menu_item_element(icons::DOWNLOAD, &download_label).on_click(move |_, _, cx| {
            e1.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::Download(
                    path_for_download.clone(),
                ));
            });
        }),
    )
    .item({
        let path = path_for_edit.clone();
        menu_item_element(icons::FILE_CODE, &edit_label).on_click(move |_, _, cx| {
            e2.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::EditFile(path.clone()));
            });
        })
    })
    .separator()
    .item({
        let name = name_for_copy.clone();
        menu_item_element(icons::COPY, &copy_name_label).on_click(move |_, _, cx| {
            e_copy_name.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::CopyName(name.clone()));
            });
        })
    })
    .item({
        let path = path_for_copy.clone();
        menu_item_element(icons::COPY, &copy_path_label).on_click(move |_, _, cx| {
            e_copy_path.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::CopyPath(path.clone()));
            });
        })
    })
    .separator()
    .item({
        let path = path_for_rename.clone();
        menu_item_element(icons::EDIT, &rename_label).on_click(move |_, _, cx| {
            e3.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::Rename(path.clone()));
            });
        })
    })
    .item({
        let path = path_for_delete.clone();
        menu_item_danger(icons::TRASH, &delete_label).on_click(move |_, _, cx| {
            e4.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::Delete(path.clone()));
            });
        })
    })
    .separator()
    .item({
        let path = path_for_terminal.clone();
        menu_item_element(icons::TERMINAL, &terminal_label).on_click(move |_, _, cx| {
            e5.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::OpenInTerminal(path.clone()));
            });
        })
    })
    .item({
        let path = path_for_properties.clone();
        menu_item_element(icons::INFO, &properties_label).on_click(move |_, _, cx| {
            e6.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::Properties(path.clone()));
            });
        })
    })
}

/// 构建文件夹右键菜单
fn build_folder_context_menu(
    menu: gpui_component::menu::PopupMenu,
    entry: &FileEntry,
    lang: &Language,
    entity: Entity<FileListView>,
) -> gpui_component::menu::PopupMenu {
    let path = entry.path.clone();
    let name = entry.name.clone();
    let path_for_open = path.clone();
    let path_for_download = path.clone();
    let name_for_copy = name.clone();
    let path_for_copy = path.clone();
    let path_for_rename = path.clone();
    let path_for_delete = path.clone();
    let path_for_terminal = path.clone();
    let path_for_properties = path.clone();

    let open_label = t(lang, "sftp.context_menu.open_folder").to_string();
    let download_label = t(lang, "sftp.context_menu.download_folder").to_string();
    let copy_name_label = t(lang, "sftp.context_menu.copy_name").to_string();
    let copy_path_label = t(lang, "sftp.context_menu.copy_path").to_string();
    let rename_label = t(lang, "sftp.context_menu.rename").to_string();
    let delete_label = t(lang, "sftp.context_menu.delete").to_string();
    let terminal_label = t(lang, "sftp.context_menu.open_in_terminal").to_string();
    let properties_label = t(lang, "sftp.context_menu.properties").to_string();

    let e1 = entity.clone();
    let e2 = entity.clone();
    let e3 = entity.clone();
    let e4 = entity.clone();
    let e5 = entity.clone();
    let e6 = entity.clone();

    menu.item({
        let path = path_for_open.clone();
        menu_item_element(icons::FOLDER_OPEN, &open_label).on_click(move |_, _, cx| {
            e1.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::OpenFolder(path.clone()));
            });
        })
    })
    .item({
        let path = path_for_download.clone();
        menu_item_element(icons::DOWNLOAD, &download_label).on_click(move |_, _, cx| {
            e2.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::DownloadFolder(path.clone()));
            });
        })
    })
    .separator()
    .item(
        menu_item_element(icons::COPY, &copy_name_label).on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(name_for_copy.clone()));
        }),
    )
    .item(
        menu_item_element(icons::COPY, &copy_path_label).on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(path_for_copy.clone()));
        }),
    )
    .separator()
    .item({
        let path = path_for_rename.clone();
        menu_item_element(icons::EDIT, &rename_label).on_click(move |_, _, cx| {
            e3.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::Rename(path.clone()));
            });
        })
    })
    .item({
        let path = path_for_delete.clone();
        menu_item_danger(icons::TRASH, &delete_label).on_click(move |_, _, cx| {
            e4.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::Delete(path.clone()));
            });
        })
    })
    .separator()
    .item({
        let path = path_for_terminal.clone();
        menu_item_element(icons::TERMINAL, &terminal_label).on_click(move |_, _, cx| {
            e5.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::OpenInTerminal(path.clone()));
            });
        })
    })
    .item({
        let path = path_for_properties.clone();
        menu_item_element(icons::INFO, &properties_label).on_click(move |_, _, cx| {
            e6.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::Properties(path.clone()));
            });
        })
    })
}

/// 构建空白区域右键菜单
fn build_empty_area_context_menu(
    menu: gpui_component::menu::PopupMenu,
    lang: &Language,
    entity: Entity<FileListView>,
) -> gpui_component::menu::PopupMenu {
    let refresh_label = t(lang, "sftp.context_menu.refresh").to_string();
    let new_folder_label = t(lang, "sftp.context_menu.new_folder").to_string();
    let new_file_label = t(lang, "sftp.context_menu.new_file").to_string();
    let upload_file_label = t(lang, "sftp.context_menu.upload_file").to_string();
    let upload_folder_label = t(lang, "sftp.context_menu.upload_folder").to_string();
    let select_all_label = t(lang, "sftp.context_menu.select_all").to_string();

    let e1 = entity.clone();
    let e2 = entity.clone();
    let e3 = entity.clone();
    let e4 = entity.clone();
    let e5 = entity.clone();
    let e6 = entity.clone();

    menu.item(
        menu_item_element(icons::REFRESH, &refresh_label).on_click(move |_, _, cx| {
            e1.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::Refresh);
            });
        }),
    )
    .separator()
    .item(
        menu_item_element(icons::FOLDER_PLUS, &new_folder_label).on_click(move |_, _, cx| {
            e2.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::NewFolder);
            });
        }),
    )
    .item(
        menu_item_element(icons::FILE, &new_file_label).on_click(move |_, _, cx| {
            e3.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::NewFile);
            });
        }),
    )
    .separator()
    .item(
        menu_item_element(icons::UPLOAD, &upload_file_label).on_click(move |_, _, cx| {
            e4.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::UploadFile);
            });
        }),
    )
    .item(
        menu_item_element(icons::UPLOAD, &upload_folder_label).on_click(move |_, _, cx| {
            e5.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::UploadFolder);
            });
        }),
    )
    .separator()
    .item(
        menu_item_element(icons::CHECK, &select_all_label).on_click(move |_, _, cx| {
            e6.update(cx, |_, cx| {
                cx.emit(FileListContextMenuEvent::SelectAll);
            });
        }),
    )
}

/// 创建带图标的菜单项元素
fn menu_item_element(icon: &str, label: &str) -> PopupMenuItem {
    menu_item_with(icon, label, false)
}

/// 危险操作（删除）的菜单项：图标和文字用危险色
fn menu_item_danger(icon: &str, label: &str) -> PopupMenuItem {
    menu_item_with(icon, label, true)
}

fn menu_item_with(icon: &str, label: &str, danger: bool) -> PopupMenuItem {
    let icon = icon.to_string();
    let label = label.to_string();
    PopupMenuItem::element(move |_window, cx| {
        let p = crate::theme::palette(cx);
        let (icon_color, text_color) = if danger {
            (p.danger, p.danger)
        } else {
            (p.text_muted, p.text)
        };
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                svg()
                    .path(icon.clone())
                    .size(px(14.))
                    .text_color(icon_color),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(text_color)
                    .child(label.clone()),
            )
    })
}
