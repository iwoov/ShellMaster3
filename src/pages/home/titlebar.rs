// 窗口顶部条：品牌区、会话标签页和窗口控制按钮

use gpui::*;
use gpui_component::ActiveTheme;

use super::widgets::{TextFitter, SIDEBAR_WIDTH, TOPBAR_HEIGHT};
use crate::components::common::icon::render_icon;
use crate::components::common::window_controls::render_windows_controls;
use crate::constants::icons;
use crate::state::{SessionState, SessionStatus};

/// 品牌区左侧的留白。
/// macOS 的红绿灯在左上角，需要给它让出位置；其他平台贴左放置。
fn brand_left_padding() -> Pixels {
    if cfg!(target_os = "macos") {
        px(76.)
    } else {
        px(12.)
    }
}

/// 品牌标识：图标 + 应用名
fn render_brand_label(cx: &App) -> Div {
    div()
        .h_9()
        .px_2()
        .rounded_lg()
        .flex()
        .items_center()
        .gap_2()
        .child(render_icon(icons::HOME, cx.theme().primary.into()))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child("ShellMaster"),
        )
}

/// 渲染会话视图左上角的品牌区，点击返回主页。
pub fn render_home_button(session_state: Entity<SessionState>, cx: &App) -> impl IntoElement {
    let bg = crate::theme::titlebar_color(cx);
    let border = cx.theme().title_bar_border;

    div()
        .w(SIDEBAR_WIDTH)
        .h(TOPBAR_HEIGHT)
        .bg(bg)
        .border_b_1()
        .border_r_1()
        .border_color(border)
        .flex()
        .flex_shrink_0()
        .items_center()
        .pl(brand_left_padding())
        .pr_2()
        .child(
            render_brand_label(cx)
                .id("home-btn")
                .cursor_pointer()
                .hover(|s| s.bg(rgba(0x80808024)))
                .on_click(move |_, _, cx| {
                    session_state.update(cx, |state, _| {
                        state.show_home = true;
                    });
                }),
        )
}

/// 渲染主页侧边栏顶部的品牌区。
///
/// 位置和尺寸与会话视图的 [`render_home_button`] 一致，切换视图时品牌标识不会移动。
/// 主页上它不需要点击，整块用来拖动窗口。
pub fn render_sidebar_brand(cx: &App) -> impl IntoElement {
    div()
        .id("sidebar-brand")
        .h(TOPBAR_HEIGHT)
        .flex_none()
        .flex()
        .items_center()
        .pl(brand_left_padding())
        .pr_2()
        .window_control_area(WindowControlArea::Drag)
        .child(render_brand_label(cx))
}

/// 标签名的最大宽度
const TAB_LABEL_MAX_WIDTH: Pixels = px(150.);

/// 标签页的两种外观
#[derive(Clone, Copy, PartialEq, Eq)]
enum TabStyle {
    /// 会话视图：标题栏上的标签，当前会话高亮
    Session,
    /// 主页：没有标题栏底色，所有标签都带浅底色，不区分当前会话
    Home,
}

