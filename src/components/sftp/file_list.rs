// SFTP 文件列表组件 - 使用 gpui_kit::component::table::DataTable 实现
// 支持列排序、手动/自适应列宽、内联过滤、快捷键、右键菜单与底部状态栏

use std::cmp::Reverse;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::table::{
    Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState,
};
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::components::common::button::{icon_button_sized, text_button, ButtonKind};
use crate::components::common::format::format_bytes;
use crate::components::common::icon::icon_sized;
use crate::components::common::panel::{panel_state, spinner, StateIcon};
use crate::components::common::text::TextFitter;
use crate::constants::icons;
use crate::i18n::t;
use crate::models::settings::Language;
use crate::models::sftp::{FileEntry, FileType, SftpState};
use crate::theme::Palette;

/// SFTP 文件列表右键菜单与快捷操作事件
#[derive(Clone, Debug)]
pub enum FileListContextMenuEvent {
    // 文件/条目操作
    OpenItem(String),   // 智能打开（目录进入、软链解析目标、普通文件编辑）
    Download(String),   // 文件路径
    EditFile(String),   // 文件路径
    CopyName(String),   // 文件名
    CopyPath(String),   // 完整路径
    Rename(String),     // 文件路径 - 开始重命名
    Delete(String),     // 文件路径 - 请求删除（先弹确认框）
    Properties(String), // 文件路径

    // 文件夹操作
    OpenFolder(String),     // 文件夹路径
    DownloadFolder(String), // 文件夹路径
    OpenInTerminal(String), // 目录路径

    // 空白区域与导航操作
    Refresh,
    GoUp,
    NewFolder,
    NewFile,
    UploadFile,
    UploadFolder,

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

/// 默认列宽常量
const NAME_COLUMN_WIDTH: Pixels = px(260.);
const NAME_COLUMN_MIN: Pixels = px(160.);
const NAME_COLUMN_MAX: Pixels = px(480.);
const SIZE_COLUMN_WIDTH: Pixels = px(86.);
const MODIFIED_COLUMN_WIDTH: Pixels = px(136.);
const PERMISSIONS_COLUMN_WIDTH: Pixels = px(96.);
const OWNER_COLUMN_WIDTH: Pixels = px(108.);
/// 其他列宽 + 滚动条安全边距，用于动态计算名称列自适应宽度
const OTHER_COLUMNS_TOTAL_WIDTH: f32 = 86. + 136. + 96. + 108. + 18.;

/// 单元格左右内边距（`Table::small()`）
const CELL_PADDING_X: f32 = 6.;
/// 名称前图标与文字的间距
const NAME_GAP: f32 = 8.;
/// 空白区域（空目录或列表下方空白）右键的哨兵行号
///
/// `DataTable` 仅在 `right_clicked_row` 为 `Some(_)` 时回调 `TableDelegate::context_menu`，
/// 且内部仅做 `== Some(row_ix)` 比较而不用于数组索引。
/// 外层容器在 Capture 阶段先写入该哨兵值；若右键落在真实行上，行的 Bubble 处理器会将其覆盖为真实行号。
const EMPTY_AREA_ROW_SENTINEL: usize = usize::MAX;

/// 行拖放回调类型
pub type RowDropCallback =
    std::sync::Arc<dyn Fn(Vec<std::path::PathBuf>, String) + Send + Sync + 'static>;

/// 预计算的行渲染缓存：仅在数据或用户/组缓存变更时构建，render_td 零格式化分配
#[derive(Clone)]
struct FormattedRow {
    name_lower: String,
    icon: &'static str,
    size_text: SharedString,
    modified_text: SharedString,
    permissions_text: SharedString,
    owner_text: SharedString,
    row_id: u64,
    name_element_id: SharedString,
}

impl FormattedRow {
    fn from_entry(
        entry: &FileEntry,
        user_cache: &HashMap<u32, String>,
        group_cache: &HashMap<u32, String>,
    ) -> Self {
        let size_text: SharedString = if entry.is_dir() {
            "—".into()
        } else {
            format_bytes(entry.size).into()
        };
        Self {
            name_lower: entry.name.to_lowercase(),
            icon: get_file_icon(entry),
            size_text,
            modified_text: format_modified_time(entry).into(),
            permissions_text: entry.format_permissions().into(),
            owner_text: format_owner_pair(entry.uid, entry.gid, user_cache, group_cache).into(),
            row_id: hash_row_id(&entry.path),
            name_element_id: SharedString::from(format!("sftp-name-{}", entry.path)),
        }
    }
}

/// 根据列表可用宽度计算名称列自适应宽度
fn adaptive_name_column_width(available_width: Pixels) -> Pixels {
    (available_width - px(OTHER_COLUMNS_TOTAL_WIDTH)).clamp(NAME_COLUMN_MIN, NAME_COLUMN_MAX)
}

/// 文件列表 Delegate - 实现 TableDelegate trait
pub struct FileListDelegate {
    /// 文件列表数据（与 SftpState 共享 Arc，零拷贝同步）
    pub file_list: Arc<Vec<FileEntry>>,
    /// 预计算格式化缓存（与 file_list 等长一一对应）
    formatted_rows: Vec<FormattedRow>,
    /// 当前显示行顺序（过滤并排序后的 file_list 索引）
    row_order: Vec<usize>,
    /// 当前小写过滤关键字（空串表示不过滤）
    filter_query_lower: String,
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
    /// 排序时是否将文件夹置顶（跟随 `settings.sftp.folders_first`）
    folders_first: bool,
    /// 正在编辑的文件路径（用于内联重命名）
    pub editing_path: Option<String>,
    /// 重命名输入框状态（用于内联编辑）
    pub rename_input: Option<Entity<InputState>>,
    /// 行拖放回调（当文件拖放到文件夹行上时调用）
    pub on_row_drop: Option<RowDropCallback>,
    /// 名称列当前宽度（拖动或自适应列宽后更新），用于按实际宽度截断文件名
    pub name_column_width: Pixels,
    /// 每帧由 FileListView 注入一次的 TextFitter，避免每个单元格重复构造
    fitter: Option<TextFitter>,
    /// 所属 FileListView 的弱引用（用于右键菜单发出事件，避免循环引用）
    view: Option<WeakEntity<FileListView>>,
}

impl FileListDelegate {
    /// 创建新的 FileListDelegate
    pub fn new(lang: Language) -> Self {
        let columns = Self::create_columns(&lang);
        let mut delegate = Self {
            file_list: Arc::new(Vec::new()),
            formatted_rows: Vec::new(),
            row_order: Vec::new(),
            filter_query_lower: String::new(),
            columns,
            user_cache: HashMap::new(),
            group_cache: HashMap::new(),
            lang,
            current_sort_col: COL_NAME,
            current_sort: ColumnSort::Ascending,
            folders_first: true,
            editing_path: None,
            rename_input: None,
            on_row_drop: None,
            name_column_width: NAME_COLUMN_WIDTH,
            fitter: None,
            view: None,
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
                .width(SIZE_COLUMN_WIDTH)
                .sortable(),
            Column::new("modified", t(lang, "sftp.header.modified"))
                .width(MODIFIED_COLUMN_WIDTH)
                .sortable(),
            Column::new("permissions", t(lang, "sftp.header.permissions"))
                .width(PERMISSIONS_COLUMN_WIDTH)
                .resizable(true),
            Column::new("owner", t(lang, "sftp.header.owner"))
                .width(OWNER_COLUMN_WIDTH)
                .resizable(true),
        ]
    }

