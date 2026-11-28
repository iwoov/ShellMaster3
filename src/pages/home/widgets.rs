// 主页各页面共用的界面部件
//
// 按钮、文字截断等通用部件放在 `components::common`，这里重新导出，主页各文件照旧从本模块引用。

use gpui_kit::*;

pub use crate::components::common::button::{
    icon_button, icon_button_colored, primary_button, secondary_button, text_tooltip,
    ICON_BUTTON_SIZE,
};
pub use crate::components::common::text::TextFitter;
use crate::theme::Palette;

/// 侧边栏宽度（会话视图左上角的品牌区与之等宽）
pub const SIDEBAR_WIDTH: Pixels = px(220.);
/// 窗口顶部条的高度。主页的品牌区、标签条和会话视图的标题栏共用这个高度，
/// 标签页在两个视图之间切换时位置不会跳动。
pub const TOPBAR_HEIGHT: Pixels = px(48.);
/// 页面内容的左右留白
pub const CONTENT_PADDING: Pixels = px(24.);

/// 页面标题行：左侧是标题和计数，调用方用 `.child()` 在右侧追加该页的操作控件
pub fn page_header(
    title: impl Into<SharedString>,
    count: Option<SharedString>,
    p: &Palette,
) -> Div {
    div()
        .flex_none()
        .h(px(32.))
        .mt(px(2.))
        .mb(px(14.))
        .px(CONTENT_PADDING)
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_none()
                .text_size(px(18.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text)
                .child(title.into()),
        )
        .children(count.map(|count| {
            div()
                .flex_none()
                .pt(px(3.))
                .text_size(px(13.))
                .text_color(p.text_faint)
                .child(count)
        }))
        .child(div().flex_1())
}

/// 按最小列宽计算一行能排下的列数（至少 1 列），用于让卡片铺满整行
pub fn grid_columns(available: Pixels, min_column_width: Pixels, gap: Pixels) -> u16 {
    let columns = ((available + gap) / (min_column_width + gap)).floor();
    // 负数和 NaN 转成 0，再保证至少 1 列
    (columns as u16).max(1)
}

/// 把 `available` 均分成 `columns` 列后每一列的宽度
pub fn grid_cell_width(available: Pixels, columns: u16, gap: Pixels) -> Pixels {
    let columns = columns.max(1) as f32;
    ((available - gap * (columns - 1.)) / columns).max(px(0.))
}

#[cfg(test)]
mod tests {
    use super::{grid_cell_width, grid_columns};
    use gpui_kit::px;

    #[test]
    fn grid_columns_fills_the_row() {
        // 3 列 264 + 2 个间距 12 = 816
        assert_eq!(grid_columns(px(816.), px(264.), px(12.)), 3);
        assert_eq!(grid_columns(px(815.), px(264.), px(12.)), 2);
        assert_eq!(grid_columns(px(932.), px(264.), px(12.)), 3);
        assert_eq!(grid_columns(px(1332.), px(264.), px(12.)), 4);
    }

    #[test]
    fn grid_columns_never_returns_zero() {
        assert_eq!(grid_columns(px(100.), px(264.), px(12.)), 1);
        assert_eq!(grid_columns(px(0.), px(264.), px(12.)), 1);
        assert_eq!(grid_columns(px(-50.), px(264.), px(12.)), 1);
    }

    #[test]
    fn grid_cells_share_the_width_left_after_gaps() {
        assert_eq!(grid_cell_width(px(932.), 3, px(12.)), px(908.) / 3.);
        assert_eq!(grid_cell_width(px(500.), 1, px(12.)), px(500.));
        // 列数为 0 时按 1 列处理；放不下时宽度为 0 而不是负数
        assert_eq!(grid_cell_width(px(500.), 0, px(12.)), px(500.));
        assert_eq!(grid_cell_width(px(10.), 3, px(12.)), px(0.));
    }
}
