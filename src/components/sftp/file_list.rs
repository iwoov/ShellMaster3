// SFTP 文件列表组件
// 表格形式展示当前目录内容

use gpui::*;
use gpui_component::scroll::ScrollableElement;
use gpui_component::{ActiveTheme, InteractiveElementExt};

use crate::constants::icons;
use crate::i18n::t;
use crate::models::sftp::{FileEntry, FileType, SftpState};

/// 表头高度
const HEADER_HEIGHT: f32 = 28.0;
/// 行高度
const ROW_HEIGHT: f32 = 26.0;
/// 图标尺寸
const ICON_SIZE: f32 = 16.0;

/// 文件列表事件
#[derive(Clone, Debug)]
pub enum FileListEvent {
    Select(String),
    Open(String),
    ContextMenu(String),
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

/// 渲染表头
fn render_header(cx: &App) -> impl IntoElement {
    let border_color = cx.theme().border;
    let muted = cx.theme().muted_foreground;
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    div()
        .w_full()
        .h(px(HEADER_HEIGHT))
        .flex()
        .items_center()
        .gap_2() // 列间距
        .border_b_1()
        .border_color(border_color)
        .px_2()
        // 名称列
        .child(
            div()
                .flex_1()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(muted)
                .child(t(&lang, "sftp.header.name")),
        )
        // 权限列
        .child(
            div()
                .w(px(85.))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(muted)
                .child(t(&lang, "sftp.header.permissions")),
        )
        // 用户/组列
        .child(
            div()
                .w(px(100.))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(muted)
                .child(t(&lang, "sftp.header.owner")),
        )
        // 大小列
        .child(
            div()
                .w(px(65.))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(muted)
                .child(t(&lang, "sftp.header.size")),
        )
        // 修改时间列
        .child(
            div()
                .w(px(130.))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(muted)
                .child(t(&lang, "sftp.header.modified")),
        )
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

/// 渲染文件行
fn render_file_row<F>(entry: &FileEntry, state: &SftpState, on_open: F, cx: &App) -> AnyElement
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let hover_bg = cx.theme().list_active_border;

    let icon = get_file_icon(entry);
    let icon_color = if entry.is_dir() {
        cx.theme().link
    } else {
        muted
    };

    // 使用 SftpState 的 format_owner 方法获取用户名和组名
    let owner = state.format_owner(entry.uid, entry.gid);

    div()
        .id(SharedString::from(format!("sftp-file-row-{}", entry.path)))
        .w_full()
        .h(px(ROW_HEIGHT))
        .flex()
        .items_center()
        .gap_2() // 列间距
        .px_2()
        .cursor_pointer()
        .hover(|s| s.bg(hover_bg))
        .on_double_click(on_open)
        // 名称列
        .child(
            div()
                .flex_1()
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
                ),
        )
        // 权限列
        .child(
            div()
                .w(px(85.))
                .text_xs()
                .text_color(muted)
                .font_family("monospace")
                .child(entry.format_permissions()),
        )
        // 用户/组列
        .child(
            div()
                .w(px(100.))
                .text_xs()
                .text_color(muted)
                .overflow_hidden()
                .text_ellipsis()
                .child(owner),
        )
        // 大小列
        .child(
            div()
                .w(px(65.))
                .text_xs()
                .text_color(muted)
                .child(entry.format_size()),
        )
        // 修改时间列
        .child(
            div()
                .w(px(130.))
                .text_xs()
                .text_color(muted)
                .child(format_modified_time(entry)),
        )
        .into_any_element()
}

/// 渲染文件列表
pub fn render_file_list<F>(state: Option<&SftpState>, on_event: F, cx: &App) -> impl IntoElement
where
    F: Fn(FileListEvent, &mut App) + Clone + 'static,
{
    let bg_color = crate::theme::sidebar_color(cx);
    let muted_foreground = cx.theme().muted_foreground;

    match state {
        Some(s) if !s.loading => {
            let rows: Vec<AnyElement> = s
                .file_list
                .iter()
                .map(|entry| {
                    let entry_path = entry.path.clone();
                    let on_open = on_event.clone();
                    render_file_row(
                        entry,
                        s,
                        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                            on_open(FileListEvent::Open(entry_path.clone()), cx);
                        },
                        cx,
                    )
                })
                .collect();

            if rows.is_empty() {
                div()
                    .size_full()
                    .bg(bg_color)
                    .flex()
                    .flex_col()
                    .child(render_header(cx))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(div().text_sm().text_color(muted_foreground).child("空目录")),
                    )
                    .into_any_element()
            } else {
                div()
                    .size_full()
                    .bg(bg_color)
                    .flex()
                    .flex_col()
                    .child(render_header(cx))
                    .child(
                        div()
                            .id("sftp-file-list-scroll")
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scrollbar()
                            .children(rows),
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
                    .child("正在加载..."),
            )
            .into_any_element(),
        None => div()
            .size_full()
            .bg(bg_color)
            .flex()
            .items_center()
            .justify_center()
            .child(div().text_sm().text_color(muted_foreground).child("未连接"))
            .into_any_element(),
    }
}
