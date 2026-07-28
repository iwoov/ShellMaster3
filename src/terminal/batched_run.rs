// 批量文本运行 - Zed 风格的终端渲染优化
// 将相邻同样式的单元格合并为文本运行，减少绘制调用

use gpui::{
    point, px, size, App, Bounds, Font, FontStyle, FontWeight, Hsla, Pixels, Point, Size,
    StrikethroughStyle, TextRun, UnderlineStyle, Window,
};

use gpui::FontFeatures;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::Match;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};
use alacritty_terminal::Term;

use crate::models::settings::TerminalSettings;
use crate::terminal::colors::{
    alac_rgb_to_hsla, ansi_indexed_color, hex_to_hsla, palette_for, TerminalPalette,
};
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

/// 依据单元格 BOLD 标记与用户默认字重决定字重。
fn weight_for(flags: Flags, default_bold: bool) -> FontWeight {
    if flags.contains(Flags::BOLD) || default_bold {
        FontWeight::BOLD
    } else {
        FontWeight::NORMAL
    }
}

impl BatchedTextRun {
    /// 创建新的批次
    fn new(line: i32, col: i32, c: char, fg: Hsla, flags: Flags, default_bold: bool) -> Self {
        let mut text = String::with_capacity(80);
        text.push(c);

        Self {
            line,
            start_col: col,
            text,
            cell_count: 1,
            fg_color: fg,
            font_weight: weight_for(flags, default_bold),
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
    fn can_append(&self, line: i32, col: i32, fg: Hsla, flags: Flags, default_bold: bool) -> bool {
        if self.line != line {
            return false;
        }
        if self.start_col + self.cell_count as i32 != col {
            return false;
        }
        if self.fg_color != fg {
            return false;
        }

        let weight = weight_for(flags, default_bold);
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
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &self,
        origin: Point<Pixels>,
        cell_width: f32,
        line_height: f32,
        font_family: String,
        font_size: f32,
        ligatures: bool,
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

        let features = if ligatures {
            FontFeatures::default()
        } else {
            FontFeatures::disable_ligatures()
        };
        let text_run = TextRun {
            len: self.text.len(),
            font: Font {
                family: font_family.into(),
                features,
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
    /// 搜索命中高亮矩形（当前命中与其他命中用不同颜色）
    pub search_rects: Vec<BackgroundRect>,
}

/// 布局网格 - 将终端单元格转换为批量文本运行和背景矩形
pub fn layout_grid(
    term: &Term<EventProxy>,
    settings: &TerminalSettings,
    search_matches: &[Match],
    search_current: Option<usize>,
) -> LayoutResult {
    let content = term.renderable_content();
    let display_offset = content.display_offset as i32;

    // 搜索高亮：预筛选可见范围内的命中，降低逐格判定成本
    let screen_lines = term.grid().screen_lines() as i32;
    let vis_top = -display_offset;
    let vis_bot = screen_lines - 1 - display_offset;
    let visible_matches: Vec<&Match> = search_matches
        .iter()
        .filter(|m| m.end().line.0 >= vis_top && m.start().line.0 <= vis_bot)
        .collect();
    let current_match: Option<&Match> = search_current.and_then(|i| search_matches.get(i));
    let match_color = hex_to_hsla("#ffd54f").opacity(0.35);
    let current_match_color = hex_to_hsla("#ff9800").opacity(0.55);
    let mut search_rects: Vec<BackgroundRect> = Vec::new();

    let palette = palette_for(&settings.color_scheme);
    let default_bold = matches!(
        settings.font_weight,
        crate::models::settings::FontWeight::Bold
    );
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

        // 检查是否在选择范围内（块选择按矩形区域判定）
        let is_selected = if let Some(ref sel) = selection {
            if sel.is_block {
                point.line >= sel.start.line
                    && point.line <= sel.end.line
                    && point.column >= sel.start.column
                    && point.column <= sel.end.column
            } else {
                point >= sel.start && point <= sel.end
            }
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

        // 搜索命中高亮（当前命中用更醒目的颜色）
        if !visible_matches.is_empty() {
            let is_current = current_match.map_or(false, |m| m.contains(&point));
            let in_match = is_current || visible_matches.iter().any(|m| m.contains(&point));
            if in_match {
                let color = if is_current {
                    current_match_color
                } else {
                    match_color
                };
                if let Some(last_rect) = search_rects.last_mut() {
                    if last_rect.can_extend(display_line, col, color) {
                        last_rect.extend();
                    } else {
                        search_rects.push(BackgroundRect::new(display_line, col, color));
                    }
                } else {
                    search_rects.push(BackgroundRect::new(display_line, col, color));
                }
            }
        }

        // 处理颜色反转
        let (fg, bg) = if flags.contains(Flags::INVERSE) {
            (cell.bg, cell.fg)
        } else {
            (cell.fg, cell.bg)
        };

        // 转换颜色
        let fg_color = convert_color(fg, fg_default, settings, &palette);
        let bg_color = convert_color(bg, bg_default, settings, &palette);

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
            if run.can_append(display_line, col, fg_color, flags, default_bold) {
                run.append(c);
            } else {
                // 刷新当前批次，开始新批次
                let old_run = current_run.take().unwrap();
                text_runs.push(old_run);
                current_run =
                    Some(BatchedTextRun::new(display_line, col, c, fg_color, flags, default_bold));
            }
        } else {
            // 开始新批次
            current_run =
                Some(BatchedTextRun::new(display_line, col, c, fg_color, flags, default_bold));
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
        search_rects,
    }
}

/// 转换 ANSI 颜色到 Hsla。0-15 号色取自主题调色板；256 色立方体走计算。
fn convert_color(
    color: AnsiColor,
    default: Hsla,
    settings: &TerminalSettings,
    palette: &TerminalPalette,
) -> Hsla {
    match color {
        AnsiColor::Named(NamedColor::Foreground) => hex_to_hsla(&settings.foreground_color),
        AnsiColor::Named(NamedColor::Background) => hex_to_hsla(&settings.background_color),
        AnsiColor::Named(NamedColor::Cursor) => hex_to_hsla(&settings.cursor_color),
        AnsiColor::Named(NamedColor::Black) => palette.ansi_hsla(0),
        AnsiColor::Named(NamedColor::Red) => palette.ansi_hsla(1),
        AnsiColor::Named(NamedColor::Green) => palette.ansi_hsla(2),
        AnsiColor::Named(NamedColor::Yellow) => palette.ansi_hsla(3),
        AnsiColor::Named(NamedColor::Blue) => palette.ansi_hsla(4),
        AnsiColor::Named(NamedColor::Magenta) => palette.ansi_hsla(5),
        AnsiColor::Named(NamedColor::Cyan) => palette.ansi_hsla(6),
        AnsiColor::Named(NamedColor::White) => palette.ansi_hsla(7),
        AnsiColor::Named(NamedColor::BrightBlack) => palette.ansi_hsla(8),
        AnsiColor::Named(NamedColor::BrightRed) => palette.ansi_hsla(9),
        AnsiColor::Named(NamedColor::BrightGreen) => palette.ansi_hsla(10),
        AnsiColor::Named(NamedColor::BrightYellow) => palette.ansi_hsla(11),
        AnsiColor::Named(NamedColor::BrightBlue) => palette.ansi_hsla(12),
        AnsiColor::Named(NamedColor::BrightMagenta) => palette.ansi_hsla(13),
        AnsiColor::Named(NamedColor::BrightCyan) => palette.ansi_hsla(14),
        AnsiColor::Named(NamedColor::BrightWhite) => palette.ansi_hsla(15),
        AnsiColor::Named(NamedColor::BrightForeground) => hex_to_hsla(&settings.foreground_color),
        AnsiColor::Named(NamedColor::DimForeground) => default.opacity(0.7),
        AnsiColor::Named(NamedColor::DimBlack) => palette.ansi_hsla(0).opacity(0.7),
        AnsiColor::Named(NamedColor::DimRed) => palette.ansi_hsla(1).opacity(0.7),
        AnsiColor::Named(NamedColor::DimGreen) => palette.ansi_hsla(2).opacity(0.7),
        AnsiColor::Named(NamedColor::DimYellow) => palette.ansi_hsla(3).opacity(0.7),
        AnsiColor::Named(NamedColor::DimBlue) => palette.ansi_hsla(4).opacity(0.7),
        AnsiColor::Named(NamedColor::DimMagenta) => palette.ansi_hsla(5).opacity(0.7),
        AnsiColor::Named(NamedColor::DimCyan) => palette.ansi_hsla(6).opacity(0.7),
        AnsiColor::Named(NamedColor::DimWhite) => palette.ansi_hsla(7).opacity(0.7),
        AnsiColor::Spec(rgb) => alac_rgb_to_hsla(rgb),
        AnsiColor::Indexed(idx) if idx < 16 => palette.ansi_hsla(idx),
        AnsiColor::Indexed(idx) => ansi_indexed_color(idx),
    }
}
