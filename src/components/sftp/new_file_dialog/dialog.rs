// 新建文件对话框渲染组件

use gpui::*;
use gpui_component::input::Input;
use gpui_component::ActiveTheme;

use crate::i18n;
use crate::models::settings::Language;
use crate::services::storage;

use super::state::NewFileDialogState;

/// 渲染新建文件对话框覆盖层
pub fn render_new_file_dialog_overlay<F>(
    state: Entity<NewFileDialogState>,
    on_create: F,
    cx: &App,
) -> impl IntoElement
where
    F: Fn(String, String, &mut App) + Clone + 'static,
{
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let state_read = state.read(cx);
    let name_input = state_read.name_input.clone();
    let error_message = state_read.error_message.clone();
    let is_creating = state_read.is_creating;

    let state_cancel = state.clone();
    let state_create = state.clone();

    let bg_color = cx.theme().popover;
    let border_color = cx.theme().border;
    let foreground = cx.theme().foreground;
    let muted_foreground = cx.theme().muted_foreground;
    let danger = cx.theme().danger;

    div()
        .id("new-file-dialog-overlay")
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
                .w(px(400.))
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
                        .child(i18n::t(&lang, "sftp.new_file.title")),
                )
                // 文件名称输入
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .text_color(muted_foreground)
                                .child(i18n::t(&lang, "sftp.new_file.name")),
                        )
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
                // 错误信息
                .children(error_message.map(|msg| div().text_sm().text_color(danger).child(msg)))
                // 底部按钮
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_3()
                        .pt_2()
                        // 取消按钮
                        .child(
                            div()
                                .id("new-file-cancel-btn")
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
                        // 创建按钮
                        .child({
                            let create_btn = div()
                                .id("new-file-create-btn")
                                .px_4()
                                .py_2()
                                .bg(cx.theme().primary)
                                .rounded_md()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().primary_foreground)
                                        .child(if is_creating {
                                            i18n::t(&lang, "common.loading")
                                        } else {
                                            i18n::t(&lang, "common.confirm")
                                        }),
                                );

                            if is_creating {
                                create_btn.opacity(0.6)
                            } else {
                                create_btn
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(cx.theme().primary_hover))
                                    .on_click(move |_, _, cx| {
                                        state_create.update(cx, |s, cx| {
                                            if s.validate_name(cx) {
                                                let full_path = s.get_full_path(cx);
                                                let tab_id = s.tab_id.clone();
                                                s.start_creating();
                                                on_create(full_path, tab_id, cx);
                                            }
                                        });
                                    })
                            }
                        }),
                ),
        )
}
