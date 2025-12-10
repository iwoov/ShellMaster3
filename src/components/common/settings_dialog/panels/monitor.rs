// 监控设置面板

use gpui::prelude::*;
use gpui::*;

use crate::i18n;

use super::super::helpers::{render_number_row, render_section_title, render_switch_row};
use super::super::SettingsDialogState;

/// 渲染监控设置面板
pub fn render_monitor_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let monitor = &state_read.settings.monitor;
    let lang = &state_read.settings.theme.language;

    // 获取输入状态
    let history_retention_input = state_read.history_retention_input.clone();
    let cpu_threshold_input = state_read.cpu_threshold_input.clone();
    let memory_threshold_input = state_read.memory_threshold_input.clone();
    let disk_threshold_input = state_read.disk_threshold_input.clone();

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 数据采集
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.monitor.data_collection"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(history_retention_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.monitor.history_retention"),
                                input,
                                cx,
                            )
                        }))
                        .child(render_switch_row(
                            "monitor-auto-deploy",
                            i18n::t(lang, "settings.monitor.auto_deploy"),
                            monitor.auto_deploy_agent,
                            state.clone(),
                            |s, v| s.settings.monitor.auto_deploy_agent = v,
                            cx,
                        )),
                ),
        )
        // 显示项目
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.monitor.display_items"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "monitor-show-cpu",
                            i18n::t(lang, "settings.monitor.cpu"),
                            monitor.show_cpu,
                            state.clone(),
                            |s, v| s.settings.monitor.show_cpu = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "monitor-show-memory",
                            i18n::t(lang, "settings.monitor.memory"),
                            monitor.show_memory,
                            state.clone(),
                            |s, v| s.settings.monitor.show_memory = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "monitor-show-disk",
                            i18n::t(lang, "settings.monitor.disk"),
                            monitor.show_disk,
                            state.clone(),
                            |s, v| s.settings.monitor.show_disk = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "monitor-show-network",
                            i18n::t(lang, "settings.monitor.network"),
                            monitor.show_network,
                            state.clone(),
                            |s, v| s.settings.monitor.show_network = v,
                            cx,
                        )),
                ),
        )
        // 告警
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.monitor.alerts"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(cpu_threshold_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.monitor.cpu_threshold"),
                                input,
                                cx,
                            )
                        }))
                        .children(memory_threshold_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.monitor.memory_threshold"),
                                input,
                                cx,
                            )
                        }))
                        .children(disk_threshold_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.monitor.disk_threshold"),
                                input,
                                cx,
                            )
                        })),
                ),
        )
}