    /// 设置名称列宽（同步更新列定义与截断宽度）
    pub fn set_name_column_width(&mut self, width: Pixels) {
        self.name_column_width = width;
        if let Some(column) = self.columns.get_mut(COL_NAME) {
            column.width = width;
        }
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

    /// 重建预计算格式化缓存
    fn rebuild_formatted_rows(&mut self) {
        self.formatted_rows.clear();
        self.formatted_rows.reserve(self.file_list.len());
        for entry in self.file_list.iter() {
            self.formatted_rows.push(FormattedRow::from_entry(
                entry,
                &self.user_cache,
                &self.group_cache,
            ));
        }
    }

    /// 仅刷新所有者列文本（user_cache / group_cache 更新时）
    fn refresh_owner_texts(&mut self) {
        for (entry, formatted) in self.file_list.iter().zip(self.formatted_rows.iter_mut()) {
            formatted.owner_text =
                format_owner_pair(entry.uid, entry.gid, &self.user_cache, &self.group_cache).into();
        }
    }

    /// 按当前过滤条件填充 row_order
    fn rebuild_filtered_indices(&mut self) {
        self.row_order.clear();
        if self.filter_query_lower.is_empty() {
            self.row_order.extend(0..self.file_list.len());
        } else {
            let needle = self.filter_query_lower.as_str();
            for (ix, formatted) in self.formatted_rows.iter().enumerate() {
                if formatted.name_lower.contains(needle) {
                    self.row_order.push(ix);
                }
            }
        }
    }

    /// 更新文件列表（O(1) Arc 共享 + 一次性构建格式化缓存）
    pub fn update_file_list(&mut self, entries: Arc<Vec<FileEntry>>) {
        self.file_list = entries;
        self.rebuild_formatted_rows();
        self.apply_filter_and_sort();
    }

    /// 设置过滤关键字（不区分大小写）
    pub fn set_filter(&mut self, query: &str) {
        let lower = query.trim().to_lowercase();
        if self.filter_query_lower == lower {
            return;
        }
        self.filter_query_lower = lower;
        self.apply_filter_and_sort();
    }

    /// 设置排序时是否将文件夹置顶，变化时立即重排
    pub fn set_folders_first(&mut self, folders_first: bool) -> bool {
        if self.folders_first == folders_first {
            return false;
        }
        self.folders_first = folders_first;
        self.apply_filter_and_sort();
        true
    }

    /// 更新用户缓存
    pub fn update_user_cache(&mut self, cache: HashMap<u32, String>) {
        self.user_cache = cache;
        self.refresh_owner_texts();
    }

    /// 更新组缓存
    pub fn update_group_cache(&mut self, cache: HashMap<u32, String>) {
        self.group_cache = cache;
        self.refresh_owner_texts();
    }

    /// 设置并应用排序
    fn sort_file_list(&mut self, col_ix: usize, sort: ColumnSort) {
        self.current_sort_col = col_ix;
        self.current_sort = sort;
        self.sync_column_sort_state();
        self.apply_filter_and_sort();
    }

    /// 应用当前过滤条件与排序状态
    fn apply_filter_and_sort(&mut self) {
        self.rebuild_filtered_indices();

        if self.current_sort == ColumnSort::Default {
            return;
        }

        let entries = &self.file_list;
        let formatted = &self.formatted_rows;
        let folders_first = self.folders_first;
        let dir_rank = |is_dir: bool| -> u8 {
            if folders_first && is_dir {
                0
            } else if folders_first {
                1
            } else {
                0
            }
        };
        match (self.current_sort_col, self.current_sort) {
            (COL_NAME, ColumnSort::Ascending) => {
                self.row_order.sort_by(|&a, &b| {
                    let ra = dir_rank(entries[a].is_dir());
                    let rb = dir_rank(entries[b].is_dir());
                    ra.cmp(&rb)
                        .then_with(|| formatted[a].name_lower.cmp(&formatted[b].name_lower))
                        .then_with(|| a.cmp(&b))
                });
            }
            (COL_NAME, ColumnSort::Descending) => {
                self.row_order.sort_by(|&a, &b| {
                    let ra = dir_rank(entries[a].is_dir());
                    let rb = dir_rank(entries[b].is_dir());
                    ra.cmp(&rb)
                        .then_with(|| formatted[b].name_lower.cmp(&formatted[a].name_lower))
                        .then_with(|| a.cmp(&b))
                });
            }
            (COL_SIZE, ColumnSort::Ascending) => {
                self.row_order.sort_by_cached_key(|&ix| {
                    let entry = &entries[ix];
                    (dir_rank(entry.is_dir()), entry.size, ix)
                });
            }
            (COL_SIZE, ColumnSort::Descending) => {
                self.row_order.sort_by_cached_key(|&ix| {
                    let entry = &entries[ix];
                    (dir_rank(entry.is_dir()), Reverse(entry.size), ix)
                });
            }
            (COL_MODIFIED, ColumnSort::Ascending) => {
                self.row_order.sort_by_cached_key(|&ix| {
                    let entry = &entries[ix];
                    (dir_rank(entry.is_dir()), entry.modified, ix)
                });
            }
            (COL_MODIFIED, ColumnSort::Descending) => {
                self.row_order.sort_by_cached_key(|&ix| {
                    let entry = &entries[ix];
                    (dir_rank(entry.is_dir()), Reverse(entry.modified), ix)
                });
            }
            _ => {}
        }
    }

    /// 获取指定行的文件路径
    pub fn get_file_path(&self, row_ix: usize) -> Option<String> {
        self.row_order
            .get(row_ix)
            .and_then(|&ix| self.file_list.get(ix))
            .map(|e| e.path.clone())
    }

    /// 获取原始文件列表是否为空
    pub fn is_empty(&self) -> bool {
        self.file_list.is_empty()
    }

    /// 原始条目总数
    pub fn total_count(&self) -> usize {
        self.file_list.len()
    }

    /// 过滤后可见条目数
    pub fn visible_count(&self) -> usize {
        self.row_order.len()
    }

    /// 是否正处于过滤状态
    pub fn has_active_filter(&self) -> bool {
        !self.filter_query_lower.is_empty()
    }
}

fn format_owner_pair(
    uid: Option<u32>,
    gid: Option<u32>,
    user_cache: &HashMap<u32, String>,
    group_cache: &HashMap<u32, String>,
) -> String {
    let user = uid
        .and_then(|u| user_cache.get(&u).cloned())
        .unwrap_or_else(|| {
            uid.map(|u| u.to_string())
                .unwrap_or_else(|| "-".to_string())
        });
    let group = gid
        .and_then(|g| group_cache.get(&g).cloned())
        .unwrap_or_else(|| {
            gid.map(|g| g.to_string())
                .unwrap_or_else(|| "-".to_string())
        });
    format!("{}:{}", user, group)
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

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        self.columns[col_ix].clone()
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
        let (Some(entry), Some(formatted)) =
            (self.file_list.get(entry_ix), self.formatted_rows.get(entry_ix))
        else {
            return div().into_any_element();
        };

        let p = crate::theme::palette(cx);
        let mono = cx.theme().mono_font_family.clone();

        match col_ix {
            COL_NAME => {
                let icon = formatted.icon;
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
                                    .child(gpui_kit::Styled::h(field, px(24.)).rounded(px(6.))),
                            )
                            .into_any_element();
                    }
                }

