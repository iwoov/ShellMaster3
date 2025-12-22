// SFTP设置面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::input::Input;

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
                        // 默认下载路径
                        .children(
                            state
                                .read(cx)
                                .local_default_path_input
                                .as_ref()
                                .map(|input| {
                                    render_path_row(
                                        i18n::t(lang, "settings.sftp.default_download_path"),
                                        i18n::t(lang, "settings.sftp.browse"),
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

/// 渲染带浏览按钮的路径输入行
fn render_path_row(
    label: &'static str,
    browse_label: &'static str,
    input: &Entity<gpui_component::input::InputState>,
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    use gpui_component::ActiveTheme;

    let text_color = cx.theme().foreground;
    let input_clone = input.clone();

    div()
        .flex()
        .items_center()
        .justify_between()
        .py_3()
        .px_4()
        .bg(cx.theme().muted)
        .rounded_lg()
        .mb_2()
        .child(
            div()
                .w(px(150.))
                .text_sm()
                .text_color(text_color)
                .child(label),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().w(px(200.)).child(Input::new(input).appearance(true)))
                .child(
                    Button::new("browse-path")
                        .outline()
                        .child(browse_label)
                        .on_click(move |_, _, cx| {
                            let input_for_update = input_clone.clone();
                            let state_for_update = state.clone();

                            // 使用异步文件对话框选择路径
                            cx.spawn(async move |cx| {
                                let folder_picker =
                                    rfd::AsyncFileDialog::new().set_title("选择默认下载路径");

                                if let Some(folder) = folder_picker.pick_folder().await {
                                    let path = folder.path().to_string_lossy().to_string();
                                    let _ = cx.update(|cx| {
                                        if let Some(window) = cx.active_window() {
                                            let _ = cx.update_window(window, |_, window, cx| {
                                                input_for_update.update(cx, |input, cx| {
                                                    input.set_value(path.clone(), window, cx);
                                                });
                                                state_for_update.update(cx, |s, _| {
                                                    s.mark_changed();
                                                });
                                            });
                                        }
                                    });
                                }
                            })
                            .detach();
                        }),
                ),
        )
}
