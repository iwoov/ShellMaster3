// 辅助渲染函数

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;

/// 渲染表单标签
pub fn render_form_label(label: &'static str, icon: &'static str, cx: &App) -> impl IntoElement {
    let icon_color = cx.theme().muted_foreground;
    let text_color = cx.theme().foreground;

    div()
        .flex()
        .items_center()
        .gap_1()
        .child(render_icon(icon, icon_color.into()))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(text_color)
                .child(SharedString::from(label)),
        )
}

/// 渲染开关组件 (使用 gpui-component Switch 保持一致性)
pub fn render_switch(
    checked: bool,
    on_click: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> gpui_component::switch::Switch {
    use gpui_component::switch::Switch;

    Switch::new("server-dialog-switch")
        .checked(checked)
        .on_click(move |new_val, window, cx| {
            on_click(new_val, window, cx);
        })
}

/// 渲染分组选择组件（Input + 内嵌下拉按钮）
/// 使用 Input 的 suffix 功能将下拉按钮嵌入输入框内部，视觉上成为一个整体
/// 下拉菜单使用 render_group_dropdown_overlay 在对话框顶层渲染，确保对齐
pub fn render_group_select(
    label: &'static str,
    icon: &'static str,
    group_input: Option<&gpui::Entity<gpui_component::input::InputState>>,
    state: gpui::Entity<super::ServerDialogState>,
    loading_text: &'static str,
    cx: &App,
) -> impl IntoElement {
    use gpui_component::input::Input;

    // 获取输入框背景色，用于下拉按钮
    let input_bg = cx.theme().background;

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(render_form_label(label, icon, cx))
        .child(if let Some(input) = group_input {
            // 创建下拉按钮作为 suffix
            // 点击时切换 show_group_dropdown 状态，由 overlay 显示下拉菜单
            let state_for_click = state.clone();
            let dropdown_button = div()
                .id("group-dropdown-suffix")
                .flex()
                .items_center()
                .justify_center()
                .px_1()
                .mr(px(-12.))
                .bg(input_bg)
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                    state_for_click.update(cx, |s, _| {
                        s.show_group_dropdown = !s.show_group_dropdown;
                    });
                })
                .child(render_icon(
                    crate::constants::icons::CHEVRON_DOWN,
                    cx.theme().muted_foreground.into(),
                ));

            Input::new(input).suffix(dropdown_button).into_any_element()
        } else {
            div().child(loading_text).into_any_element()
        })
}