                let name_width = (self.name_column_width
                    - px(CELL_PADDING_X * 2. + ICON_SIZE + NAME_GAP))
                .max(px(24.));
                let fit = self
                    .fitter
                    .clone()
                    .unwrap_or_else(|| TextFitter::new(window, cx));
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
                            formatted.name_element_id.clone(),
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
                .child(formatted.size_text.clone())
                .into_any_element(),
            COL_MODIFIED => cell()
                .whitespace_nowrap()
                .text_size(px(12.))
                .text_color(p.text_muted)
                .child(formatted.modified_text.clone())
                .into_any_element(),
            COL_PERMISSIONS => cell()
                .whitespace_nowrap()
                .font_family(mono)
                .text_size(px(12.))
                .text_color(p.text_faint)
                .child(formatted.permissions_text.clone())
                .into_any_element(),
            COL_OWNER => cell()
                .whitespace_nowrap()
                .text_size(px(12.))
                .text_color(p.text_faint)
                .child(formatted.owner_text.clone())
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
        let entry_ix = self.row_order.get(row_ix).copied();
        let entry = entry_ix.and_then(|ix| self.file_list.get(ix));
        let row_id = entry_ix
            .and_then(|ix| self.formatted_rows.get(ix))
            .map(|f| f.row_id)
            .unwrap_or(row_ix as u64);

