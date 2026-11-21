// 会话视图的分栏宽度：左列（监控、SFTP 目录树）、右侧侧栏、最右边的图标栏

use gpui::*;

/// 左列默认宽度，与主页侧栏一致：切换主页和会话时，标题栏品牌区的右边线不动
pub const LEFT_COLUMN_DEFAULT: Pixels = px(220.);
/// 左列可拖动的范围
pub const LEFT_COLUMN_MIN: Pixels = px(180.);
pub const LEFT_COLUMN_MAX: Pixels = px(360.);

/// 右侧侧栏默认宽度（与 Zed 的 Agent Panel 一致）
pub const SIDEBAR_DEFAULT: Pixels = px(300.);
/// 右侧侧栏可拖动的范围
pub const SIDEBAR_MIN: Pixels = px(260.);
pub const SIDEBAR_MAX: Pixels = px(520.);

/// 最右边图标栏的宽度
pub const RAIL_WIDTH: Pixels = px(40.);
/// 中间（终端、文件列表）至少要留的宽度
pub const CENTER_MIN: Pixels = px(320.);

/// 各栏的实际宽度
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SessionColumns {
    /// 左列宽度（含右边线）
    pub left: Pixels,
    /// 右侧侧栏宽度（含左边线）；侧栏收起时为 0
    pub sidebar: Pixels,
    /// 窗口宽度
    pub window: Pixels,
}

impl SessionColumns {
    /// 右侧侧栏左边线的横坐标
    pub fn sidebar_left(&self) -> Pixels {
        self.window - RAIL_WIDTH - self.sidebar
    }
}

/// 按窗口宽度算出各栏宽度：先把左列和侧栏夹到各自的范围里，
/// 中间不够 [`CENTER_MIN`] 时先压缩侧栏、再压缩左列（都不低于各自的最小值）。
pub fn fit_columns(
    window: Pixels,
    left_preferred: Pixels,
    sidebar_preferred: Pixels,
    sidebar_visible: bool,
) -> SessionColumns {
    let mut left = left_preferred.clamp(LEFT_COLUMN_MIN, LEFT_COLUMN_MAX);
    let mut sidebar = if sidebar_visible {
        sidebar_preferred.clamp(SIDEBAR_MIN, SIDEBAR_MAX)
    } else {
        px(0.)
    };

    let mut overflow = left + sidebar + RAIL_WIDTH + CENTER_MIN - window;
    if overflow > px(0.) && sidebar_visible {
        let shrink = overflow.min(sidebar - SIDEBAR_MIN);
        sidebar -= shrink;
        overflow -= shrink;
    }
    if overflow > px(0.) {
        left -= overflow.min(left - LEFT_COLUMN_MIN);
    }

    SessionColumns {
        left,
        sidebar,
        window,
    }
}

/// 拖动左列分隔条：鼠标横坐标就是左列宽度（会话视图从窗口最左边开始）
pub fn left_width_from_drag(mouse_x: Pixels) -> Pixels {
    mouse_x.clamp(LEFT_COLUMN_MIN, LEFT_COLUMN_MAX)
}

/// 拖动侧栏分隔条：侧栏宽度是鼠标到图标栏左边的距离
pub fn sidebar_width_from_drag(window: Pixels, mouse_x: Pixels) -> Pixels {
    (window - RAIL_WIDTH - mouse_x).clamp(SIDEBAR_MIN, SIDEBAR_MAX)
}

#[cfg(test)]
mod tests {
    // 不能 `use super::*`：父模块的 `use gpui::*` 会把 gpui 的 `test` 宏带进来，盖掉标准库的 `#[test]`
    use super::{
        fit_columns, left_width_from_drag, sidebar_width_from_drag, LEFT_COLUMN_MIN, SIDEBAR_MAX,
        SIDEBAR_MIN,
    };
    use gpui::px;

    #[test]
    fn wide_windows_keep_the_preferred_widths() {
        let columns = fit_columns(px(1200.), px(220.), px(300.), true);
        assert_eq!(columns.left, px(220.));
        assert_eq!(columns.sidebar, px(300.));
        assert_eq!(columns.sidebar_left(), px(1200. - 40. - 300.));
    }

    #[test]
    fn preferences_are_clamped_to_their_ranges() {
        let columns = fit_columns(px(2000.), px(50.), px(9000.), true);
        assert_eq!(columns.left, LEFT_COLUMN_MIN);
        assert_eq!(columns.sidebar, SIDEBAR_MAX);
    }

    #[test]
    fn narrow_windows_shrink_the_sidebar_first() {
        // 860 - 40 - 320 = 500 留给左列和侧栏：侧栏先从 300 让到 280
        let columns = fit_columns(px(860.), px(220.), px(300.), true);
        assert_eq!(columns.sidebar, px(280.));
        assert_eq!(columns.left, px(220.));
    }

    #[test]
    fn then_the_left_column_down_to_its_minimum() {
        // 侧栏已到最小值 260，再压缩左列；两者都到最小值后不再压缩
        let columns = fit_columns(px(820.), px(240.), px(300.), true);
        assert_eq!(columns.sidebar, SIDEBAR_MIN);
        assert_eq!(columns.left, px(820. - 40. - 320. - 260.));
        let columns = fit_columns(px(600.), px(240.), px(300.), true);
        assert_eq!(columns.sidebar, SIDEBAR_MIN);
        assert_eq!(columns.left, LEFT_COLUMN_MIN);
    }

    #[test]
    fn a_hidden_sidebar_takes_no_width() {
        let columns = fit_columns(px(860.), px(220.), px(300.), false);
        assert_eq!(columns.sidebar, px(0.));
        assert_eq!(columns.left, px(220.));
        assert_eq!(columns.sidebar_left(), px(860. - 40.));
    }

    #[test]
    fn drag_positions_map_to_clamped_widths() {
        assert_eq!(left_width_from_drag(px(250.)), px(250.));
        assert_eq!(left_width_from_drag(px(20.)), LEFT_COLUMN_MIN);
        assert_eq!(sidebar_width_from_drag(px(1200.), px(800.)), px(360.));
        assert_eq!(sidebar_width_from_drag(px(1200.), px(1150.)), SIDEBAR_MIN);
    }
}
