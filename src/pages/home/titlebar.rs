// 标题栏组件

use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;

/// 渲染标题栏
pub fn render_titlebar(cx: &App) -> impl IntoElement {
    let bg = crate::theme::titlebar_color(cx);
    let border = cx.theme().title_bar_border;
    let icon_color = cx.theme().muted_foreground;

    div()
        .h(px(44.))
        .w_full()
        .bg(bg)
        .border_b_1()
        .border_color(border)
        .flex()
        .items_center()
        .px_4()
        .justify_between()
        .child(
            div().flex().items_center().gap_4().child(
                div()
                    .cursor_pointer()
                    .child(render_icon(icons::HOME, icon_color.into())),
            ),
        )
}
