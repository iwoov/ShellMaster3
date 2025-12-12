// 终端渲染器 - 将 alacritty_terminal 内容渲染到 GPUI

use gpui::*;
use gpui_component::ActiveTheme;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point as AlacPoint};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};

use crate::models::settings::{CursorStyle, TerminalSettings};
use crate::terminal::colors::{alac_rgb_to_hsla, ansi_indexed_color, hex_to_hsla};
use crate::terminal::state::{EventProxy, TerminalSize};

use alacritty_terminal::Term;

/// 终端渲染边界
#[derive(Clone, Debug)]
pub struct TerminalBounds {
    pub cell_width: Pixels,
    pub line_height: Pixels,
    pub columns: usize,
    pub lines: usize,
}

impl TerminalBounds {
    pub fn new(size: &TerminalSize) -> Self {
        Self {
            cell_width: px(size.cell_width),
            line_height: px(size.line_height),
            columns: size.columns,
            lines: size.lines,
        }
    }
}

/// 渲染的单元格
struct RenderCell {
    point: AlacPoint,
    fg: Hsla,
    bg: Option<Hsla>,
    c: char,
    flags: Flags,
}

/// 渲染终端内容
pub fn render_terminal_view(
    term: &Term<EventProxy>,
    size: &TerminalSize,
    settings: &TerminalSettings,
    cursor_visible: bool,
    cx: &App,
) -> Div {
    let bounds = TerminalBounds::new(size);

    // 获取背景色
    let bg_color = hex_to_hsla(&settings.background_color);
    let fg_default = hex_to_hsla(&settings.foreground_color);
    let cursor_color = hex_to_hsla(&settings.cursor_color);

    // 收集可渲染的单元格
    let mut cells: Vec<RenderCell> = Vec::new();
    let content = term.renderable_content();

    for cell in content.display_iter {
        let point = cell.point;

        // 跳过屏幕外的单元格
        if point.line.0 < 0 || point.line.0 >= size.lines as i32 {
            continue;
        }

        let c = cell.cell.c;

        // 跳过空格（除非有背景色）
        let has_bg = cell.cell.bg != AnsiColor::Named(NamedColor::Background);
        if c == ' '
            && !has_bg
            && !cell
                .cell
                .flags
                .intersects(Flags::UNDERLINE | Flags::STRIKEOUT)
        {
            continue;
        }

        // 转换颜色
        let fg = convert_color(cell.cell.fg, fg_default, settings);
        let bg = if has_bg {
            Some(convert_color(cell.cell.bg, bg_color, settings))
        } else {
            None
        };

        cells.push(RenderCell {
            point,
            fg,
            bg,
            c,
            flags: cell.cell.flags,
        });
    }

    // 获取光标位置
    let cursor = content.cursor;
    let cursor_point = cursor.point;

    // 构建渲染元素
    let theme_bg = crate::theme::sidebar_color(cx);

    div()
        .size_full()
        .bg(bg_color)
        .relative()
        .overflow_hidden()
        // 渲染单元格
        .children(cells.into_iter().map(|cell| {
            let x = cell.point.column.0 as f32 * size.cell_width;
            let y = cell.point.line.0 as f32 * size.line_height;

            let mut cell_div = div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(bounds.cell_width)
                .h(bounds.line_height)
                .text_color(cell.fg);

            // 背景色
            if let Some(bg) = cell.bg {
                cell_div = cell_div.bg(bg);
            }

            // 下划线
            if cell.flags.contains(Flags::UNDERLINE) {
                cell_div = cell_div.border_b_1().border_color(cell.fg);
            }

            // 字符
            if cell.c != ' ' {
                cell_div = cell_div
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(settings.font_family.clone())
                    .text_size(px(settings.font_size as f32))
                    .child(cell.c.to_string());
            }

            cell_div
        }))
        // 渲染光标
        .child(render_cursor(
            cursor_point,
            &bounds,
            cursor_color,
            &settings.cursor_style,
            size,
            cursor_visible,
        ))
}

