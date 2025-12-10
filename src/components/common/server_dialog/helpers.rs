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
