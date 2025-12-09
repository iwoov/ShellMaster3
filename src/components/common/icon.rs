// 图标组件

use gpui::*;

/// 渲染 SVG 图标
pub fn render_icon(path: &'static str, color: Hsla) -> impl IntoElement {
    svg().path(path).size_4().text_color(color)
}
