// 连接设置面板

use gpui::prelude::*;
use gpui::*;

use crate::i18n;

use super::super::helpers::{render_number_row, render_section_title, render_switch_row};
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
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
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
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
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
}
