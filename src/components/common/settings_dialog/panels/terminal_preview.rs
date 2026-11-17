// 终端配色预览卡片：用一段示例终端输出，实时展示当前的配色 / 字体 / 光标样式 / 背景透明度。
//
// 数据来源：
// - 颜色：`settings.terminal.*_color`（颜色选择器与主题下拉都会实时写入 settings）
//   以及当前主题的 ANSI 调色板
// - 字体、字号、行高、透明度：优先读输入框里正在编辑的值（未保存也实时生效），回退到 settings
// - 光标样式、默认字重：settings

use gpui::prelude::*;
use gpui::*;
use gpui_component::input::InputState;
use gpui_component::ActiveTheme;

use crate::models::settings::{CursorStyle, FontWeight as TerminalFontWeight};
use crate::terminal::highlight::classify_line;
use crate::terminal::{hex_to_hsla, palette_for};

use super::super::SettingsDialogState;

// ANSI 调色板中示例会用到的颜色索引
const RED: usize = 1;
const GREEN: usize = 2;
const YELLOW: usize = 3;

/// 预览字号范围：卡片宽度有限，过大的字号会让示例文本溢出。
const MIN_FONT_SIZE: f32 = 10.0;
const MAX_FONT_SIZE: f32 = 18.0;
/// 预览行高系数范围
const MIN_LINE_HEIGHT: f32 = 1.0;
const MAX_LINE_HEIGHT: f32 = 2.0;
/// 与真实终端一致：光标以 80% 不透明度绘制，下划线/竖线光标粗 2px。
const CURSOR_OPACITY: f32 = 0.8;
const CURSOR_THICKNESS: f32 = 2.0;

/// 预览用到的颜色
struct PreviewColors {
    foreground: Hsla,
    selection: Hsla,
    ansi: [Hsla; 16],
    /// 是否启用关键字高亮（与真实终端同一开关）
    highlight: bool,
}

/// 一段同色文本，可带背景色（用于选区高亮）。
struct Seg {
    text: String,
    color: Hsla,
    background: Option<Hsla>,
}

impl Seg {
    fn new(text: impl Into<String>, color: Hsla) -> Self {
        Self {
            text: text.into(),
            color,
            background: None,
        }
    }

    fn on(mut self, background: Hsla) -> Self {
        self.background = Some(background);
        self
    }
}

/// 读取输入框当前的文本；输入框尚未创建时返回 None。
fn read_input(input: &Option<Entity<InputState>>, cx: &App) -> Option<String> {
    input
        .as_ref()
        .map(|input| input.read(cx).value().to_string())
}

/// 读取输入框当前的数值。用户输入过程中可能是空串、半截小数，甚至 `nan` / `inf`，
/// 这些都按"没有有效值"处理，由调用方回退到 settings。
fn read_number(input: &Option<Entity<InputState>>, cx: &App) -> Option<f32> {
    read_input(input, cx)?
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
}

/// 渲染终端配色预览卡片。
pub fn render_terminal_preview(state: &SettingsDialogState, cx: &App) -> impl IntoElement {
    let terminal = &state.settings.terminal;

    // 字体/行高/透明度：输入框里正在编辑的值优先
    let font_family = read_input(&state.terminal_font_family_input, cx)
        .filter(|family| !family.trim().is_empty())
        .unwrap_or_else(|| terminal.font_family.clone());
    let font_size = read_number(&state.terminal_font_size_input, cx)
        .unwrap_or(terminal.font_size as f32)
        .clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
    let line_height = font_size
        * read_number(&state.terminal_line_height_input, cx)
            .unwrap_or(terminal.line_height)
            .clamp(MIN_LINE_HEIGHT, MAX_LINE_HEIGHT);
    let opacity = read_number(&state.terminal_background_opacity_input, cx)
        .unwrap_or(terminal.background_opacity as f32)
        .clamp(0.0, 100.0)
        / 100.0;
    let font_weight = match terminal.font_weight {
        TerminalFontWeight::Normal => FontWeight::NORMAL,
        TerminalFontWeight::Bold => FontWeight::BOLD,
    };

    // 颜色
    let palette = palette_for(&terminal.color_scheme);
    let colors = PreviewColors {
        foreground: hex_to_hsla(&terminal.foreground_color),
        selection: hex_to_hsla(&terminal.selection_color),
        ansi: std::array::from_fn(|i| palette.ansi_hsla(i as u8)),
        highlight: terminal.keyword_highlight,
    };
    // 与真实终端一致：透明度只作用于背景
    let background = hex_to_hsla(&terminal.background_color).opacity(opacity);
    let cursor = hex_to_hsla(&terminal.cursor_color).opacity(CURSOR_OPACITY);
    let fg = colors.foreground;
    let border = fg.opacity(0.12);

    div()
        .w_full()
        .flex()
        .flex_col()
        .rounded_lg()
        .border_1()
        .border_color(cx.theme().border)
        .overflow_hidden()
        .bg(background)
        .child(render_title_bar(&terminal.color_scheme, &colors, border))
        .child(
            div()
                .flex()
                .flex_col()
                .px_3()
                .py_2()
                .font_family(font_family)
                .text_size(px(font_size))
                .line_height(px(line_height))
                .font_weight(font_weight)
                .text_color(fg)
                // 示例文本按单行排版，窄了就裁掉，不要折行
                .whitespace_nowrap()
                .children(render_sample_lines(&colors))
                .child(render_cursor_line(&colors, &terminal.cursor_style, cursor))
                .child(
                    div()
                        .mt_2()
                        .child(render_ansi_strip(&colors.ansi[..8], border)),
                )
                .child(
                    div()
                        .mt_1()
                        .child(render_ansi_strip(&colors.ansi[8..], border)),
                ),
        )
}

