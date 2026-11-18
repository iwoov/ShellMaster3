// Snippets 弹窗渲染组件：新建 / 编辑分组和命令

use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::common::button::{text_button, ButtonKind};
use crate::components::common::dialog::{
    dialog_footer, dialog_header, dialog_panel, DialogOverlay,
};
use crate::components::common::input::{field_error, form_field, text_area, text_field};
use crate::i18n;

use super::state::{SnippetField, SnippetsDialogMode, SnippetsDialogState};

/// 弹窗宽度
const DIALOG_WIDTH: Pixels = px(460.);

/// 渲染 Snippets 弹窗覆盖层
pub fn render_snippets_dialog_overlay(
    state: Entity<SnippetsDialogState>,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let state_read = state.read(cx);
    let lang = state_read.lang.clone();
    let t = |key| i18n::t(&lang, key);
    let is_group_dialog = state_read.is_group_dialog();
    let title = match state_read.dialog_mode {
        SnippetsDialogMode::AddGroup => t("snippets.add_group"),
        SnippetsDialogMode::EditGroup => t("snippets.dialog.edit_group"),
        SnippetsDialogMode::AddCommand => t("snippets.add_command"),
        SnippetsDialogMode::EditCommand => t("snippets.dialog.edit_command"),
        SnippetsDialogMode::Closed => "",
    };

    let errors = state_read.visible_errors(cx);
    let error_for = |field| {
        errors
            .iter()
            .find(|(error_field, _)| *error_field == field)
            .map(|(_, key)| SharedString::from(t(key)))
    };
    let name_error = error_for(SnippetField::Name);
    let command_error = error_for(SnippetField::Command);

    // 名称
    let name_field = form_field(
        if is_group_dialog {
            t("snippets.dialog.group_name")
        } else {
            t("snippets.dialog.command_name")
        },
        match &state_read.name_input {
            Some(input) => {
                field_error(text_field(input), name_error.is_some(), &p).into_any_element()
            }
            None => div().into_any_element(),
        },
        name_error,
        &p,
    );

    // 命令内容（等宽字体，多行）
    let mono_font = cx.theme().mono_font_family.clone();
    let command_field = (!is_group_dialog).then(|| {
        form_field(
            t("snippets.dialog.command_content"),
            match &state_read.command_input {
                Some(input) => field_error(text_area(input), command_error.is_some(), &p)
                    .font_family(mono_font)
                    .into_any_element(),
                None => div().into_any_element(),
            },
            command_error,
            &p,
        )
    });

    let state_for_close = state.clone();
    let state_for_cancel = state.clone();
    let state_for_save = state.clone();
    let state_for_escape = state.clone();
    let state_for_enter = state.clone();

    let panel = dialog_panel(&p)
        .w(DIALOG_WIDTH)
        .child(dialog_header(
            title,
            None,
            t("common.close"),
            move |_, cx| {
                state_for_close.update(cx, |s, cx| {
                    s.close();
                    cx.notify();
                });
            },
            &p,
        ))
        .child(
            div()
                .px(px(20.))
                .pt(px(12.))
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(name_field)
                .children(command_field),
        )
        .child(
            dialog_footer(state_read.save_error.clone().map(SharedString::from), &p)
                .child(
                    text_button(
                        "snippets-cancel-btn",
                        ButtonKind::Secondary,
                        t("common.cancel"),
                        &p,
                    )
                    .on_click(move |_, _, cx| {
                        state_for_cancel.update(cx, |s, cx| {
                            s.close();
                            cx.notify();
                        });
                    }),
                )
                .child(
                    text_button(
                        "snippets-save-btn",
                        ButtonKind::Primary,
                        t("common.save"),
                        &p,
                    )
                    .on_click(move |_, window, cx| {
                        state_for_save.update(cx, |s, cx| s.try_save(window, cx));
                    }),
                ),
        );

    // 回车保存：名称输入框里按回车会冒泡到这里；多行的命令输入框里回车是换行，不会冒泡
    DialogOverlay::new()
        .focus(state_read.focus_handle.as_ref())
        .on_escape(move |_, cx| {
            state_for_escape.update(cx, |s, cx| {
                s.close();
                cx.notify();
            });
        })
        .on_enter(move |window, cx| {
            state_for_enter.update(cx, |s, cx| s.try_save(window, cx));
        })
        .render("snippets-dialog-overlay", panel, &p)
}
