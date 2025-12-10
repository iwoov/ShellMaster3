// 代理设置面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::constants::icons;
use crate::i18n;
use crate::models::server::ProxyType;
use crate::models::settings::Language;
use crate::services::storage;

use super::super::helpers::{render_form_label, render_switch};
use super::super::ServerDialogState;

/// 渲染代理设置表单
pub fn render_proxy_settings_form(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    use gpui_component::input::Input;

    // 加载当前语言
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let state_read = state.read(cx);
    let enabled = state_read.enable_proxy;
    let proxy_type = state_read.proxy_type.clone();

    let loading_text = i18n::t(&lang, "common.loading");
    let host_input = if let Some(input) = &state_read.proxy_host_input {
        Input::new(input).into_any_element()
    } else {
        div().child(loading_text).into_any_element()
    };
    let port_input = if let Some(input) = &state_read.proxy_port_input {
        Input::new(input).into_any_element()
    } else {
        div().child(loading_text).into_any_element()
    };
    let username_input = if let Some(input) = &state_read.proxy_username_input {
        Input::new(input).into_any_element()
    } else {
        div().child(loading_text).into_any_element()
    };
    let password_input = if let Some(input) = &state_read.proxy_password_input {
        Input::new(input).mask_toggle().into_any_element()
    } else {
        div().child(loading_text).into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(render_form_label(
                    i18n::t(&lang, "server_dialog.enable_proxy"),
                    icons::GLOBE,
                    cx,
                ))
                .child({
                    let state = state.clone();
                    render_switch(enabled, move |_, _, cx| {
                        state.update(cx, |s, _| {
                            s.enable_proxy = !s.enable_proxy;
                        });
                    })
                }),
        )
        .children(if enabled {
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    // 代理类型
                    .child(div().flex().flex_col().gap_2().child({
                        let toggle_bg = cx.theme().muted;
                        let selected_bg = cx.theme().popover;
                        let unselected_bg = cx.theme().muted;
                        let selected_text = cx.theme().foreground;
                        let unselected_text = cx.theme().muted_foreground;

                        div()
                            .flex()
                            .gap_1()
                            .p_1()
                            .bg(toggle_bg)
                            .rounded_md()
                            .child(render_proxy_type_button(
                                state.clone(),
                                ProxyType::Http,
                                "HTTP",
                                proxy_type == ProxyType::Http,
                                selected_bg,
                                unselected_bg,
                                selected_text,
                                unselected_text,
                            ))
                            .child(render_proxy_type_button(
                                state.clone(),
                                ProxyType::Socks5,
                                "SOCKS5",
                                proxy_type == ProxyType::Socks5,
                                selected_bg,
                                unselected_bg,
                                selected_text,
                                unselected_text,
                            ))
                    }))
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(render_form_label(
                                        i18n::t(&lang, "server_dialog.proxy_host"),
                                        icons::SERVER,
                                        cx,
                                    ))
                                    .child(host_input),
                            )
                            .child(
                                div()
                                    .w(px(100.))
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(render_form_label(
                                        i18n::t(&lang, "server_dialog.proxy_port"),
                                        icons::LINK,
                                        cx,
                                    ))
                                    .child(port_input),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(render_form_label(
                                i18n::t(&lang, "server_dialog.proxy_username"),
                                icons::USER,
                                cx,
                            ))
                            .child(username_input),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(render_form_label(
                                i18n::t(&lang, "server_dialog.proxy_password"),
                                icons::LOCK,
                                cx,
                            ))
                            .child(password_input),
                    ),
            )
        } else {
            None
        })
}

/// 渲染代理类型切换按钮
fn render_proxy_type_button(
    state: Entity<ServerDialogState>,
    proxy_type: ProxyType,
    label: &'static str,
    selected: bool,
    selected_bg: gpui::Hsla,
    unselected_bg: gpui::Hsla,
    selected_text: gpui::Hsla,
    unselected_text: gpui::Hsla,
) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .bg(if selected { selected_bg } else { unselected_bg })
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            state.update(cx, |s, _| {
                s.proxy_type = proxy_type.clone();
            });
        })
        .shadow(if selected {
            vec![BoxShadow {
                color: rgba(0x00000010).into(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
            }]
        } else {
            vec![]
        })
        .child(
            div()
                .text_sm()
                .font_weight(if selected {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if selected {
                    selected_text
                } else {
                    unselected_text
                })
                .child(label),
        )
}
