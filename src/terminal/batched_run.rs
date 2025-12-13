// 批量文本运行 - Zed 风格的终端渲染优化
// 将相邻同样式的单元格合并为文本运行，减少绘制调用

use gpui::{
    point, px, size, App, Bounds, Font, FontStyle, FontWeight, Hsla, Pixels, Point, Size,
    StrikethroughStyle, TextRun, UnderlineStyle, Window,
};

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};
use alacritty_terminal::Term;

use crate::models::settings::TerminalSettings;
use crate::terminal::colors::{alac_rgb_to_hsla, ansi_indexed_color, hex_to_hsla};
use crate::terminal::state::EventProxy;

/// 批量文本运行 - 合并相邻同样式的单元格
#[derive(Debug, Clone)]
pub struct BatchedTextRun {
    /// 行号
    pub line: i32,
    /// 起始列
    pub start_col: i32,
    /// 合并后的文本
    pub text: String,
    /// 单元格数量
    pub cell_count: usize,
    /// 前景色
    pub fg_color: Hsla,
    /// 字体粗细
    pub font_weight: FontWeight,
    /// 字体样式
    pub font_style: FontStyle,
    /// 是否有下划线
    pub underline: bool,
    /// 是否有删除线
    pub strikethrough: bool,
}

impl BatchedTextRun {
    /// 创建新的批次
    fn new(line: i32, col: i32, c: char, fg: Hsla, flags: Flags) -> Self {
        let mut text = String::with_capacity(80);
        text.push(c);

        Self {
            line,
            start_col: col,
            text,
            cell_count: 1,
            fg_color: fg,
            font_weight: if flags.contains(Flags::BOLD) {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            },
            font_style: if flags.contains(Flags::ITALIC) {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            strikethrough: flags.contains(Flags::STRIKEOUT),
        }
    }

    /// 检查是否可以追加（样式匹配且位置连续）
    fn can_append(&self, line: i32, col: i32, fg: Hsla, flags: Flags) -> bool {
        if self.line != line {
            return false;
        }
        if self.start_col + self.cell_count as i32 != col {
            return false;
        }
        if self.fg_color != fg {
            return false;
        }

        let weight = if flags.contains(Flags::BOLD) {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        };
        let style = if flags.contains(Flags::ITALIC) {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };
        let underline = flags.intersects(Flags::ALL_UNDERLINES);
        let strikethrough = flags.contains(Flags::STRIKEOUT);

        self.font_weight == weight
            && self.font_style == style
            && self.underline == underline
            && self.strikethrough == strikethrough
    }

    /// 追加字符
    fn append(&mut self, c: char) {
        self.text.push(c);
        self.cell_count += 1;
    }

    /// 绘制文本运行
    pub fn paint(
        &self,
        origin: Point<Pixels>,
        cell_width: f32,
        line_height: f32,
        font_family: String,
        font_size: f32,
        window: &mut Window,
        cx: &mut App,
    ) {
        let pos = point(
            origin.x + px(self.start_col as f32 * cell_width),
            origin.y + px(self.line as f32 * line_height),
        );

        let underline = self.underline.then(|| UnderlineStyle {
            color: Some(self.fg_color),
            thickness: px(1.0),
            wavy: false,
        });

        let strikethrough = self.strikethrough.then(|| StrikethroughStyle {
            color: Some(self.fg_color),
            thickness: px(1.0),
        });

        let text_run = TextRun {
            len: self.text.len(),
            font: Font {
                family: font_family.into(),
                features: Default::default(),
                weight: self.font_weight,
                style: self.font_style,
                fallbacks: None,
            },
            color: self.fg_color,
            background_color: None,
            underline,
            strikethrough,
        };

        let shaped_line = window.text_system().shape_line(
            self.text.clone().into(),
            px(font_size),
            &[text_run],
            Some(px(cell_width)),
        );

        let _ = shaped_line.paint(pos, px(line_height), window, cx);
    }
}

/// 背景矩形 - 合并相邻同色背景
#[derive(Debug, Clone)]
pub struct BackgroundRect {
    /// 行号
    pub line: i32,
    /// 起始列
    pub start_col: i32,
    /// 结束列（包含）
    pub end_col: i32,
    /// 背景色
    pub color: Hsla,
}

impl BackgroundRect {
    /// 创建新的背景矩形
    fn new(line: i32, col: i32, color: Hsla) -> Self {
        Self {
            line,
            start_col: col,
            end_col: col,
            color,
        }
    }

    /// 检查是否可以扩展
    fn can_extend(&self, line: i32, col: i32, color: Hsla) -> bool {
        self.line == line && self.end_col + 1 == col && self.color == color
    }

    /// 扩展矩形
    fn extend(&mut self) {
        self.end_col += 1;
    }

