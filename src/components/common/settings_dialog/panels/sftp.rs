// SFTP设置面板

use crate::components::common::button::{text_button, ButtonKind};
use crate::components::common::input::text_field;
use gpui::prelude::*;
use gpui::*;

use crate::i18n;

use super::super::helpers::{
    render_card, render_input_row, render_number_row, render_row, render_section_title,
    render_switch_row, CONTROL_WIDTH,
};
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
                    render_card(cx)
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
                    render_card(cx)
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
                        // 默认下载路径
                        .children(
                            state
                                .read(cx)
                                .local_default_path_input
                                .as_ref()
                                .map(|input| {
                                    render_browse_row(
                                        "browse-path",
                                        i18n::t(lang, "settings.sftp.default_download_path"),
                                        i18n::t(lang, "settings.sftp.pick_download_dir"),
                                        true,
                                        input,
                                        state.clone(),
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
        // 编辑器 - 基本设置
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
                    render_card(cx)
                        .child(render_switch_row(
                            "sftp-builtin-editor",
                            i18n::t(lang, "settings.sftp.builtin_editor"),
                            sftp.use_builtin_editor,
                            state.clone(),
                            |s, v| s.settings.sftp.use_builtin_editor = v,
                            cx,
                        ))
                        // 外置编辑器路径
                        .children(
                            state
                                .read(cx)
                                .external_editor_path_input
                                .as_ref()
                                .map(|input| {
                                    render_browse_row(
                                        "browse-file",
                                        i18n::t(lang, "settings.sftp.external_editor_path"),
                                        i18n::t(lang, "settings.sftp.pick_editor"),
                                        false,
                                        input,
                                        state.clone(),
                                        cx,
                                    )
                                }),
                        )
                        // 最大可编辑文件大小
                        .children(
                            state
                                .read(cx)
                                .max_edit_file_size_input
                                .as_ref()
                                .map(|input| {
                                    render_number_row(
                                        i18n::t(lang, "settings.sftp.max_edit_file_size"),
                                        input,
                                        cx,
                                    )
                                }),
                        ),
                ),
        )
        // 编辑器 - 外观设置
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.sftp.editor_appearance"),
                    cx,
                ))
                .child(
                    render_card(cx)
                        // 编辑器字体
                        .children(
                            state
                                .read(cx)
                                .editor_font_family_input
                                .as_ref()
                                .map(|input| {
                                    render_input_row(
                                        i18n::t(lang, "settings.sftp.editor_font_family"),
                                        input,
                                        cx,
                                    )
                                }),
                        )
                        // 编辑器字号
                        .children(state.read(cx).editor_font_size_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.sftp.editor_font_size"),
                                input,
                                cx,
                            )
                        }))
                        // 行高
                        .children(
                            state
                                .read(cx)
                                .editor_line_height_input
                                .as_ref()
                                .map(|input| {
                                    render_number_row(
                                        i18n::t(lang, "settings.sftp.editor_line_height"),
                                        input,
                                        cx,
                                    )
                                }),
                        )
                        // 行号栏宽度
                        .children(
                            state
                                .read(cx)
                                .editor_gutter_width_input
                                .as_ref()
                                .map(|input| {
                                    render_number_row(
                                        i18n::t(lang, "settings.sftp.editor_gutter_width"),
                                        input,
                                        cx,
                                    )
                                }),
                        )
                        // 行号栏内边距
                        .children(state.read(cx).editor_gutter_padding_input.as_ref().map(
                            |input| {
                                render_number_row(
                                    i18n::t(lang, "settings.sftp.editor_gutter_padding"),
                                    input,
                                    cx,
                                )
                            },
                        )),
                ),
        )
}

/// 渲染路径选择行：输入框 + 「浏览」按钮。`pick_folder` 为 true 时选择文件夹，否则选择文件。
fn render_browse_row(
    id: &'static str,
    label: &'static str,
    dialog_title: &'static str,
    pick_folder: bool,
    input: &Entity<gpui_component::input::InputState>,
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let browse_label = i18n::t(
        &state.read(cx).settings.theme.language,
        "settings.sftp.browse",
    );
    let input_clone = input.clone();

    render_row(
        label,
        None,
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(div().w(CONTROL_WIDTH).child(text_field(input)))
            .child(
                text_button(id, ButtonKind::Secondary, browse_label, &p).on_click(
                    move |_, _, cx| {
                        let input_for_update = input_clone.clone();
                        let state_for_update = state.clone();

                        // 使用系统的异步文件对话框选择路径
                        cx.spawn(async move |cx| {
                            let picker = rfd::AsyncFileDialog::new().set_title(dialog_title);
                            let picked = if pick_folder {
                                picker.pick_folder().await
                            } else {
                                picker.pick_file().await
                            };
                            let Some(picked) = picked else {
                                return;
                            };
                            let path = picked.path().to_string_lossy().to_string();
                            let _ = cx.update(|cx| {
                                if let Some(window) = cx.active_window() {
                                    let _ = cx.update_window(window, |_, window, cx| {
                                        input_for_update.update(cx, |input, cx| {
                                            input.set_value(path.clone(), window, cx);
                                        });
                                        state_for_update.update(cx, |s, cx| {
                                            s.mark_changed();
                                            cx.notify();
                                        });
                                    });
                                }
                            });
                        })
                        .detach();
                    },
                ),
            ),
        cx,
    )
}
