// 标题栏组件

use gpui::*;

use crate::components::common::icon::render_icon;
use crate::constants::icons;

/// 渲染标题栏
pub fn render_titlebar() -> impl IntoElement {
    div()
        .h(px(44.))
        .w_full()
        .bg(rgb(0xffffff))
        .border_b_1()
        .border_color(rgb(0xe5e7eb))
        .flex()
        .items_center()
        .px_4()
        .justify_between()
        .child(
            div().flex().items_center().gap_4().child(
                div()
                    .cursor_pointer()
                    .child(render_icon(icons::HOME, rgb(0x6b7280).into())),
            ),
        )
}
