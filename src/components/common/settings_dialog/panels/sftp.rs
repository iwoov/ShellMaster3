// SFTP设置面板

use gpui::prelude::*;
use gpui::*;

use crate::i18n;

use super::super::helpers::{render_number_row, render_section_title, render_switch_row};
use super::super::SettingsDialogState;

/// 渲染SFTP设置面板
pub fn render_sftp_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let sftp = &state_read.settings.sftp;
    let lang = &state_read.settings.theme.language;

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 文件显示
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.sftp.file_display"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sftp-show-hidden",
                            i18n::t(lang, "settings.sftp.show_hidden"),
                            sftp.show_hidden_files,
                            state.clone(),
                            |s, v| s.settings.sftp.show_hidden_files = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sftp-folders-first",
                            i18n::t(lang, "settings.sftp.folders_first"),
                            sftp.folders_first,
                            state.clone(),
                            |s, v| s.settings.sftp.folders_first = v,
                            cx,
                        )),
                ),
        )
        // 传输设置
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.sftp.transfer"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(
                            state
                                .read(cx)
                                .concurrent_transfers_input
                                .as_ref()
                                .map(|input| {
                                    render_number_row(
                                        i18n::t(lang, "settings.sftp.concurrent"),
                                        input,
                                        cx,
                                    )
                                }),
                        )
                        .child(render_switch_row(
                            "sftp-preserve-timestamps",
                            i18n::t(lang, "settings.sftp.preserve_time"),
                            sftp.preserve_timestamps,
                            state.clone(),
                            |s, v| s.settings.sftp.preserve_timestamps = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sftp-resume-transfers",
                            i18n::t(lang, "settings.sftp.resume"),
                            sftp.resume_transfers,
                            state.clone(),
                            |s, v| s.settings.sftp.resume_transfers = v,
                            cx,
                        )),
                ),
        )
        // 编辑器
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.sftp.editor"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_switch_row(
                            "sftp-builtin-editor",
                            i18n::t(lang, "settings.sftp.builtin_editor"),
                            sftp.use_builtin_editor,
                            state.clone(),
                            |s, v| s.settings.sftp.use_builtin_editor = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sftp-syntax-highlight",
                            i18n::t(lang, "settings.sftp.syntax_highlight"),
                            sftp.syntax_highlighting,
                            state.clone(),
                            |s, v| s.settings.sftp.syntax_highlighting = v,
                            cx,
                        )),
                ),
        )
}
