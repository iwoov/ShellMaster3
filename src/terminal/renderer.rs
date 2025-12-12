// 终端渲染器 - 使用 Zed 风格的 BatchedTextRun + Canvas 渲染
// 性能优化：将 ~2000 个 div 减少到 ~100 个绘制调用

use gpui::*;

use alacritty_terminal::index::{Line, Point as AlacPoint};
use alacritty_terminal::Term;

use crate::models::settings::{CursorStyle, TerminalSettings};
use crate::terminal::batched_run::layout_grid;
use crate::terminal::colors::hex_to_hsla;
use crate::terminal::state::{EventProxy, TerminalSize};

/// 渲染终端内容（Canvas 方式）
pub fn render_terminal_view(
    term: &Term<EventProxy>,
    size: &TerminalSize,
    settings: &TerminalSettings,
    cursor_visible: bool,
    _cx: &App,
) -> impl IntoElement {
    // 预计算布局
    let layout = layout_grid(term, settings);

    // 获取颜色设置
    let bg_color = hex_to_hsla(&settings.background_color);
    let cursor_color = hex_to_hsla(&settings.cursor_color);

    // 获取光标位置
    let content = term.renderable_content();
    let cursor = content.cursor;
    let display_offset = content.display_offset as i32;
    let cursor_line = cursor.point.line.0 + display_offset;
    let cursor_point = if cursor_line >= 0 && cursor_line < size.lines as i32 {
        Some(AlacPoint::new(Line(cursor_line), cursor.point.column))
    } else {
        None
    };

    // 克隆需要移动到闭包的数据
    let cell_width = size.cell_width;
    let line_height = size.line_height;
    let font_family = settings.font_family.clone();
    let font_size = settings.font_size as f32;
    let cursor_style = settings.cursor_style.clone();

    div()
        .size_full()
        .bg(bg_color)
        .relative()
        .overflow_hidden()
        .child(
            canvas(
                // Layout 阶段：返回布局数据
                {
                    let layout = layout.clone();
                    move |_bounds, _window, _cx| layout.clone()
                },
                // Paint 阶段：绘制内容
                move |bounds, layout, window, cx| {
                    let origin = bounds.origin;

                    // 1. 绘制背景矩形
                    for rect in &layout.background_rects {
                        rect.paint(origin, cell_width, line_height, window);
                    }

                    // 2. 绘制文本运行
                    for run in &layout.text_runs {
                        run.paint(
                            origin,
                            cell_width,
                            line_height,
                            font_family.clone(),
                            font_size,
                            window,
                            cx,
                        );
                    }

                    // 3. 绘制光标
                    if cursor_visible {
                        if let Some(point) = cursor_point {
                            paint_cursor(
                                point,
                                origin,
                                cell_width,
                                line_height,
                                cursor_color,
                                &cursor_style,
                                window,
                            );
                        }
                    }
                },
            )
            .size_full(),
        )
}

/// 绘制光标
fn paint_cursor(
    alac_point: AlacPoint,
    origin: Point<Pixels>,
    cell_width: f32,
    line_height: f32,
    color: Hsla,
    style: &CursorStyle,
    window: &mut Window,
) {
    let x = origin.x + px(alac_point.column.0 as f32 * cell_width);
    let y = origin.y + px(alac_point.line.0 as f32 * line_height);

    let (cursor_pos, cursor_size) = match style {
        CursorStyle::Block => (Point::new(x, y), Size::new(px(cell_width), px(line_height))),
        CursorStyle::Bar => (Point::new(x, y), Size::new(px(2.0), px(line_height))),
        CursorStyle::Underline => (
            Point::new(x, y + px(line_height - 2.0)),
            Size::new(px(cell_width), px(2.0)),
        ),
    };

    window.paint_quad(fill(
        Bounds::new(cursor_pos, cursor_size),
        color.opacity(0.8),
    ));
}

/// 渲染空终端（用于未连接状态）
#[allow(dead_code)]
pub fn render_empty_terminal(settings: &TerminalSettings, message: &str, _cx: &App) -> Div {
    let bg_color = hex_to_hsla(&settings.background_color);
    let fg_color = hex_to_hsla(&settings.foreground_color);

    div()
        .size_full()
        .bg(bg_color)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_color(fg_color.opacity(0.5))
                .text_sm()
                .child(message.to_string()),
        )
}
