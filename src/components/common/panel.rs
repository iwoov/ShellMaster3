// 会话视图各面板共用的部件：标题条、分组小标题、状态块、加载图标、进度条、迷你曲线、分隔条

use std::time::Duration;

use gpui_kit::*;

use super::icon::icon_sized;
use crate::constants::icons;
use crate::theme::Palette;

/// 面板标题条的高度：终端标签条、SFTP 两栏表头、右侧面板标题、监控头部共用，同一行的底线连成一条
pub const PANEL_HEADER_HEIGHT: Pixels = px(36.);
/// 列表行的高度：目录树、文件列表、快捷命令
pub const LIST_ROW_HEIGHT: Pixels = px(30.);
/// 标题条里的小号控件（终端标签、路径框）的高度
pub const HEADER_CONTROL_HEIGHT: Pixels = px(28.);
/// 面板内容的左右留白
pub const PANEL_PADDING: Pixels = px(12.);

/// 面板标题条：固定高度、底部细线、横向排列，调用方往里放标题和按钮
pub fn panel_header(p: &Palette) -> Div {
    div()
        .flex_none()
        .h(PANEL_HEADER_HEIGHT)
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(4.))
        .border_b_1()
        .border_color(p.border)
}

/// 面板标题文字（13 半粗）
pub fn panel_title(text: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .flex_none()
        .whitespace_nowrap()
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.text)
        .child(text.into())
}

/// 分组小标题（11 半粗弱色），与主页侧栏的「最近连接」一致
pub fn section_title(text: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .flex_none()
        .whitespace_nowrap()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.text_faint)
        .child(text.into())
}

/// 状态块左上方的图形
pub enum StateIcon {
    Icon(&'static str),
    /// 转动的加载图标
    Spinner,
}

/// 居中的状态块：空列表、未连接、出错、加载中。返回纵向容器，调用方可以继续 `.child()` 追加按钮
pub fn panel_state(
    id: impl Into<ElementId>,
    icon: StateIcon,
    icon_color: Hsla,
    title: impl Into<SharedString>,
    description: Option<SharedString>,
    p: &Palette,
) -> Div {
    div()
        .size_full()
        .min_h(px(0.))
        .p(px(16.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .child(match icon {
            StateIcon::Icon(path) => icon_sized(path, 20., icon_color).into_any_element(),
            StateIcon::Spinner => spinner(id, 20., icon_color).into_any_element(),
        })
        .child(
            div()
                .mt(px(2.))
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.text_muted)
                .text_center()
                .child(title.into()),
        )
        .children(description.map(|description| {
            div()
                .max_w(px(320.))
                .text_size(px(12.))
                .text_color(p.text_faint)
                .text_center()
                .child(description)
        }))
}

/// 转动的加载图标。同一个父元素下放多个时 `id` 要各不相同
pub fn spinner(id: impl Into<ElementId>, size: f32, color: Hsla) -> impl IntoElement {
    icon_sized(icons::LOADER, size, color).with_animation(
        id,
        Animation::new(Duration::from_millis(900)).repeat(),
        |icon, delta| icon.with_transformation(Transformation::rotate(percentage(delta))),
    )
}

/// 小圆点（状态指示）
pub fn status_dot(color: Hsla, size: f32) -> Div {
    div().flex_none().size(px(size)).rounded_full().bg(color)
}

/// 用量的严重程度
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageLevel {
    Normal,
    /// 达到 75%
    Warning,
    /// 达到 90%
    Critical,
}

/// 按自定义危险阈值判断用量的严重程度（警告阈值自动取 `critical - 15`，下限 1%）
pub fn usage_level_with_threshold(percent: f32, critical_threshold: f32) -> UsageLevel {
    let critical = critical_threshold.clamp(10., 100.);
    let warning = (critical - 15.).max(1.);
    if percent >= critical {
        UsageLevel::Critical
    } else if percent >= warning {
        UsageLevel::Warning
    } else {
        UsageLevel::Normal
    }
}

/// 按百分比判断用量的严重程度（默认 75% 警告、90% 危险）
pub fn usage_level(percent: f32) -> UsageLevel {
    usage_level_with_threshold(percent, 90.)
}

/// 按自定义危险阈值取用量对应的颜色
pub fn usage_color_with_threshold(percent: f32, critical_threshold: f32, p: &Palette) -> Hsla {
    match usage_level_with_threshold(percent, critical_threshold) {
        UsageLevel::Normal => p.primary,
        UsageLevel::Warning => p.warning,
        UsageLevel::Critical => p.danger,
    }
}

/// 用量对应的颜色：正常为主色，75% 起为警告色，90% 起为危险色
pub fn usage_color(percent: f32, p: &Palette) -> Hsla {
    usage_color_with_threshold(percent, 90., p)
}

/// 细进度条：`fraction` 为 0–1，超出范围按边界处理
pub fn meter_bar(fraction: f32, color: Hsla, height: f32, p: &Palette) -> Div {
    let fraction = if fraction.is_finite() {
        fraction.clamp(0., 1.)
    } else {
        0.
    };
    div()
        .w_full()
        .h(px(height))
        .rounded_full()
        .bg(p.hover_strong)
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(relative(fraction))
                .rounded_full()
                .bg(color),
        )
}

/// 迷你曲线中的一组数据
pub struct SparkSeries {
    pub values: Vec<f32>,
    pub stroke: Hsla,
    /// 曲线下方的填充色，`None` 时只画线
    pub fill: Option<Hsla>,
}

/// 迷你曲线：各组数据按共同的上限 `max` 缩放，横向分成 `slots` 格、靠右对齐（最新的点在最右边，
/// 数据不足 `slots` 个时左边留空，像滚动的监控曲线）。数据少于两个点时什么都不画
pub fn sparkline(
    series: Vec<SparkSeries>,
    max: f32,
    slots: usize,
    height: Pixels,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            for series in &series {
                let points = spark_points(&series.values, max, slots, bounds);
                if points.len() < 2 {
                    continue;
                }
                if let Some(fill) = series.fill {
                    let mut area = PathBuilder::fill();
                    area.move_to(point(points[0].x, bounds.bottom()));
                    for point in &points {
                        area.line_to(*point);
                    }
                    area.line_to(point(points[points.len() - 1].x, bounds.bottom()));
                    area.close();
                    if let Ok(path) = area.build() {
                        window.paint_path(path, fill);
                    }
                }
                let mut line = PathBuilder::stroke(px(SPARK_STROKE));
                line.move_to(points[0]);
                for point in &points[1..] {
                    line.line_to(*point);
                }
                if let Ok(path) = line.build() {
                    window.paint_path(path, series.stroke);
                }
            }
        },
    )
    .w_full()
    .h(height)
}

