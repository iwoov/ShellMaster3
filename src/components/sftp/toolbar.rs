// SFTP 工具栏组件
// 包含导航按钮（返回、前进、上级、主目录）+ 地址栏 + 操作按钮

use gpui::*;
use gpui_component::breadcrumb::{Breadcrumb, BreadcrumbItem};
use gpui_component::ActiveTheme;

use crate::constants::icons;
use crate::models::sftp::SftpState;

/// 工具栏高度
const TOOLBAR_HEIGHT: f32 = 32.0;
/// 按钮尺寸
const BUTTON_SIZE: f32 = 24.0;
/// 图标尺寸
const ICON_SIZE: f32 = 14.0;

/// SFTP 工具栏事件
#[derive(Clone, Debug)]
pub enum SftpToolbarEvent {
    GoBack,
    GoForward,
    GoUp,
    GoHome,
    Refresh,
    NewFolder,
    ToggleHidden,
    Upload,
    Download,
    NavigateTo(String),
}

/// 渲染工具栏按钮
fn toolbar_button<F>(
    id: impl Into<ElementId>,
    icon_path: &'static str,
    enabled: bool,
    on_click: Option<F>,
    cx: &App,
) -> impl IntoElement
where
    F: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    let icon_color = if enabled {
        cx.theme().foreground
    } else {
        cx.theme().muted_foreground.opacity(0.5)
    };
    let hover_bg = cx.theme().list_active;

    let mut el = div()
        .id(id)
        .size(px(BUTTON_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .child(
            svg()
                .path(icon_path)
                .size(px(ICON_SIZE))
                .text_color(icon_color),
        );

    if enabled {
        el = el.cursor_pointer().hover(|s| s.bg(hover_bg));
        if let Some(handler) = on_click {
            el = el.on_mouse_down(MouseButton::Left, handler);
        }
    }

    el
}

/// 渲染 SFTP 工具栏
pub fn render_sftp_toolbar<F>(state: Option<&SftpState>, on_event: F, cx: &App) -> impl IntoElement
where
    F: Fn(SftpToolbarEvent, &mut App) + Clone + 'static,
{
    let bg_color = crate::theme::sidebar_color(cx);
    let border_color = cx.theme().border;
    let input_bg = cx.theme().background;
    let _muted_foreground = cx.theme().muted_foreground;

    // 获取状态信息
    let (can_back, can_forward, can_up, current_path, show_hidden) = match state {
        Some(s) => (
            s.can_go_back(),
            s.can_go_forward(),
            s.can_go_up(),
            s.current_path.as_str(),
            s.show_hidden,
        ),
        None => (false, false, false, "/", false),
    };

    // === 导航按钮组 ===
    let on_back = on_event.clone();
    let on_forward = on_event.clone();
    let on_up = on_event.clone();
    let on_home = on_event.clone();

    let nav_buttons = div()
        .flex()
        .items_center()
        .gap_0p5()
        .flex_shrink_0()
        .child(toolbar_button(
            "sftp-btn-back",
            icons::ARROW_LEFT,
            can_back,
            Some(move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                on_back(SftpToolbarEvent::GoBack, cx);
            }),
            cx,
        ))
        .child(toolbar_button(
            "sftp-btn-forward",
            icons::ARROW_RIGHT,
            can_forward,
            Some(move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                on_forward(SftpToolbarEvent::GoForward, cx);
            }),
            cx,
        ))
        .child(toolbar_button(
            "sftp-btn-up",
            icons::ARROW_UP,
            can_up,
            Some(move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                on_up(SftpToolbarEvent::GoUp, cx);
            }),
            cx,
        ))
        .child(toolbar_button(
            "sftp-btn-home",
            icons::HOME,
            true,
            Some(move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                on_home(SftpToolbarEvent::GoHome, cx);
            }),
            cx,
        ));

    // === 地址栏（面包屑导航） ===
    let path_segments = parse_path_segments(current_path);
    let mut breadcrumb = Breadcrumb::new().text_xs();

    for (i, (name, full_path)) in path_segments.iter().enumerate() {
        let is_last = i == path_segments.len() - 1;
        let path = full_path.clone();
        let on_nav = on_event.clone();

        let item = BreadcrumbItem::new(name.clone());
        let item = if is_last {
            // 最后一个元素禁用点击
            item.disabled(true)
        } else {
            // 其他元素可点击导航
            item.on_click(move |_, _, cx| {
                on_nav(SftpToolbarEvent::NavigateTo(path.clone()), cx);
            })
        };

        breadcrumb = breadcrumb.child(item);
    }

    let path_bar = div()
        .flex_1()
        .h(px(22.))
        .mx_2()
        .px_2()
        .bg(input_bg)
        .border_1()
        .border_color(border_color)
        .rounded(px(4.))
        .flex()
        .items_center()
        .overflow_hidden()
        .child(breadcrumb);

    // === 操作按钮组 ===
    let hidden_icon = if show_hidden {
        icons::EYE_OFF
    } else {
        icons::EYE
    };

    let on_refresh = on_event.clone();
    let on_new_folder = on_event.clone();
    let on_toggle_hidden = on_event.clone();
    let on_upload = on_event.clone();
    let on_download = on_event.clone();

    let action_buttons = div()
        .flex()
        .items_center()
        .gap_0p5()
        .flex_shrink_0()
        .child(toolbar_button(
            "sftp-btn-refresh",
            icons::REFRESH,
            true,
            Some(move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                on_refresh(SftpToolbarEvent::Refresh, cx);
            }),
            cx,
        ))
        .child(toolbar_button(
            "sftp-btn-new-folder",
            icons::FOLDER_PLUS,
            true,
            Some(move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                on_new_folder(SftpToolbarEvent::NewFolder, cx);
            }),
            cx,
        ))
        .child(toolbar_button(
            "sftp-btn-toggle-hidden",
            hidden_icon,
            true,
            Some(move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                on_toggle_hidden(SftpToolbarEvent::ToggleHidden, cx);
            }),
            cx,
        ))
        .child(div().w(px(1.)).h(px(16.)).mx_1().bg(border_color))
        .child(toolbar_button(
            "sftp-btn-upload",
            icons::UPLOAD,
            true,
            Some(move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                on_upload(SftpToolbarEvent::Upload, cx);
            }),
            cx,
        ))
        .child(toolbar_button(
            "sftp-btn-download",
            icons::DOWNLOAD,
            true,
            Some(move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                on_download(SftpToolbarEvent::Download, cx);
            }),
            cx,
        ));

    // === 工具栏布局 ===
    div()
        .w_full()
        .h(px(TOOLBAR_HEIGHT))
        .flex_shrink_0()
        .bg(bg_color)
        .border_b_1()
        .border_color(border_color)
        .flex()
        .items_center()
        .px_1()
        .gap_1()
        .child(nav_buttons)
        .child(path_bar)
        .child(action_buttons)
}

/// 解析路径为面包屑段
/// 例如："/home/wuyun" -> [("/", "/"), ("home", "/home"), ("wuyun", "/home/wuyun")]
fn parse_path_segments(path: &str) -> Vec<(String, String)> {
    let mut segments = Vec::new();

    if path.is_empty() || !path.starts_with('/') {
        return segments;
    }

    // 根目录
    segments.push(("/".to_string(), "/".to_string()));

    // 其他路径段
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut current_path = String::new();

    for part in parts {
        current_path.push('/');
        current_path.push_str(part);
        segments.push((part.to_string(), current_path.clone()));
    }

    segments
}
