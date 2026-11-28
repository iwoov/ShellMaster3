// 窗口顶部条：品牌区、会话标签页和窗口控制按钮

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::widgets::{TextFitter, TOPBAR_HEIGHT};
use crate::components::common::button::icon_button_sized;
use crate::components::common::icon::render_icon;
use crate::components::common::panel::{spinner, status_dot};
use crate::components::common::window_controls::render_windows_controls;
use crate::constants::icons;
use crate::state::{SessionState, SessionStatus};
use crate::theme::Palette;

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
///
/// 宽度与会话视图的左列（监控、SFTP 目录树）一致，右边线和下面的分栏线对齐；品牌标识右侧的空白可以拖动窗口。
pub fn render_home_button(
    width: Pixels,
    session_state: Entity<SessionState>,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let bg = crate::theme::titlebar_color(cx);
    let border = cx.theme().title_bar_border;
    let hover_bg = p.hover;

    div()
        .w(width)
        .h(TOPBAR_HEIGHT)
        .bg(bg)
        .border_b_1()
        .border_r_1()
        .border_color(border)
        .flex()
        .flex_shrink_0()
        .items_center()
        .pl(brand_left_padding())
        .child(
            render_brand_label(cx)
                .id("home-btn")
                .flex_none()
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .on_click(move |_, _, cx| {
                    session_state.update(cx, |state, cx| {
                        state.show_home = true;
                        cx.notify();
                    });
                }),
        )
        .child(
            div()
                .id("session-brand-drag-area")
                .flex_1()
                .h_full()
                .window_control_area(WindowControlArea::Drag),
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
/// 标签的高度和圆角（与主页按钮一致）
const TAB_HEIGHT: Pixels = px(30.);
const TAB_RADIUS: Pixels = px(7.);
/// 标签内部用于联动悬停样式（显示关闭钮）的分组名
const TAB_GROUP: &str = "session-tab";

/// 标签页的两种外观
#[derive(Clone, Copy, PartialEq, Eq)]
enum TabStyle {
    /// 会话视图：标题栏上的标签，当前会话高亮
    Session,
    /// 主页：没有标题栏底色，所有标签都带浅底色，不区分当前会话
    Home,
}

/// 标签左侧的状态指示：连接中 / 重连中是转动的加载图标，其余是小圆点
fn render_tab_status(tab_id: &str, status: &SessionStatus, p: &Palette) -> AnyElement {
    let indicator = match status {
        SessionStatus::Connecting => spinner(
            SharedString::from(format!("tab-spinner-{tab_id}")),
            12.,
            p.primary,
        )
        .into_any_element(),
        SessionStatus::Reconnecting { .. } => spinner(
            SharedString::from(format!("tab-spinner-{tab_id}")),
            12.,
            p.warning,
        )
        .into_any_element(),
        SessionStatus::Connected => status_dot(p.success, 7.).into_any_element(),
        SessionStatus::Disconnected => status_dot(p.warning, 7.).into_any_element(),
        SessionStatus::Error(_) => status_dot(p.danger, 7.).into_any_element(),
    };
    div()
        .flex_none()
        .size(px(14.))
        .flex()
        .items_center()
        .justify_center()
        .child(indicator)
        .into_any_element()
}

/// 渲染会话标签页列表
fn render_tabs(
    session_state: &Entity<SessionState>,
    style: TabStyle,
    fit: &TextFitter,
    cx: &App,
) -> Div {
    let p = crate::theme::palette(cx);
    let lang = crate::services::storage::cached_settings()
        .theme
        .language
        .clone();
    let close_tooltip = crate::i18n::t(&lang, "session.tab.close");

    // 直接借用标签，不要克隆：SessionTab 里有监控历史、SFTP 目录缓存和 AI 对话，整体克隆会在每帧深拷贝一遍
    let state = session_state.read(cx);
    let active_tab_id = state.active_tab_id.as_ref();

    div()
        .flex()
        .items_center()
        .gap(px(4.))
        .children(state.tabs.iter().map(|tab| {
            let is_active = style == TabStyle::Session && active_tab_id == Some(&tab.id);
            let tab_id_for_click = tab.id.clone();
            let tab_id_for_close = tab.id.clone();
            let session_state_for_click = session_state.clone();
            let session_state_for_close = session_state.clone();

            let (rest_bg, hover_bg) = match style {
                TabStyle::Session if is_active => (Some(p.selected), p.selected),
                TabStyle::Session => (None, p.hover),
                TabStyle::Home => (Some(p.hover), p.hover_strong),
            };

            div()
                .id(SharedString::from(format!("tab-{}", tab.id)))
                .group(TAB_GROUP)
                .flex_none()
                .h(TAB_HEIGHT)
                .pl(px(10.))
                .pr(px(5.))
                .rounded(TAB_RADIUS)
                .cursor_pointer()
                .when_some(rest_bg, |tab, bg| tab.bg(bg))
                .hover(move |s| s.bg(hover_bg))
                .flex()
                .items_center()
                .gap(px(7.))
                .on_click(move |_, _, cx| {
                    session_state_for_click.update(cx, |state, cx| {
                        state.activate_tab(&tab_id_for_click);
                        state.show_home = false;
                        cx.notify();
                    });
                })
                .child(render_tab_status(&tab.id, &tab.status, &p))
                // 标签名：最宽 150px，放不下时末尾显示省略号，悬停显示全名
                .child(
                    fit.label_with_tooltip(
                        SharedString::from(format!("tab-label-{}", tab.id)),
                        &tab.server_label,
                        TAB_LABEL_MAX_WIDTH,
                        13.,
                        if is_active {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        },
                    )
                    .flex_none()
                    .text_color(if is_active { p.text } else { p.text_muted }),
                )
                // 关闭按钮：会话视图里当前标签一直显示，其余标签悬停时显示；主页上的标签一直显示
                .child(
                    icon_button_sized(
                        SharedString::from(format!("tab-close-{}", tab_id_for_close)),
                        icons::X,
                        px(18.),
                        12.,
                        close_tooltip,
                        &p,
                    )
                    .rounded(px(5.))
                    .when(style == TabStyle::Session && !is_active, |button| {
                        button.invisible().group_hover(TAB_GROUP, |s| s.visible())
                    })
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        // 若该会话存在未保存的编辑器文件，先弹确认框；否则立即关闭 SSH 会话
                        session_state_for_close.update(cx, |state, cx| {
                            state.request_close_tab(&tab_id_for_close, cx);
                        });
                    }),
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