/// 曲线的线宽
const SPARK_STROKE: f32 = 1.5;

/// 曲线各点在 `bounds` 里的位置：横向分成 `slots` 格、靠右对齐，纵向按 `max` 缩放（0 在底部）。
/// 上下各留出半个线宽，最高点和最低点的线不会被裁掉。
pub fn spark_points(
    values: &[f32],
    max: f32,
    slots: usize,
    bounds: Bounds<Pixels>,
) -> Vec<Point<Pixels>> {
    if values.len() < 2 {
        return Vec::new();
    }
    let slots = slots.max(values.len());
    let max = if max.is_finite() && max > 0. { max } else { 1. };
    let inset = SPARK_STROKE / 2.;
    let top = f32::from(bounds.top()) + inset;
    let bottom = f32::from(bounds.bottom()) - inset;
    let left = f32::from(bounds.left());
    let step = f32::from(bounds.size.width) / (slots - 1) as f32;
    let first_slot = slots - values.len();

    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let value = if value.is_finite() { *value } else { 0. };
            let ratio = (value / max).clamp(0., 1.);
            point(
                px(left + step * (first_slot + index) as f32),
                px(bottom - (bottom - top) * ratio),
            )
        })
        .collect()
}

/// 拖动竖向分隔条时携带的数据，`id` 用来区分不同的分隔条
#[derive(Clone)]
pub struct SplitterDrag {
    pub id: &'static str,
}

impl Render for SplitterDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// 竖向分隔条的拖动热区宽度
const SPLITTER_HIT_WIDTH: Pixels = px(7.);