/// 转换 ANSI 颜色到 Hsla
fn convert_color(color: AnsiColor, default: Hsla, settings: &TerminalSettings) -> Hsla {
    match color {
        AnsiColor::Named(NamedColor::Foreground) => hex_to_hsla(&settings.foreground_color),
        AnsiColor::Named(NamedColor::Background) => hex_to_hsla(&settings.background_color),
        AnsiColor::Named(NamedColor::Cursor) => hex_to_hsla(&settings.cursor_color),
        AnsiColor::Named(NamedColor::Black) => ansi_indexed_color(0),
        AnsiColor::Named(NamedColor::Red) => ansi_indexed_color(1),
        AnsiColor::Named(NamedColor::Green) => ansi_indexed_color(2),
        AnsiColor::Named(NamedColor::Yellow) => ansi_indexed_color(3),
        AnsiColor::Named(NamedColor::Blue) => ansi_indexed_color(4),
        AnsiColor::Named(NamedColor::Magenta) => ansi_indexed_color(5),
        AnsiColor::Named(NamedColor::Cyan) => ansi_indexed_color(6),
        AnsiColor::Named(NamedColor::White) => ansi_indexed_color(7),
        AnsiColor::Named(NamedColor::BrightBlack) => ansi_indexed_color(8),
        AnsiColor::Named(NamedColor::BrightRed) => ansi_indexed_color(9),
        AnsiColor::Named(NamedColor::BrightGreen) => ansi_indexed_color(10),
        AnsiColor::Named(NamedColor::BrightYellow) => ansi_indexed_color(11),
        AnsiColor::Named(NamedColor::BrightBlue) => ansi_indexed_color(12),
        AnsiColor::Named(NamedColor::BrightMagenta) => ansi_indexed_color(13),
        AnsiColor::Named(NamedColor::BrightCyan) => ansi_indexed_color(14),
        AnsiColor::Named(NamedColor::BrightWhite) => ansi_indexed_color(15),
        AnsiColor::Named(NamedColor::BrightForeground) => hex_to_hsla(&settings.foreground_color),
        AnsiColor::Named(NamedColor::DimForeground) => default.opacity(0.7),
        AnsiColor::Named(NamedColor::DimBlack) => ansi_indexed_color(0).opacity(0.7),
        AnsiColor::Named(NamedColor::DimRed) => ansi_indexed_color(1).opacity(0.7),
        AnsiColor::Named(NamedColor::DimGreen) => ansi_indexed_color(2).opacity(0.7),
        AnsiColor::Named(NamedColor::DimYellow) => ansi_indexed_color(3).opacity(0.7),
        AnsiColor::Named(NamedColor::DimBlue) => ansi_indexed_color(4).opacity(0.7),
        AnsiColor::Named(NamedColor::DimMagenta) => ansi_indexed_color(5).opacity(0.7),
        AnsiColor::Named(NamedColor::DimCyan) => ansi_indexed_color(6).opacity(0.7),
        AnsiColor::Named(NamedColor::DimWhite) => ansi_indexed_color(7).opacity(0.7),
        AnsiColor::Spec(rgb) => alac_rgb_to_hsla(rgb),
        AnsiColor::Indexed(idx) => ansi_indexed_color(idx),
    }
}

/// 渲染光标
fn render_cursor(
    point: AlacPoint,
    bounds: &TerminalBounds,
    color: Hsla,
    style: &CursorStyle,
    size: &TerminalSize,
    cursor_visible: bool,
) -> Div {
    // 如果光标不可见，返回空 div
    if !cursor_visible {
        return div();
    }

    let x = point.column.0 as f32 * size.cell_width;
    let y = point.line.0 as f32 * size.line_height;

    let cursor_div = div().absolute().left(px(x)).top(px(y));

    match style {
        CursorStyle::Block => cursor_div
            .w(bounds.cell_width)
            .h(bounds.line_height)
            .bg(color.opacity(0.7)),
        CursorStyle::Underline => cursor_div
            .w(bounds.cell_width)
            .h(px(2.))
            .top(px(y + size.line_height - 2.))
            .bg(color),
        CursorStyle::Bar => cursor_div.w(px(2.)).h(bounds.line_height).bg(color),
    }
}

/// 渲染空终端（用于未连接状态）
pub fn render_empty_terminal(settings: &TerminalSettings, message: &str, cx: &App) -> Div {
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
