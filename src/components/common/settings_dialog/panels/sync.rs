// 数据同步面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::button::Button;

use crate::i18n;

use super::super::helpers::{render_input_row, render_section_title, render_switch_row};
use super::super::SettingsDialogState;

/// 渲染数据同步面板
pub fn render_sync_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let sync = &state_read.settings.sync;
    let lang = &state_read.settings.theme.language;

    // 获取输入状态
    let webdav_url_input = state_read.webdav_url_input.clone();
    let webdav_username_input = state_read.webdav_username_input.clone();
    let webdav_password_input = state_read.webdav_password_input.clone();
    let webdav_path_input = state_read.webdav_path_input.clone();

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 同步状态
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.sync.status"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sync-enabled",
                            i18n::t(lang, "settings.sync.enabled"),
                            sync.enabled,
                            state.clone(),
                            |s, v| s.settings.sync.enabled = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-auto",
                            i18n::t(lang, "settings.sync.auto"),
                            sync.auto_sync,
                            state.clone(),
                            |s, v| s.settings.sync.auto_sync = v,
                            cx,
                        )),
                ),
        )
        // 同步内容
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.sync.content"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sync-servers",
                            i18n::t(lang, "settings.sync.servers"),
                            sync.sync_servers,
                            state.clone(),
                            |s, v| s.settings.sync.sync_servers = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-groups",
                            i18n::t(lang, "settings.sync.groups"),
                            sync.sync_groups,
                            state.clone(),
                            |s, v| s.settings.sync.sync_groups = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-settings",
                            i18n::t(lang, "settings.sync.settings"),
                            sync.sync_settings,
                            state.clone(),
                            |s, v| s.settings.sync.sync_settings = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-keybindings",
                            i18n::t(lang, "settings.sync.keybindings"),
                            sync.sync_keybindings,
                            state.clone(),
                            |s, v| s.settings.sync.sync_keybindings = v,
                            cx,
                        )),
                ),
        )
        // WebDAV
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.sync.webdav"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(webdav_url_input.as_ref().map(|input| {
                            render_input_row(i18n::t(lang, "settings.sync.webdav_url"), input, cx)
                        }))
                        .children(webdav_username_input.as_ref().map(|input| {
                            render_input_row(
                                i18n::t(lang, "settings.sync.webdav_username"),
                                input,
                                cx,
                            )
                        }))
                        .children(webdav_password_input.as_ref().map(|input| {
                            render_input_row(
                                i18n::t(lang, "settings.sync.webdav_password"),
                                input,
                                cx,
                            )
                        }))
                        .children(webdav_path_input.as_ref().map(|input| {
                            render_input_row(i18n::t(lang, "settings.sync.webdav_path"), input, cx)
                        })),
                )
                // 测试和同步按钮
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .mt_2()
                        .child(
                            Button::new("test-webdav")
                                .outline()
                                .child(i18n::t(lang, "settings.sync.test_connection"))
                                .on_click({
                                    let _state = state.clone();
                                    move |_event, _window, _cx| {
                                        // TODO: 实现测试连接功能
                                    }
                                }),
                        )
                        .child(
                            Button::new("sync-now")
                                .child(i18n::t(lang, "settings.sync.sync_now"))
                                .on_click({
                                    let _state = state.clone();
                                    move |_event, _window, _cx| {
                                        // TODO: 实现同步功能
                                    }
                                }),
                        ),
                ),
        )
}