/// 竖向分隔条：盖在两栏交界处（`x` 为交界线的横坐标，相对于父元素），本身是 7px 宽的透明热区，
/// 悬停和拖动时中间显示一条主色细线。拖动时把鼠标在窗口里的横坐标交给 `on_drag`，由调用方换算成宽度。
/// 父元素需要是 `relative()`，并且把它作为最后一个子元素，保证盖在两栏上面。
pub fn splitter(
    id: &'static str,
    x: Pixels,
    p: &Palette,
    on_drag: impl Fn(Pixels, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let active = p.primary;
    let group = SharedString::from(format!("splitter-{id}"));
    div()
        .id(id)
        .group(group.clone())
        .absolute()
        .top_0()
        .bottom_0()
        .left(x - SPLITTER_HIT_WIDTH / 2.)
        .w(SPLITTER_HIT_WIDTH)
        .cursor_col_resize()
        .flex()
        .justify_center()
        .child(
            div()
                .w(px(1.))
                .h_full()
                .group_hover(group, move |s| s.bg(active)),
        )
        .on_drag(SplitterDrag { id }, |drag, _, _, cx| {
            cx.stop_propagation();
            cx.new(|_| drag.clone())
        })
        .on_drag_move(move |event: &DragMoveEvent<SplitterDrag>, window, cx| {
            if event.drag(cx).id == id {
                on_drag(event.event.position.x, window, cx);
            }
        })
}

#[cfg(test)]
mod tests {
    // 不能 `use super::*`：父模块的 `use gpui_kit::*` 会把 gpui_kit 的 `test` 宏带进来，盖掉标准库的 `#[test]`
    use super::{spark_points, usage_level, UsageLevel};
    use gpui_kit::{point, px, size, Bounds};

    #[test]
    fn usage_levels_follow_the_thresholds() {
        assert_eq!(usage_level(0.), UsageLevel::Normal);
        assert_eq!(usage_level(74.9), UsageLevel::Normal);
        assert_eq!(usage_level(75.), UsageLevel::Warning);
        assert_eq!(usage_level(89.9), UsageLevel::Warning);
        assert_eq!(usage_level(90.), UsageLevel::Critical);
        assert_eq!(usage_level(130.), UsageLevel::Critical);
    }

    #[test]
    fn spark_points_span_the_width_and_scale_to_max() {
        let bounds = Bounds::new(point(px(10.), px(20.)), size(px(100.), px(41.5)));
        let points = spark_points(&[0., 50., 100.], 100., 3, bounds);
        assert_eq!(points.len(), 3);
        // 横向：从左边界到右边界均分
        assert_eq!(points[0].x, px(10.));
        assert_eq!(points[1].x, px(60.));
        assert_eq!(points[2].x, px(110.));
        // 纵向：0 在底部、上限在顶部，各留半个线宽
        assert_eq!(points[0].y, px(61.5 - 0.75));
        assert_eq!(points[2].y, px(20. + 0.75));
        assert_eq!(points[1].y, px((20.75 + 60.75) / 2.));
    }

    #[test]
    fn spark_points_right_align_when_there_are_fewer_values_than_slots() {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(100.), px(10.)));
        // 5 格（4 段，每段 25px），2 个点落在最右边两格
        let points = spark_points(&[1., 2.], 2., 5, bounds);
        assert_eq!(points[0].x, px(75.));
        assert_eq!(points[1].x, px(100.));
        // 数据比格子多时按数据个数铺满
        let points = spark_points(&[1., 2., 3.], 3., 2, bounds);
        assert_eq!(points[0].x, px(0.));
        assert_eq!(points[2].x, px(100.));
    }

    #[test]
    fn spark_points_clamp_and_skip_degenerate_input() {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(10.), px(11.5)));
        assert!(spark_points(&[5.], 10., 30, bounds).is_empty());
        // 超过上限的值贴顶，负数和非数值贴底；上限无效时按 1 处理
        let points = spark_points(&[20., -3., f32::NAN], 10., 3, bounds);
        assert_eq!(points[0].y, px(0.75));
        assert_eq!(points[1].y, px(10.75));
        assert_eq!(points[2].y, px(10.75));
        let points = spark_points(&[1., 0.], 0., 2, bounds);
        assert_eq!(points[0].y, px(0.75));
    }
}