        let is_dir = entry.map(|e| e.is_dir()).unwrap_or(false);
        let folder_path = entry.map(|e| e.path.clone());
        let on_row_drop = self.on_row_drop.clone();

        let base_div = div().id(("file-row", row_id));

        // 如果是文件夹，添加拖放处理器
        if is_dir {
            base_div
                .drag_over::<ExternalPaths>(|this, _, _, cx| {
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

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        let Some(entity) = self.view.as_ref().and_then(|w| w.upgrade()) else {
            return menu;
        };
        let entry = if row_ix == EMPTY_AREA_ROW_SENTINEL {
            None
        } else {
            self.row_order
                .get(row_ix)
                .and_then(|&ix| self.file_list.get(ix))
        };

        match entry {
            Some(entry) if entry.is_dir() => {
                build_folder_context_menu(menu, entry, &self.lang, entity)
            }
            Some(entry) => build_file_context_menu(menu, entry, &self.lang, entity),
            None => build_empty_area_context_menu(menu, &self.lang, entity),
        }
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let p = crate::theme::palette(cx);
        let (icon, message) = if self.has_active_filter() && !self.file_list.is_empty() {
            (icons::SEARCH, t(&self.lang, "sftp.filter.no_matches"))
        } else {
            (icons::FOLDER_OPEN, t(&self.lang, "sftp.empty_folder"))
        };
        panel_state(
            "sftp-empty-folder",
            StateIcon::Icon(icon),
            p.text_faint,
            message,
            None,
            &p,
        )
    }

    fn loading(&self, _cx: &App) -> bool {
        false
    }
}

/// 文件列表视图组件 - 管理 Table Entity、过滤框、状态栏与键盘快捷键
pub struct FileListView {
    /// Table 状态
    table_state: Entity<TableState<FileListDelegate>>,
    /// 列表键盘焦点句柄
    focus_handle: FocusHandle,
    /// 内联过滤输入框
    filter_input: Entity<InputState>,
    /// 过滤栏是否展开
    filter_open: bool,
    /// 切换目录后需要在下一次带 Window 的渲染中清空输入框文本
    pending_clear_filter: bool,
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
    /// 用户是否手动拖动过列宽（手动调整后不再自动覆盖）
    user_resized_columns: bool,
    /// 上次自动设置的名称列宽度
    last_auto_name_width: Pixels,
    /// 文件列表版本
    last_file_list_revision: u64,
    /// 用户缓存版本
    last_user_cache_revision: u64,
    /// 组缓存版本
    last_group_cache_revision: u64,
    /// 内联重命名输入框
    rename_input: Option<Entity<InputState>>,
    /// 正在编辑的文件路径
    editing_path: Option<String>,
    /// 当前远程路径（用于拖放上传和路径变化时清空过滤）
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

        let focus_handle = cx.focus_handle();

        // 创建内联过滤输入框
        let placeholder = t(&lang, "sftp.filter.placeholder").to_string();
        let filter_input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));

