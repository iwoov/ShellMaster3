// 连接设置面板

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::i18n;
use crate::models::settings::GlobalProxyType;

use super::super::helpers::{
    render_card, render_hint, render_input_row, render_number_row, render_section_title,
    render_select_row, render_switch_row,
};
use super::super::SettingsDialogState;

/// 渲染连接设置面板
pub fn render_connection_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let conn = &state_read.settings.connection;
    let lang = &state_read.settings.theme.language;

    // 获取输入状态
    let default_port_input = state_read.default_port_input.clone();
    let connection_timeout_input = state_read.connection_timeout_input.clone();
    let keepalive_interval_input = state_read.keepalive_interval_input.clone();
    let reconnect_attempts_input = state_read.reconnect_attempts_input.clone();
    let reconnect_interval_input = state_read.reconnect_interval_input.clone();
    let proxy_host_input = state_read.global_proxy_host_input.clone();
    let proxy_port_input = state_read.global_proxy_port_input.clone();
    let proxy_username_input = state_read.global_proxy_username_input.clone();
    let proxy_password_input = state_read.global_proxy_password_input.clone();

    let proxy_type = conn.global_proxy_type.clone();
    let proxy_enabled = proxy_type != GlobalProxyType::None;
    // SOCKS4 没有密码
    let proxy_has_password = matches!(proxy_type, GlobalProxyType::Http | GlobalProxyType::Socks5);

    div()
        .flex()
        .flex_col()
        .gap_6()
        // SSH 设置
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.connection.ssh"),
                    cx,
                ))
                .child(
                    render_card(cx)
                        .children(default_port_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.connection.default_port"),
                                input,
                                cx,
                            )
                        }))
                        .children(connection_timeout_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.connection.timeout"),
                                input,
                                cx,
                            )
                        }))
                        .children(keepalive_interval_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.connection.keepalive"),
                                input,
                                cx,
                            )
                        }))
                        .child(render_switch_row(
                            "conn-compression",
                            i18n::t(lang, "settings.connection.compression"),
                            conn.compression,
                            state.clone(),
                            |s, v| s.settings.connection.compression = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "conn-strict-host-key",
                            i18n::t(lang, "settings.connection.strict_host_key"),
                            conn.strict_host_key_checking,
                            state.clone(),
                            |s, v| s.settings.connection.strict_host_key_checking = v,
                            cx,
                        ))
                        .child(render_hint(
                            i18n::t(lang, "settings.connection.strict_host_key_hint"),
                            cx,
                        )),
                ),
        )
        // 自动重连
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.connection.reconnect"),
                    cx,
                ))
                .child(
                    render_card(cx)
                        .child(render_switch_row(
                            "conn-auto-reconnect",
                            i18n::t(lang, "settings.connection.reconnect_enabled"),
                            conn.auto_reconnect,
                            state.clone(),
                            |s, v| s.settings.connection.auto_reconnect = v,
                            cx,
                        ))
                        .children(reconnect_attempts_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.connection.reconnect_attempts"),
                                input,
                                cx,
                            )
                        }))
                        .children(reconnect_interval_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.connection.reconnect_interval"),
                                input,
                                cx,
                            )
                        })),
                ),
        )
        // 全局代理
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.connection.global_proxy"),
                    cx,
                ))
                .child(
                    render_card(cx)
                        .child(render_select_row(
                            "conn-global-proxy-type",
                            i18n::t(lang, "settings.connection.global_proxy_type"),
                            &proxy_type,
                            vec![
                                (
                                    GlobalProxyType::None,
                                    i18n::t(lang, "settings.connection.global_proxy_none"),
                                ),
                                (GlobalProxyType::Http, "HTTP"),
                                (GlobalProxyType::Socks4, "SOCKS4"),
                                (GlobalProxyType::Socks5, "SOCKS5"),
                            ],
                            state.clone(),
                            |s, v| s.settings.connection.global_proxy_type = v,
                            cx,
                        ))
                        .when(proxy_enabled, |section| {
                            section
                                .children(proxy_host_input.as_ref().map(|input| {
                                    render_input_row(
                                        i18n::t(lang, "settings.connection.global_proxy_host"),
                                        input,
                                        cx,
                                    )
                                }))
                                .children(proxy_port_input.as_ref().map(|input| {
                                    render_input_row(
                                        i18n::t(lang, "settings.connection.global_proxy_port"),
                                        input,
                                        cx,
                                    )
                                }))
                                .children(proxy_username_input.as_ref().map(|input| {
                                    render_input_row(
                                        i18n::t(lang, "settings.connection.global_proxy_username"),
                                        input,
                                        cx,
                                    )
                                }))
                                .when(proxy_has_password, |section| {
                                    section.children(proxy_password_input.as_ref().map(|input| {
                                        render_input_row(
                                            i18n::t(
                                                lang,
                                                "settings.connection.global_proxy_password",
                                            ),
                                            input,
                                            cx,
                                        )
                                    }))
                                })
                        })
                        .child(render_hint(
                            i18n::t(lang, "settings.connection.global_proxy_hint"),
                            cx,
                        )),
                ),
        )
}