/// 渲染会话标签页列表
fn render_tabs(
    session_state: &Entity<SessionState>,
    style: TabStyle,
    fit: &TextFitter,
    cx: &App,
) -> Div {
    let bar_bg = crate::theme::titlebar_color(cx);
    let p = crate::theme::palette(cx);
    let primary = cx.theme().primary;
    let foreground = cx.theme().foreground;
    let muted_foreground = cx.theme().muted_foreground;
    let secondary = cx.theme().secondary;
    let secondary_hover = cx.theme().secondary_hover;

    let state = session_state.read(cx);
    let tabs = state.tabs.clone();
    let active_tab_id = state.active_tab_id.clone();

    div()
        .flex()
        .items_center()
        .gap_1()
        .children(tabs.into_iter().map(|tab| {
            let is_active = style == TabStyle::Session && active_tab_id.as_ref() == Some(&tab.id);
            let tab_id_for_click = tab.id.clone();
            let tab_id_for_close = tab.id.clone();
            let session_state_for_click = session_state.clone();
            let session_state_for_close = session_state.clone();

            // 标签状态图标
            let status_icon = match &tab.status {
                SessionStatus::Connecting => Some((icons::LOADER, primary)),
                SessionStatus::Connected => Some((icons::CHECK, primary)),
                SessionStatus::Error(_) => Some((icons::X, Hsla::from(rgb(0xef4444)))),
                // 已断开：拔出的插头（与“重连中”的加载图标区分开）
                SessionStatus::Disconnected => Some((icons::UNPLUG, Hsla::from(rgb(0xf59e0b)))),
                // 重连中：橙色加载图标
                SessionStatus::Reconnecting { .. } => {
                    Some((icons::LOADER, Hsla::from(rgb(0xf59e0b))))
                }
            };

            let (rest_bg, hover_bg) = match style {
                TabStyle::Session if is_active => (secondary, secondary),
                TabStyle::Session => (bar_bg, secondary_hover),
                TabStyle::Home => (p.hover, p.hover_strong),
            };

            div()
                .id(SharedString::from(format!("tab-{}", tab.id)))
                .h(px(32.))
                .px_3()
                .rounded_md()
                .cursor_pointer()
                .bg(rest_bg)
                .hover(move |s| s.bg(hover_bg))
                .flex()
                .items_center()
                .gap_2()
                .on_click(move |_, _, cx| {
                    session_state_for_click.update(cx, |state, _| {
                        state.activate_tab(&tab_id_for_click);
                        state.show_home = false;
                    });
                })
                // 状态图标
                .children(status_icon.map(|(icon, color)| {
                    div()
                        .w_4()
                        .h_4()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(render_icon(icon, color))
                }))
                // 标签名：最宽 150px，放不下时末尾显示省略号
                .child(
                    fit.label(
                        &tab.server_label,
                        TAB_LABEL_MAX_WIDTH,
                        14.,
                        FontWeight::NORMAL,
                    )
                    .flex_none()
                    .text_color(if is_active {
                        foreground
                    } else {
                        muted_foreground
                    }),
                )
                // 关闭按钮
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "tab-close-{}",
                            tab_id_for_close
                        )))
                        .w_4()
                        .h_4()
                        .flex_shrink_0()
                        .rounded_sm()
                        .cursor_pointer()
                        .hover(move |s| s.bg(secondary_hover))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            // close_tab 会一并关闭 SSH 连接（含仍在进行中的连接）
                            session_state_for_close.update(cx, |state, _| {
                                state.close_tab(&tab_id_for_close);
                            });
                        })
                        .child(render_icon(icons::X, muted_foreground)),
                )
        }))
}

/// 顶部条：标签页 + 可拖动窗口的空白区域 + 非 macOS 平台的窗口控制按钮
fn topbar_with_tabs(tabs: Div, drag_area_id: &'static str, cx: &App) -> Div {
    div()
        .h(TOPBAR_HEIGHT)
        .w_full()
        .flex()
        .items_center()
        .pl_4() // 只添加左侧 padding，右侧让 window controls 靠边
        .child(tabs)
        .child(
            div()
                .id(drag_area_id)
                .flex_1()
                .h_full()
                .window_control_area(WindowControlArea::Drag),
        )
        .child(render_windows_controls(cx))
}

/// 渲染主页内容区上方的顶部条：有会话时显示标签页，其余部分用来拖动窗口
pub fn render_home_topbar(
    session_state: Entity<SessionState>,
    fit: &TextFitter,
    cx: &App,
) -> impl IntoElement {
    let has_sessions = session_state.read(cx).has_sessions();
    let tabs = if has_sessions {
        render_tabs(&session_state, TabStyle::Home, fit, cx)
    } else {
        div()
    };
    // 主页上它是纵向布局里的一行，高度不能被内容区挤压
    topbar_with_tabs(tabs, "titlebar-drag-area", cx).flex_shrink_0()
}

/// 渲染会话标题栏（带标签页）
pub fn render_session_titlebar(
    session_state: Entity<SessionState>,
    fit: &TextFitter,
    cx: &App,
) -> impl IntoElement {
    let tabs = render_tabs(&session_state, TabStyle::Session, fit, cx);
    topbar_with_tabs(tabs, "session-titlebar-drag-area", cx)
        .bg(crate::theme::titlebar_color(cx))
        .border_b_1()
        .border_color(cx.theme().title_bar_border)
}
