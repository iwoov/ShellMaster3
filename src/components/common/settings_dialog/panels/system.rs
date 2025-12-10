// 系统配置面板

use gpui::prelude::*;
use gpui::*;

use crate::i18n;

use super::super::helpers::{render_number_row, render_section_title, render_switch_row};
use super::super::SettingsDialogState;

/// 渲染系统配置面板
pub fn render_system_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let system = &state_read.settings.system;
    let lang = &state_read.settings.theme.language;

    // 获取输入状态
    let log_retention_input = state_read.log_retention_input.clone();

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 启动
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.system.startup"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sys-launch-login",
                            i18n::t(lang, "settings.system.auto_start"),
                            system.launch_at_login,
                            state.clone(),
                            |s, v| s.settings.system.launch_at_login = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sys-start-minimized",
                            i18n::t(lang, "settings.system.start_minimized"),
                            system.start_minimized,
                            state.clone(),
                            |s, v| s.settings.system.start_minimized = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sys-check-updates",
                            i18n::t(lang, "settings.system.check_updates"),
                            system.check_updates,
                            state.clone(),
                            |s, v| s.settings.system.check_updates = v,
                            cx,
                        )),
                ),
        )
        // 窗口
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.system.window"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sys-close-tray",
                            i18n::t(lang, "settings.system.close_to_tray"),
                            system.close_to_tray,
                            state.clone(),
                            |s, v| s.settings.system.close_to_tray = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sys-show-tray",
                            i18n::t(lang, "settings.system.show_tray_icon"),
                            system.show_tray_icon,
                            state.clone(),
                            |s, v| s.settings.system.show_tray_icon = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sys-single-instance",
                            i18n::t(lang, "settings.system.single_instance"),
                            system.single_instance,
                            state.clone(),
                            |s, v| s.settings.system.single_instance = v,
                            cx,
                        )),
                ),
        )
        // 通知
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.system.notification"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sys-notify-disconnect",
                            i18n::t(lang, "settings.system.notify_disconnect"),
                            system.notify_on_disconnect,
                            state.clone(),
                            |s, v| s.settings.system.notify_on_disconnect = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sys-notify-transfer",
                            i18n::t(lang, "settings.system.notify_transfer"),
                            system.notify_on_transfer,
                            state.clone(),
                            |s, v| s.settings.system.notify_on_transfer = v,
                            cx,
                        )),
                ),
        )
        // 日志
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.system.logging"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(render_switch_row(
                            "sys-logging",
                            i18n::t(lang, "settings.system.logging_enabled"),
                            system.logging_enabled,
                            state.clone(),
                            |s, v| s.settings.system.logging_enabled = v,
                            cx,
                        ))
                        .children(log_retention_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.system.log_retention"),
                                input,
                                cx,
                            )
                        })),
                ),
        )
}
