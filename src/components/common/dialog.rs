// 对话框组件

use gpui::*;
use gpui_component::ActiveTheme;

/// 确认对话框的文案
pub struct ConfirmDialogText {
    pub title: SharedString,
    pub message: SharedString,
    pub confirm: SharedString,
    pub cancel: SharedString,
}

/// 渲染一个用于危险操作（删除等）的确认对话框覆盖层。
///
/// `focus_handle` 由调用方在打开对话框时创建并聚焦，对话框据此响应 Esc 取消。
/// 确认只能点击按钮：危险操作不设回车默认键，避免连按回车误删。
pub fn render_confirm_dialog(
    text: ConfirmDialogText,
    focus_handle: &FocusHandle,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + Clone + 'static,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let (secondary_hover, danger_hover) = (theme.secondary_hover, theme.danger_hover);

    let cancel_on_key = on_cancel.clone();

    div()
        .id("confirm-dialog-overlay")
        .track_focus(focus_handle)
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(gpui::black().opacity(0.5))
        .flex()
        .items_center()
        .justify_center()
        // 遮罩拦截所有鼠标事件，避免点到下层内容
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_key_down(move |event, window, cx| {
            if event.keystroke.key == "escape" {
                cx.stop_propagation();
                cancel_on_key(window, cx);
            }
        })
        .child(
            div()
                .w(px(400.))
                .bg(theme.popover)
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .p_6()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.foreground)
                        .child(text.title),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(text.message),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_3()
                        .pt_3()
                        .child(
                            div()
                                .id("confirm-dialog-cancel-btn")
                                .px_4()
                                .py_2()
                                .bg(theme.secondary)
                                .rounded_md()
                                .cursor_pointer()
                                .hover(move |s| s.bg(secondary_hover))
                                .on_click(move |_, window, cx| on_cancel(window, cx))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.foreground)
                                        .child(text.cancel),
                                ),
                        )
                        .child(
                            div()
                                .id("confirm-dialog-confirm-btn")
                                .px_4()
                                .py_2()
                                .bg(theme.danger)
                                .rounded_md()
                                .cursor_pointer()
                                .hover(move |s| s.bg(danger_hover))
                                .on_click(move |_, window, cx| on_confirm(window, cx))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.danger_foreground)
                                        .child(text.confirm),
                                ),
                        ),
                ),
        )
}