        cx.subscribe(&filter_input, |this, input, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                let query = input.read(cx).value().to_string();
                this.table_state.update(cx, |table, cx| {
                    table.delegate_mut().set_filter(&query);
                    cx.notify();
                });
                cx.notify();
            }
        })
        .detach();

        // 创建共享事件队列
        let pending_row_drops: std::sync::Arc<
            std::sync::Mutex<Vec<(Vec<std::path::PathBuf>, String)>>,
        > = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let pending_row_drops_for_callback = pending_row_drops.clone();

        // 创建 delegate 并设置回调
        let mut delegate = FileListDelegate::new(lang.clone());
        delegate.view = Some(cx.entity().downgrade());
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
                    if let Some(width) = widths.get(COL_NAME).copied() {
                        if (width - this.last_auto_name_width).abs() > px(1.) {
                            this.user_resized_columns = true;
                        }
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
            focus_handle,
            filter_input,
            filter_open: false,
            pending_clear_filter: false,
            loading: false,
            connected: false,
            error: None,
            lang,
            column_widths: Vec::new(),
            user_resized_columns: false,
            last_auto_name_width: NAME_COLUMN_WIDTH,
            last_file_list_revision: 0,
            last_user_cache_revision: 0,
            last_group_cache_revision: 0,
            rename_input: None,
            editing_path: None,
            current_path: String::new(),
            pending_row_drops,
        }
    }

    /// 过滤栏是否处于打开状态
    pub fn is_filter_open(&self) -> bool {
        self.filter_open
    }

    /// 切换内联过滤栏开关
    pub fn toggle_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter_open {
            self.close_filter(window, cx);
        } else {
            self.open_filter(window, cx);
        }
    }

    /// 打开并聚焦内联过滤栏
    pub fn open_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter_open = true;
        self.filter_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// 关闭并清空内联过滤栏
    pub fn close_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter_open = false;
        self.pending_clear_filter = false;
        self.filter_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        self.table_state.update(cx, |table, cx| {
            table.delegate_mut().set_filter("");
            cx.notify();
        });
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// 根据父容器可用宽度自适应名称列宽（用户手动调整过列宽后保持用户设置）
    pub fn sync_available_width(&mut self, available_width: Pixels, cx: &mut Context<Self>) {
        if self.user_resized_columns {
            return;
        }
        let target = adaptive_name_column_width(available_width);
        if (target - self.last_auto_name_width).abs() < px(4.) {
            return;
        }
        self.last_auto_name_width = target;
        self.table_state.update(cx, |table, cx| {
            table.delegate_mut().set_name_column_width(target);
            table.refresh(cx);
        });
    }

    /// 从 SftpState 同步数据
    pub fn sync_from_sftp_state(&mut self, sftp_state: Option<&SftpState>, cx: &mut Context<Self>) {
        let mut needs_notify = false;

        let settings = crate::services::storage::cached_settings();
        let lang = settings.theme.language.clone();
        let folders_first = settings.sftp.folders_first;
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
        if self.table_state.read(cx).delegate().folders_first != folders_first {
            self.table_state.update(cx, |table_state, cx| {
                if table_state.delegate_mut().set_folders_first(folders_first) {
                    cx.notify();
                }
            });
            needs_notify = true;
        }

        match sftp_state {
            Some(state) => {
                if !self.connected {
                    self.connected = true;
                    needs_notify = true;
                }
                // 切换目录时清空旧过滤条件，避免新目录因残留关键字显示为空
                if self.current_path != state.current_path {
                    self.current_path = state.current_path.clone();
                    if self.table_state.read(cx).delegate().has_active_filter() {
                        self.table_state.update(cx, |table, cx| {
                            table.delegate_mut().set_filter("");
                            cx.notify();
                        });
                        self.pending_clear_filter = true;
                        needs_notify = true;
                    }
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
                            if user_cache_changed {
                                delegate.update_user_cache(state.user_cache.clone());
                            }
                            if group_cache_changed {
                                delegate.update_group_cache(state.group_cache.clone());
                            }
                            if file_list_changed {
                                delegate.update_file_list(Arc::clone(&state.file_list));
                            }
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
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();

        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(name, window, cx);
            state
        });

        cx.subscribe(&input, |this, _input, event: &InputEvent, cx| match event {
            InputEvent::PressEnter { .. } => {
                this.confirm_rename(cx);
            }
            InputEvent::Blur => {
                this.cancel_rename(cx);
            }
            _ => {}
        })
        .detach();

        input.update(cx, |input, cx| {
            input.focus(window, cx);
        });

        self.rename_input = Some(input.clone());
        self.editing_path = Some(path.clone());

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

        self.table_state.update(cx, |state, _| {
            let delegate = state.delegate_mut();
            delegate.editing_path = None;
            delegate.rename_input = None;
        });

        cx.emit(FileListContextMenuEvent::RenameCancelled);
        cx.notify();
    }

    /// 处理文件列表容器键盘快捷键
    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let mods = &event.keystroke.modifiers;
        let cmd_or_ctrl = mods.platform || mods.control;

        // Cmd/Ctrl+F 随时可唤起并聚焦过滤框
        if cmd_or_ctrl && key.eq_ignore_ascii_case("f") {
            self.open_filter(window, cx);
            cx.stop_propagation();
            return;
        }

        // Esc：优先取消重命名，其次关闭过滤栏
        if key == "escape" {
            if self.editing_path.is_some() {
                self.cancel_rename(cx);
                cx.stop_propagation();
                return;
            }
            if self.filter_open {
                self.close_filter(window, cx);
                cx.stop_propagation();
                return;
            }
        }

        // 重命名输入框或过滤输入框处于聚焦状态时，不拦截普通按键
        let filter_focused = self.filter_input.read(cx).focus_handle(cx).is_focused(window);
        if self.editing_path.is_some() || filter_focused {
            return;
        }

        // Cmd/Ctrl+R 或 F5：刷新目录
        if (cmd_or_ctrl && key.eq_ignore_ascii_case("r")) || key == "f5" {
            cx.emit(FileListContextMenuEvent::Refresh);
            cx.stop_propagation();
            return;
        }

        // Cmd+Up 或无修饰键 Backspace：返回上级目录
        if (mods.platform && key == "up")
            || (key == "backspace" && !mods.platform && !mods.control && !mods.alt)
        {
            cx.emit(FileListContextMenuEvent::GoUp);
            cx.stop_propagation();
            return;
        }

        // 以下快捷键作用于当前选中项
        let Some(selected) = self.get_selected_file(cx) else {
            return;
        };

        match key {
            "enter" => {
                cx.emit(FileListContextMenuEvent::OpenItem(selected.path));
                cx.stop_propagation();
            }
            "f2" => {
                self.start_rename(selected.path, window, cx);
                cx.stop_propagation();
            }
            "delete" => {
                cx.emit(FileListContextMenuEvent::Delete(selected.path));
                cx.stop_propagation();
            }
            "backspace" if mods.platform => {
                cx.emit(FileListContextMenuEvent::Delete(selected.path));
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    /// 渲染顶部内联过滤条
    fn render_filter_bar(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let delegate = self.table_state.read(cx).delegate();
        let visible = delegate.visible_count();
        let total = delegate.total_count();
        let has_query = delegate.has_active_filter();
        let count_text = if has_query {
            format!("{} / {}", visible, total)
        } else {
            String::new()
        };

        div()
            .flex_none()
            .h(px(32.))
            .px(px(8.))
            .bg(p.sidebar)
            .border_b_1()
            .border_color(p.border)
            .flex()
            .items_center()
            .gap(px(6.))
            .child(icon_sized(icons::SEARCH, 13., p.text_faint))
            .child(
                div().flex_1().min_w_0().child(
                    gpui_kit::Styled::h(
                        Input::new(&self.filter_input)
                            .appearance(false)
                            .text_size(px(12.))
                            .py_0(),
                        px(24.),
                    ),
                ),
            )
            .when(!count_text.is_empty(), |bar| {
                bar.child(
                    div()
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(p.text_faint)
                        .whitespace_nowrap()
                        .child(count_text),
                )
            })
            .child(
                icon_button_sized(
                    "sftp-filter-close",
                    icons::X,
                    px(22.),
                    12.,
                    t(&self.lang, "sftp.filter.clear"),
                    p,
                )
                .on_click(cx.listener(|view, _, window, cx| {
                    view.close_filter(window, cx);
                })),
            )
    }

    /// 渲染底部轻量状态栏（总项数 / 过滤命中数 / 当前选中项摘要）
    fn render_status_bar(&self, p: &Palette, cx: &App) -> Div {
        let delegate = self.table_state.read(cx).delegate();
        let total = delegate.total_count();
        let visible = delegate.visible_count();
        let left_text = if delegate.has_active_filter() {
            t(&self.lang, "sftp.status.filtered_items")
                .replace("{matched}", &visible.to_string())
                .replace("{total}", &total.to_string())
        } else {
            t(&self.lang, "sftp.status.items").replace("{count}", &total.to_string())
        };

        let right_text = self.get_selected_file(cx).map(|entry| {
            if entry.is_dir() {
                t(&self.lang, "sftp.status.selected_folder").replace("{name}", &entry.name)
            } else {
                t(&self.lang, "sftp.status.selected_file")
                    .replace("{name}", &entry.name)
                    .replace("{size}", &format_bytes(entry.size))
            }
        });

        div()
            .flex_none()
            .h(px(24.))
            .px(px(10.))
            .bg(p.sidebar)
            .border_t_1()
            .border_color(p.border)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .text_size(px(11.))
            .text_color(p.text_faint)
            .child(div().flex_none().whitespace_nowrap().child(left_text))
            .children(right_text.map(|text| {
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_right()
                    .text_color(p.text_muted)
                    .child(text)
            }))
    }
}

impl EventEmitter<TableEvent> for FileListView {}
impl EventEmitter<FileListContextMenuEvent> for FileListView {}

impl Render for FileListView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 切换目录后在持有 Window 时清空过滤输入框文本
        if self.pending_clear_filter {
            self.pending_clear_filter = false;
            self.filter_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
        }

        // 处理待处理的行拖放事件
        if let Ok(mut queue) = self.pending_row_drops.lock() {
            for (paths, target_dir) in queue.drain(..) {
                cx.emit(FileListContextMenuEvent::DropFiles { paths, target_dir });
            }
        }

        // 每帧注入一次 TextFitter，避免每个文件名单元格重复构造
        let fitter = TextFitter::new(window, cx);
        self.table_state.update(cx, |table, _| {
            table.delegate_mut().fitter = Some(fitter);
        });

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

        let drop_border = p.primary;
        let table_state_for_rclick = self.table_state.clone();
        let focus_handle_for_click = self.focus_handle.clone();
        let is_editing = self.editing_path.is_some();

        // 正常状态：表格 + 可选过滤栏 + 底部状态栏
        let table = DataTable::new(&self.table_state)
            .small()
            .stripe(false)
            .bordered(false);

        let table_area = div()
            .id("sftp-file-list-table-area")
            .flex_1()
            .min_h(px(0.))
            .relative()
            // 常态就有 1px 透明边框，拖入时只变色，不挤动内容
            .border_1()
            .border_color(gpui_kit::transparent_black())
            // Capture 阶段预置空白区域哨兵；若右键落在真实行上，行的 Bubble 处理器会覆盖为真实 row_ix
            .capture_any_mouse_down(move |event, window, cx| {
                if event.button == MouseButton::Right {
                    table_state_for_rclick.update(cx, |table, cx| {
                        table.set_right_clicked_row(Some(EMPTY_AREA_ROW_SENTINEL), cx);
                    });
                } else if event.button == MouseButton::Left && !is_editing {
                    focus_handle_for_click.focus(window, cx);
                }
            })
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
                    let paths: Vec<std::path::PathBuf> =
                        external_paths.paths().iter().cloned().collect();
                    if !paths.is_empty() {
                        let target_dir = view.current_path.clone();
                        cx.emit(FileListContextMenuEvent::DropFiles { paths, target_dir });
                    }
                }),
            );

        div()
            .id("sftp-file-list-container")
            .size_full()
            .bg(p.background)
            .flex()
            .flex_col()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .when(self.filter_open, |container| {
                container.child(self.render_filter_bar(&p, cx))
            })
            .child(table_area)
            .child(self.render_status_bar(&p, cx))
            .into_any_element()
    }
}

