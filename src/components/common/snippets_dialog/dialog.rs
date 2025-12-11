// Snippets 弹窗渲染组件

use gpui::*;
use gpui_component::input::Input;
use gpui_component::ActiveTheme;

use crate::i18n;
use crate::models::settings::Language;
use crate::services::storage;

use super::state::{SnippetsDialogMode, SnippetsDialogState};

/// 渲染 Snippets 弹窗覆盖层
pub fn render_snippets_dialog_overlay(
    state: Entity<SnippetsDialogState>,
    cx: &App,
) -> impl IntoElement {
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let state_read = state.read(cx);
    let is_group_dialog = state_read.is_group_dialog();
    let dialog_title = match state_read.dialog_mode {
        SnippetsDialogMode::AddGroup => i18n::t(&lang, "snippets.add_group"),
        SnippetsDialogMode::EditGroup => i18n::t(&lang, "snippets.dialog.edit_group"),
        SnippetsDialogMode::AddCommand => i18n::t(&lang, "snippets.add_command"),
        SnippetsDialogMode::EditCommand => i18n::t(&lang, "snippets.dialog.edit_command"),
        _ => "",
    };

    // 获取输入框 Entity
    let name_input = state_read.name_input.clone();
    let command_input = state_read.command_input.clone();

    let state_cancel = state.clone();
    let state_save = state;

    let bg_color = cx.theme().popover;
    let border_color = cx.theme().border;
    let foreground = cx.theme().foreground;
    let muted_foreground = cx.theme().muted_foreground;

    div()
        .id("snippets-dialog-overlay")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(gpui::black().opacity(0.5))
        .flex()
        .items_center()
        .justify_center()
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .w(px(420.))
                .bg(bg_color)
                .rounded_lg()
                .border_1()
                .border_color(border_color)
                .p_6()
                .flex()
                .flex_col()
                .gap_4()
                // 标题
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::BOLD)
                        .text_color(foreground)
                        .child(dialog_title),
                )
                // 名称输入
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().text_sm().text_color(muted_foreground).child(
                            if is_group_dialog {
                                i18n::t(&lang, "snippets.dialog.group_name")
                            } else {
                                i18n::t(&lang, "snippets.dialog.command_name")
                            },
                        ))
                        .child(if let Some(input) = &name_input {
                            Input::new(input).into_any_element()
                        } else {
                            div()
                                .text_sm()
                                .text_color(muted_foreground)
                                .child(i18n::t(&lang, "common.loading"))
                                .into_any_element()
                        }),
                )
                // 命令内容输入 (仅命令弹窗)
                .children(if !is_group_dialog {
                    Some(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted_foreground)
                                    .child(i18n::t(&lang, "snippets.dialog.command_content")),
                            )
                            .child(if let Some(input) = &command_input {
                                Input::new(input).into_any_element()
                            } else {
                                div()
                                    .text_sm()
                                    .text_color(muted_foreground)
                                    .child(i18n::t(&lang, "common.loading"))
                                    .into_any_element()
                            }),
                    )
                } else {
                    None
                })
                // 底部按钮
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_3()
                        .pt_2()
                        .child(
                            div()
                                .id("dialog-cancel-btn")
                                .px_4()
                                .py_2()
                                .bg(cx.theme().secondary)
                                .rounded_md()
                                .cursor_pointer()
                                .hover(move |s| s.bg(cx.theme().secondary_hover))
                                .on_click(move |_, _, cx| {
                                    state_cancel.update(cx, |s, _| s.close());
                                })
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(foreground)
                                        .child(i18n::t(&lang, "common.cancel")),
                                ),
                        )
                        .child(
                            div()
                                .id("dialog-save-btn")
                                .px_4()
                                .py_2()
                                .bg(cx.theme().primary)
                                .rounded_md()
                                .cursor_pointer()
                                .hover(move |s| s.bg(cx.theme().primary_hover))
                                .on_click(move |_, _, cx| {
                                    state_save.update(cx, |s, cx| {
                                        if s.is_group_dialog() {
                                            s.save_group(cx);
                                        } else {
                                            s.save_command(cx);
                                        }
                                    });
                                })
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().primary_foreground)
                                        .child(i18n::t(&lang, "common.save")),
                                ),
                        ),
                ),
        )
}