/// 标题栏：三个圆点 + 主题名
fn render_title_bar(scheme: &str, colors: &PreviewColors, border: Hsla) -> impl IntoElement {
    let fg = colors.foreground;
    let dot = |color: Hsla| div().size(px(10.)).rounded_full().bg(color);
    // 左右两侧占位等宽，让主题名真正居中
    let side_width = px(10. * 3. + 8. * 2.);

    div()
        .flex()
        .items_center()
        .h(px(28.))
        .px_3()
        .bg(fg.opacity(0.06))
        .border_b_1()
        .border_color(border)
        .child(
            div()
                .w(side_width)
                .flex()
                .items_center()
                .gap_2()
                .child(dot(colors.ansi[RED]))
                .child(dot(colors.ansi[YELLOW]))
                .child(dot(colors.ansi[GREEN])),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_xs()
                .text_color(fg.opacity(0.6))
                .child(scheme.to_string()),
        )
        .child(div().w(side_width))
}

/// 把一行示例文本按真实终端的关键字规则切成同色片段；`selected` 为被选中的字符范围。
fn text_segs(
    text: &str,
    colors: &PreviewColors,
    selected: Option<std::ops::Range<usize>>,
) -> Vec<Seg> {
    let chars: Vec<char> = text.chars().collect();
    let mut slots: Vec<Option<u8>> = vec![None; chars.len()];
    if colors.highlight {
        for (range, slot) in classify_line(text) {
            for s in &mut slots[range] {
                *s = Some(slot);
            }
        }
    }
    let key = |i: usize| (slots[i], selected.as_ref().is_some_and(|r| r.contains(&i)));

    let mut segs = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let k = key(start);
        let mut end = start + 1;
        while end < chars.len() && key(end) == k {
            end += 1;
        }
        let color = k.0.map_or(colors.foreground, |i| colors.ansi[i as usize]);
        let seg = Seg::new(chars[start..end].iter().collect::<String>(), color);
        segs.push(if k.1 { seg.on(colors.selection) } else { seg });
        start = end;
    }
    segs
}

/// 示例终端输出（光标所在的最后一行单独渲染）
fn render_sample_lines(colors: &PreviewColors) -> Vec<Div> {
    let line = |text: &str| render_line(text_segs(text, colors, None));
    let echo = "user@server:~$ echo \"hello\"";
    // 被选中的文本带选区背景色
    let selected = echo.find("\"hello\"").map(|b| {
        let start = echo[..b].chars().count();
        start..start + "\"hello\"".chars().count()
    });

    vec![
        line("user@server:~$ ls -F /etc"),
        line("hosts  nginx/  ssh/  passwd"),
        line("user@server:~$ ping -c1 8.8.8.8"),
        line("64 bytes from 8.8.8.8: time=12 ms"),
        line("ssh.service: active (running)"),
        line("warning: deprecated option"),
        line("ERROR: disk full"),
        render_line(text_segs(echo, colors, selected)),
    ]
}

/// 带光标的最后一行提示符
fn render_cursor_line(colors: &PreviewColors, style: &CursorStyle, cursor: Hsla) -> Div {
    render_line(text_segs("user@server:~$ ", colors, None)).child(render_cursor(style, cursor))
}

/// 光标：占一个字符单元格的宽度，按光标样式绘制。
fn render_cursor(style: &CursorStyle, color: Hsla) -> Div {
    // 一个空格的宽度就是一个字符单元格，和真实终端光标的占位一致
    let cell = div().relative().flex_shrink_0().child(" ");
    match style {
        CursorStyle::Block => cell.bg(color),
        CursorStyle::Bar => cell.child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .h_full()
                .w(px(CURSOR_THICKNESS))
                .bg(color),
        ),
        CursorStyle::Underline => cell.child(
            div()
                .absolute()
                .bottom_0()
                .left_0()
                .w_full()
                .h(px(CURSOR_THICKNESS))
                .bg(color),
        ),
    }
}

/// 一行由若干 span 组成的终端文本
fn render_line(segs: Vec<Seg>) -> Div {
    div()
        .flex()
        .flex_row()
        .children(segs.into_iter().map(|seg| {
            div()
                .flex_shrink_0()
                .text_color(seg.color)
                .when_some(seg.background, |this, background| this.bg(background))
                .child(seg.text)
        }))
}

/// 一排 ANSI 色块。每块带淡边框，避免与背景同色的颜色（如 ANSI 0）看不见。
fn render_ansi_strip(colors: &[Hsla], border: Hsla) -> impl IntoElement {
    div()
        .flex()
        .gap_0p5()
        .h(px(10.))
        .children(colors.iter().map(|&color| {
            div()
                .flex_1()
                .h_full()
                .rounded_sm()
                .border_1()
                .border_color(border)
                .bg(color)
        }))
}
