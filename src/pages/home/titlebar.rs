// 标题栏组件

use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::state::{SessionState, SessionStatus};

/// 渲染 Home 按钮区域（独立于 Sidebar，宽度与 Sidebar 相同）
pub fn render_home_button(session_state: Entity<SessionState>, cx: &App) -> impl IntoElement {
    let bg = crate::theme::sidebar_color(cx);
    let border = cx.theme().border;
    let hover_bg = cx.theme().muted;
    let icon_color = cx.theme().muted_foreground;
    let session_state_for_click = session_state.clone();

    div()
        .w(px(220.)) // 与 sidebar 宽度相同
        .flex_shrink_0() // 防止被压缩
        .h(px(44.)) // 标题栏高度
        .bg(bg)
        .border_r_1()
        .border_b_1()
        .border_color(border)
        .flex()
        .items_center()
        .justify_end() // 右对齐，让 Home 按钮在红绿灯右边
        .px_2()
        .child(
            div()
                .id("home-btn")
                .w_9()
                .h_9()
                .rounded_md()
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .flex()
                .items_center()
                .justify_center()
                .on_click(move |_, _, cx| {
                    session_state_for_click.update(cx, |state, _| {
                        state.show_home = true;
                    });
                })
                .child(render_icon(icons::HOME, icon_color.into())),
        )
}

/// 渲染主页标题栏（Home 页面，无内容）
pub fn render_titlebar(cx: &App) -> impl IntoElement {
    let bg = crate::theme::titlebar_color(cx);
    let border = cx.theme().title_bar_border;

    div()
        .h(px(44.)) // 与 Home 按钮区域高度相同
        .w_full()
        .bg(bg)
        .border_b_1()
        .border_color(border)
}

/// 渲染会话标题栏（带标签页）
pub fn render_session_titlebar(session_state: Entity<SessionState>, cx: &App) -> impl IntoElement {
    let bg = crate::theme::titlebar_color(cx);
    let border = cx.theme().title_bar_border;
    let primary = cx.theme().primary;
    let foreground = cx.theme().foreground;
    let muted_foreground = cx.theme().muted_foreground;
    let secondary = cx.theme().secondary;
    let secondary_hover = cx.theme().secondary_hover;

    let state = session_state.read(cx);
    let tabs = state.tabs.clone();
    let active_tab_id = state.active_tab_id.clone();
    let sidebar_collapsed = state.sidebar_collapsed;

    // 折叠按钮图标
    let toggle_icon = if sidebar_collapsed {
        icons::PANEL_RIGHT_OPEN
    } else {
        icons::PANEL_RIGHT_CLOSE
    };
    let session_state_for_toggle = session_state.clone();

    div()
        .h(px(44.)) // 与 Home 按钮区域高度相同
        .w_full()
        .bg(bg)
        .border_b_1()
        .border_color(border)
        .flex()
        .items_center()
        .px_4()
        // 标签页列表
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .children(tabs.into_iter().map({
                    let active_id = active_tab_id.clone();
                    let session_state = session_state.clone();
                    move |tab| {
                        let is_active = active_id.as_ref() == Some(&tab.id);
                        let tab_id = tab.id.clone();
                        let tab_id_for_click = tab_id.clone();
                        let tab_id_for_close = tab_id.clone();
                        let session_state_for_click = session_state.clone();
                        let session_state_for_close = session_state.clone();

                        // 标签状态图标
                        let status_icon = match &tab.status {
                            SessionStatus::Connecting => Some((icons::LOADER, primary)),
                            SessionStatus::Connected => Some((icons::CHECK, primary)),
                            SessionStatus::Error(_) => Some((icons::X, Hsla::from(rgb(0xef4444)))),
                            SessionStatus::Disconnected => {
                                Some((icons::X, Hsla::from(rgb(0xef4444))))
                            }
                        };

                        div()
                            .id(SharedString::from(format!("tab-{}", tab_id)))
                            .h(px(32.))
                            .px_3()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(if is_active { secondary } else { bg })
                            .hover(move |s| {
                                s.bg(if is_active {
                                    secondary
                                } else {
                                    secondary_hover
                                })
                            })
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
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(render_icon(icon, color.into()))
                            }))
                            // 标签名
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(if is_active {
                                        foreground
                                    } else {
                                        muted_foreground
                                    })
                                    .max_w(px(150.))
                                    .overflow_hidden()
                                    .child(tab.server_label.clone()),
                            )
                            // 关闭按钮
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "tab-close-{}",
                                        tab_id_for_close.clone()
                                    )))
                                    .w_4()
                                    .h_4()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(secondary_hover))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                        cx.stop_propagation();
                                        session_state_for_close.update(cx, |state, _| {
                                            state.close_tab(&tab_id_for_close);
                                        });
                                        // 主动关闭 SSH 连接
                                        crate::ssh::SshManager::global()
                                            .close_session(&tab_id_for_close);
                                    })
                                    .child(render_icon(icons::X, muted_foreground.into())),
                            )
                    }
                })),
        )
        // 占位，将折叠按钮推到右侧
        .child(div().flex_1())
        // Sidebar 折叠/展开按钮
        .child(
            div()
                .id("sidebar-toggle-btn")
                .w_8()
                .h_8()
                .rounded_md()
                .cursor_pointer()
                .hover(move |s| s.bg(secondary_hover))
                .flex()
                .items_center()
                .justify_center()
                .on_click(move |_, _, cx| {
                    session_state_for_toggle.update(cx, |state, _| {
                        state.toggle_sidebar();
                    });
                })
                .child(render_icon(toggle_icon, muted_foreground.into())),
        )
}
