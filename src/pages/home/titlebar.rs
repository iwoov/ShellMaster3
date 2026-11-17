// 标题栏组件

use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::state::{SessionState, SessionStatus};

/// 渲染品牌区，并作为返回主页的入口。
pub fn render_home_button(session_state: Entity<SessionState>, cx: &App) -> impl IntoElement {
    let bg = crate::theme::titlebar_color(cx);
    let border = cx.theme().title_bar_border;
    let session_state_for_click = session_state.clone();

    // macOS: 红绿灯在左侧，Home 按钮需要在红绿灯右边留出空间
    // Windows: Home 按钮直接在最左侧
    let left_padding = if cfg!(target_os = "macos") {
        px(76.)
    } else {
        px(12.)
    };

    div()
        .w(px(220.))
        .h(px(48.))
        .bg(bg)
        .border_b_1()
        .border_r_1()
        .border_color(border)
        .flex()
        .flex_shrink_0()
        .items_center()
        .pl(left_padding)
        .pr_2()
        .child(
            div()
                .id("home-btn")
                .h_9()
                .px_2()
                .rounded_lg()
                .cursor_pointer()
                .hover(|s| s.bg(rgba(0x80808024)))
                .flex()
                .items_center()
                .gap_2()
                .on_click(move |_, _, cx| {
                    session_state_for_click.update(cx, |state, _| {
                        state.show_home = true;
                    });
                })
                .child(render_icon(icons::HOME, cx.theme().primary.into()))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().foreground)
                        .child("ShellMaster"),
                ),
        )
}

/// 渲染主页标题栏（Home 页面，无内容）
use crate::components::common::window_controls::render_windows_controls;

// ...

pub fn render_titlebar(cx: &App) -> impl IntoElement {
    let bg = crate::theme::titlebar_color(cx);
    let border = cx.theme().title_bar_border;

    div()
        .h(px(48.))
        .w_full()
        .bg(bg)
        .border_b_1()
        .border_color(border)
        .flex() // Added flex
        .items_center() // Added items_center
        .child(
            div()
                .id("titlebar-drag-area")
                .flex_1()
                .h_full()
                .px_5()
                .window_control_area(WindowControlArea::Drag)
                .flex()
                .items_center()
                .gap_2()
                .child(render_icon(
                    icons::GLOBE,
                    cx.theme().muted_foreground.into(),
                ))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().muted_foreground)
                        .child("REMOTE WORKSPACE"),
                ),
        )
        .child(render_windows_controls(cx))
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

    div()
        .h(px(48.))
        .w_full()
        .bg(bg)
        .border_b_1()
        .border_color(border)
        .flex()
        .items_center()
        .pl_4() // 只添加左侧 padding，右侧让 window controls 靠边
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
                                Some((icons::LOADER, Hsla::from(rgb(0xf59e0b))))
                                // 橙色加载图标
                            }
                            SessionStatus::Reconnecting { .. } => {
                                Some((icons::LOADER, Hsla::from(rgb(0xf59e0b))))
                                // 橙色加载图标
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
                                    .flex_shrink_0()
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
                                    .min_w_0()
                                    .max_w(px(150.))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
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
                                    .child(render_icon(icons::X, muted_foreground.into())),
                            )
                    }
                })),
        )
        // 占位，将折叠按钮推到右侧 (with drag support for Windows)
        .child(
            div()
                .id("session-titlebar-drag-area")
                .flex_1()
                .h_full()
                .window_control_area(WindowControlArea::Drag),
        )
        .child(render_windows_controls(cx)) // Add window controls
}
