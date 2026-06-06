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

/// 渲染字母头像（用于自定义供应商：圆底 + 名称首字符）
/// 颜色按字符确定性选取，深浅主题下均可读。
pub fn render_letter_avatar(ch: char, size: f32) -> impl IntoElement {
    // 一组柔和的强调色（圆底用低透明度，文字用实色）
    const PALETTE: [u32; 6] = [
        0x3b82f6, // 蓝
        0x22c55e, // 绿
        0xf59e0b, // 橙
        0xa855f7, // 紫
        0xef4444, // 红
        0x14b8a6, // 青
    ];
    let idx = (ch as usize) % PALETTE.len();
    let color: Hsla = gpui::rgb(PALETTE[idx]).into();
    let font_px = (size * 0.5).max(9.);

    div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(px(size))
        .h(px(size))
        .rounded_full()
        .bg(color.opacity(0.18))
        .child(
            div()
                .text_color(color)
                .text_size(px(font_px))
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(ch.to_string())),
        )
}