/// 构建文件（含符号链接）右键菜单
fn build_file_context_menu(
    menu: PopupMenu,
    entry: &FileEntry,
    lang: &Language,
    entity: Entity<FileListView>,
) -> PopupMenu {
    let path = entry.path.clone();
    let name = entry.name.clone();
    let is_symlink = entry.is_symlink();
    let path_for_open = path.clone();
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

    let open_label = t(lang, "sftp.context_menu.open").to_string();
    let download_label = t(lang, "sftp.context_menu.download").to_string();
    let edit_label = t(lang, "sftp.context_menu.edit_file").to_string();
    let copy_name_label = t(lang, "sftp.context_menu.copy_name").to_string();
    let copy_path_label = t(lang, "sftp.context_menu.copy_path").to_string();
    let rename_label = t(lang, "sftp.context_menu.rename").to_string();
    let delete_label = t(lang, "sftp.context_menu.delete").to_string();
    let terminal_label = t(lang, "sftp.context_menu.open_in_terminal").to_string();
    let properties_label = t(lang, "sftp.context_menu.properties").to_string();

    let e_open = entity.clone();
    let e1 = entity.clone();
    let e2 = entity.clone();
    let e3 = entity.clone();
    let e4 = entity.clone();
    let e5 = entity.clone();
    let e6 = entity.clone();
    let e_copy_name = entity.clone();
    let e_copy_path = entity.clone();

    let menu = if is_symlink {
        menu.item(
            menu_item_element(icons::FOLDER_OPEN, &open_label).on_click(move |_, _, cx| {
                e_open.update(cx, |_, cx| {
                    cx.emit(FileListContextMenuEvent::OpenItem(path_for_open.clone()));
                });
            }),
        )
    } else {
        menu
    };

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
    menu: PopupMenu,
    entry: &FileEntry,
    lang: &Language,
    entity: Entity<FileListView>,
) -> PopupMenu {
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
    menu: PopupMenu,
    lang: &Language,
    entity: Entity<FileListView>,
) -> PopupMenu {
    let refresh_label = t(lang, "sftp.context_menu.refresh").to_string();
    let new_folder_label = t(lang, "sftp.context_menu.new_folder").to_string();
    let new_file_label = t(lang, "sftp.context_menu.new_file").to_string();
    let upload_file_label = t(lang, "sftp.context_menu.upload_file").to_string();
    let upload_folder_label = t(lang, "sftp.context_menu.upload_folder").to_string();

    let e1 = entity.clone();
    let e2 = entity.clone();
    let e3 = entity.clone();
    let e4 = entity.clone();
    let e5 = entity.clone();

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
}