    /// 绘制背景矩形
    pub fn paint(
        &self,
        origin: Point<Pixels>,
        cell_width: f32,
        line_height: f32,
        window: &mut Window,
    ) {
        let pos = point(
            origin.x + px(self.start_col as f32 * cell_width),
            origin.y + px(self.line as f32 * line_height),
        );
        let rect_size: Size<Pixels> = size(
            px((self.end_col - self.start_col + 1) as f32 * cell_width),
            px(line_height),
        );
        window.paint_quad(gpui::fill(Bounds::new(pos, rect_size), self.color));
    }
}

/// 布局结果
#[derive(Clone)]
pub struct LayoutResult {
    /// 文本运行批次
    pub text_runs: Vec<BatchedTextRun>,
    /// 背景矩形
    pub background_rects: Vec<BackgroundRect>,
    /// 选择高亮矩形
    pub selection_rects: Vec<BackgroundRect>,
}

/// 布局网格 - 将终端单元格转换为批量文本运行和背景矩形
pub fn layout_grid(term: &Term<EventProxy>, settings: &TerminalSettings) -> LayoutResult {
    let content = term.renderable_content();
    let display_offset = content.display_offset as i32;

    let fg_default = hex_to_hsla(&settings.foreground_color);
    let bg_default = hex_to_hsla(&settings.background_color);
    let selection_color = hex_to_hsla(&settings.selection_color);

    // 获取选择范围
    let selection = content.selection;

    let mut text_runs: Vec<BatchedTextRun> = Vec::with_capacity(200);
    let mut background_rects: Vec<BackgroundRect> = Vec::with_capacity(100);
    let mut selection_rects: Vec<BackgroundRect> = Vec::with_capacity(50);
    let mut current_run: Option<BatchedTextRun> = None;

    let mut cell_count = 0;

    for indexed in content.display_iter {
        let point = indexed.point;
        let display_line = point.line.0 + display_offset;
        let col = point.column.0 as i32;
        let cell = indexed.cell;
        let flags = cell.flags;

        cell_count += 1;

        // 跳过宽字符占位符
        if flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }

        let c = cell.c;

        // 检查是否在选择范围内
        let is_selected = if let Some(ref sel) = selection {
            point >= sel.start && point <= sel.end
        } else {
            false
        };

        // 如果被选中，添加选择高亮矩形
        if is_selected {
            if let Some(ref mut last_rect) = selection_rects.last_mut() {
                if last_rect.can_extend(display_line, col, selection_color) {
                    last_rect.extend();
                } else {
                    selection_rects.push(BackgroundRect::new(display_line, col, selection_color));
                }
            } else {
                selection_rects.push(BackgroundRect::new(display_line, col, selection_color));
            }
        }

        // 处理颜色反转
        let (fg, bg) = if flags.contains(Flags::INVERSE) {
            (cell.bg, cell.fg)
        } else {
            (cell.fg, cell.bg)
        };

        // 转换颜色
        let fg_color = convert_color(fg, fg_default, settings);
        let bg_color = convert_color(bg, bg_default, settings);

        // 处理背景（非默认背景才需要绘制）
        let has_bg = !matches!(bg, AnsiColor::Named(NamedColor::Background));
        if has_bg {
            if let Some(ref mut last_rect) = background_rects.last_mut() {
                if last_rect.can_extend(display_line, col, bg_color) {
                    last_rect.extend();
                } else {
                    background_rects.push(BackgroundRect::new(display_line, col, bg_color));
                }
            } else {
                background_rects.push(BackgroundRect::new(display_line, col, bg_color));
            }
        }

        // 跳过空白字符（除非有特殊标记）
        if c == ' ' && !flags.intersects(Flags::UNDERLINE | Flags::STRIKEOUT) {
            // 刷新当前批次
            if let Some(run) = current_run.take() {
                text_runs.push(run);
            }
            continue;
        }

        // 尝试追加到当前批次
        if let Some(ref mut run) = current_run {
            if run.can_append(display_line, col, fg_color, flags) {
                run.append(c);
            } else {
                // 刷新当前批次，开始新批次
                let old_run = current_run.take().unwrap();
                text_runs.push(old_run);
                current_run = Some(BatchedTextRun::new(display_line, col, c, fg_color, flags));
            }
        } else {
            // 开始新批次
            current_run = Some(BatchedTextRun::new(display_line, col, c, fg_color, flags));
        }
    }

    // 刷新最后的批次
    if let Some(run) = current_run {
        text_runs.push(run);
    }

    tracing::trace!(
        "layout_grid: {} cells → {} runs, {} bg_rects, {} sel_rects",
        cell_count,
        text_runs.len(),
        background_rects.len(),
        selection_rects.len()
    );

    LayoutResult {
        text_runs,
        background_rects,
        selection_rects,
    }
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
