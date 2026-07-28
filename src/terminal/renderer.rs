// 终端渲染器 - 使用 Zed 风格的 BatchedTextRun + Canvas 渲染
// 性能优化：将 ~2000 个 div 减少到 ~100 个绘制调用

use gpui::*;

use alacritty_terminal::index::{Line, Point as AlacPoint};
use alacritty_terminal::term::search::Match;
use alacritty_terminal::vte::ansi::CursorShape;
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
    bell_flash: bool,
    search_matches: &[Match],
    search_current: Option<usize>,
    _cx: &App,
) -> impl IntoElement {
    // 预计算布局
    let layout = layout_grid(term, settings, search_matches, search_current);

    // 获取颜色设置（背景应用不透明度设置）
    let bg_opacity = (settings.background_opacity as f32 / 100.0).clamp(0.0, 1.0);
    let bg_color = hex_to_hsla(&settings.background_color).opacity(bg_opacity);
    let cursor_color = hex_to_hsla(&settings.cursor_color);
    let flash_color = hex_to_hsla(&settings.foreground_color).opacity(0.25);

    // 获取光标位置与形状
    let content = term.renderable_content();
    let cursor = content.cursor;
    let display_offset = content.display_offset as i32;
    let cursor_line = cursor.point.line.0 + display_offset;
    // 应用请求隐藏光标（如 SHOW_CURSOR 关闭）时不绘制
    let cursor_hidden = cursor.shape == CursorShape::Hidden;
    let cursor_point = if !cursor_hidden && cursor_line >= 0 && cursor_line < size.lines as i32 {
        Some(AlacPoint::new(Line(cursor_line), cursor.point.column))
    } else {
        None
    };

    // 克隆需要移动到闭包的数据
    let cell_width = size.cell_width;
    let line_height = size.line_height;
    let font_family = settings.font_family.clone();
    let font_size = settings.font_size as f32;
    let ligatures = settings.ligatures;
    let padding = settings.padding as f32;
    // 依据应用上报的形状（DECSCUSR）决定绘制样式：Block 时沿用用户设置。
    let cursor_kind = draw_cursor_kind(cursor.shape, &settings.cursor_style);

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
                    // 应用左侧 padding 偏移
                    let origin = Point::new(bounds.origin.x + px(padding), bounds.origin.y);

                    // 1. 绘制背景矩形
                    for rect in &layout.background_rects {
                        rect.paint(origin, cell_width, line_height, window);
                    }

                    // 2. 绘制选择高亮矩形
                    for rect in &layout.selection_rects {
                        rect.paint(origin, cell_width, line_height, window);
                    }

                    // 2.5 绘制搜索命中高亮
                    for rect in &layout.search_rects {
                        rect.paint(origin, cell_width, line_height, window);
                    }

                    // 3. 绘制文本运行
                    for run in &layout.text_runs {
                        run.paint(
                            origin,
                            cell_width,
                            line_height,
                            font_family.clone(),
                            font_size,
                            ligatures,
                            window,
                            cx,
                        );
                    }

                    // 4. 绘制光标
                    if cursor_visible {
                        if let Some(point) = cursor_point {
                            paint_cursor(
                                point,
                                origin,
                                cell_width,
                                line_height,
                                cursor_color,
                                cursor_kind,
                                window,
                            );
                        }
                    }
                },
            )
            .size_full(),
        )
        // 视觉响铃：短暂的半透明叠加层
        .children(bell_flash.then(|| div().absolute().inset_0().bg(flash_color)))
}

/// 光标绘制样式（已综合应用上报形状与用户设置）。
#[derive(Clone, Copy)]
enum DrawCursorKind {
    Block,
    Bar,
    Underline,
    Hollow,
}

/// 依据应用上报的 CursorShape 与用户默认样式决定实际绘制样式。
/// Block（默认形状）时沿用用户设置，其余形状遵循应用请求。
fn draw_cursor_kind(shape: CursorShape, user_style: &CursorStyle) -> DrawCursorKind {
    match shape {
        CursorShape::Block => match user_style {
            CursorStyle::Block => DrawCursorKind::Block,
            CursorStyle::Bar => DrawCursorKind::Bar,
            CursorStyle::Underline => DrawCursorKind::Underline,
        },
        CursorShape::Beam => DrawCursorKind::Bar,
        CursorShape::Underline => DrawCursorKind::Underline,
        CursorShape::HollowBlock => DrawCursorKind::Hollow,
        // Hidden 在调用方已过滤，这里回退为 Block。
        CursorShape::Hidden => DrawCursorKind::Block,
    }
}

/// 绘制光标
fn paint_cursor(
    alac_point: AlacPoint,
    origin: Point<Pixels>,
    cell_width: f32,
    line_height: f32,
    color: Hsla,
    kind: DrawCursorKind,
    window: &mut Window,
) {
    let x = origin.x + px(alac_point.column.0 as f32 * cell_width);
    let y = origin.y + px(alac_point.line.0 as f32 * line_height);
    let cursor_color = color.opacity(0.8);

    match kind {
        DrawCursorKind::Block => {
            window.paint_quad(fill(
                Bounds::new(Point::new(x, y), Size::new(px(cell_width), px(line_height))),
                cursor_color,
            ));
        }
        DrawCursorKind::Bar => {
            window.paint_quad(fill(
                Bounds::new(Point::new(x, y), Size::new(px(2.0), px(line_height))),
                cursor_color,
            ));
        }
        DrawCursorKind::Underline => {
            window.paint_quad(fill(
                Bounds::new(
                    Point::new(x, y + px(line_height - 2.0)),
                    Size::new(px(cell_width), px(2.0)),
                ),
                cursor_color,
            ));
        }
        DrawCursorKind::Hollow => {
            // 空心方块：绘制四条 1px 边框
            let t = px(1.0);
            let w = px(cell_width);
            let h = px(line_height);
            // 上
            window.paint_quad(fill(
                Bounds::new(Point::new(x, y), Size::new(w, t)),
                cursor_color,
            ));
            // 下
            window.paint_quad(fill(
                Bounds::new(Point::new(x, y + h - t), Size::new(w, t)),
                cursor_color,
            ));
            // 左
            window.paint_quad(fill(
                Bounds::new(Point::new(x, y), Size::new(t, h)),
                cursor_color,
            ));
            // 右
            window.paint_quad(fill(
                Bounds::new(Point::new(x + w - t, y), Size::new(t, h)),
                cursor_color,
            ));
        }
    }
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