/// 创建带图标的菜单项元素
pub(crate) fn menu_item_element(icon: &str, label: &str) -> PopupMenuItem {
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
                    .flex_none()
                    .text_color(icon_color),
            )
            .child(div().text_size(px(13.)).text_color(text_color).child(label.clone()))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        adaptive_name_column_width, FileListDelegate, NAME_COLUMN_MAX, NAME_COLUMN_MIN,
        OTHER_COLUMNS_TOTAL_WIDTH,
    };
    use crate::models::settings::Language;
    use crate::models::sftp::{FileEntry, FileType};
    use gpui_kit::px;
    use std::sync::Arc;

    #[test]
    fn filter_and_sort_keep_folders_first_and_case_insensitive() {
        let mut delegate = FileListDelegate::new(Language::English);
        let entries = Arc::new(vec![
            FileEntry::new("README.md".into(), "/README.md".into(), FileType::File),
            FileEntry::new("config.yaml".into(), "/config.yaml".into(), FileType::File),
            FileEntry::new("configs".into(), "/configs".into(), FileType::Directory),
            FileEntry::new("Alpha.txt".into(), "/Alpha.txt".into(), FileType::File),
        ]);
        delegate.update_file_list(entries);
        assert_eq!(delegate.total_count(), 4);
        assert_eq!(delegate.visible_count(), 4);
        // 文件夹排最前，随后文件按字母升序
        assert_eq!(delegate.get_file_path(0).as_deref(), Some("/configs"));
        assert_eq!(delegate.get_file_path(1).as_deref(), Some("/Alpha.txt"));

        delegate.set_filter("CONFIG");
        assert!(delegate.has_active_filter());
        assert_eq!(delegate.visible_count(), 2);
        assert_eq!(delegate.get_file_path(0).as_deref(), Some("/configs"));
        assert_eq!(delegate.get_file_path(1).as_deref(), Some("/config.yaml"));

        delegate.set_filter("");
        assert!(!delegate.has_active_filter());
        assert_eq!(delegate.visible_count(), 4);
    }

    #[test]
    fn adaptive_name_width_clamps_to_bounds() {
        assert_eq!(adaptive_name_column_width(px(400.)), NAME_COLUMN_MIN);
        assert_eq!(adaptive_name_column_width(px(1400.)), NAME_COLUMN_MAX);
        assert_eq!(
            adaptive_name_column_width(px(700.)),
            px(700. - OTHER_COLUMNS_TOTAL_WIDTH)
        );
    }
}
