// SFTP 工具栏组件
// 包含导航按钮（返回、前进、上级、主目录）+ 地址栏 + 操作按钮

use gpui::*;
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
fn toolbar_button(icon_path: &'static str, enabled: bool, cx: &App) -> impl IntoElement {
    let icon_color = if enabled {
        cx.theme().foreground
    } else {
        cx.theme().muted_foreground.opacity(0.5)
    };
    let hover_bg = cx.theme().list_active;

    let mut el = div()
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
    }

    el
}

/// 渲染 SFTP 工具栏
pub fn render_sftp_toolbar(state: Option<&SftpState>, cx: &App) -> impl IntoElement {
    let bg_color = crate::theme::sidebar_color(cx);
    let border_color = cx.theme().border;
    let input_bg = cx.theme().background;
    let muted_foreground = cx.theme().muted_foreground;

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
    let nav_buttons = div()
        .flex()
        .items_center()
        .gap_0p5()
        .flex_shrink_0()
        .child(toolbar_button(icons::ARROW_LEFT, can_back, cx))
        .child(toolbar_button(icons::ARROW_RIGHT, can_forward, cx))
        .child(toolbar_button(icons::ARROW_UP, can_up, cx))
        .child(toolbar_button(icons::HOME, true, cx));

    // === 地址栏 ===
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
        .child(
            div()
                .text_xs()
                .text_color(muted_foreground)
                .overflow_hidden()
                .text_ellipsis()
                .child(current_path.to_string()),
        );

    // === 操作按钮组 ===
    let hidden_icon = if show_hidden {
        icons::EYE_OFF
    } else {
        icons::EYE
    };

    let action_buttons = div()
        .flex()
        .items_center()
        .gap_0p5()
        .flex_shrink_0()
        .child(toolbar_button(icons::REFRESH, true, cx))
        .child(toolbar_button(icons::FOLDER_PLUS, true, cx))
        .child(toolbar_button(hidden_icon, true, cx))
        .child(div().w(px(1.)).h(px(16.)).mx_1().bg(border_color))
        .child(toolbar_button(icons::UPLOAD, true, cx))
        .child(toolbar_button(icons::DOWNLOAD, true, cx));

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
