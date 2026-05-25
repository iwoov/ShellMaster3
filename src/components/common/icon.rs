// 图标组件

use gpui::*;

/// 渲染 SVG 图标
pub fn render_icon(path: &'static str, color: Hsla) -> impl IntoElement {
    svg().path(path).size_4().text_color(color)
}

/// 渲染 SVG/PNG 图标，保留原始颜色（用于品牌图标等）
pub fn render_colored_icon(path: &'static str, size: f32) -> impl IntoElement {
    img(path).w(px(size)).h(px(size))
}
