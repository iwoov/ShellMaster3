// 线程安全的终端屏幕快照读取（供后台 AI Agent 工具直接调用）

use std::sync::Arc;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::Term;

use super::EventProxy;

/// 可跨线程共享的底层 alacritty 终端句柄。
pub type SharedTerm = Arc<FairMutex<Term<EventProxy>>>;

/// 终端屏幕纯文本快照。
#[derive(Clone, Debug)]
pub struct TerminalSnapshot {
    /// 截取并去除行尾空白的纯文本内容
    pub text: String,
    /// 当前是否处于全屏/备选缓冲区（如 vim / top / less / htop）
    pub is_alt_screen: bool,
    /// 当前可见行数
    pub screen_lines: usize,
    /// 含 scrollback 的总行数
    pub total_lines: usize,
}

/// 从 `SharedTerm` 读取终端纯文本快照。
///
/// - `max_lines`: 最多保留末尾多少行（0 表示不限，调用方通常限制在 80~300）。
/// - `include_scrollback`: 为 true 且不处于 alt-screen 时，向上包含 scrollback 历史行。
pub fn read_term_snapshot(
    term: &SharedTerm,
    max_lines: usize,
    include_scrollback: bool,
) -> TerminalSnapshot {
    let guard = term.lock();
    let is_alt_screen = guard.mode().contains(TermMode::ALT_SCREEN);
    let grid = guard.grid();
    let screen_lines = grid.screen_lines();
    let total_lines = grid.total_lines();
    let cols = grid.columns();

    let bottom_line = grid.bottommost_line().0;
    let topmost_line = if include_scrollback && !is_alt_screen {
        grid.topmost_line().0
    } else {
        0
    };

    // 为了避免遍历上万行 scrollback，如果指定了 max_lines，最多多取一段缓冲行用于去尾部空行。
    let start_line = if max_lines > 0 {
        let span = (max_lines as i32).saturating_add(screen_lines as i32);
        (bottom_line - span + 1).max(topmost_line)
    } else {
        topmost_line
    };

    let mut lines: Vec<String> = Vec::with_capacity((bottom_line - start_line + 1).max(0) as usize);
    for line_idx in start_line..=bottom_line {
        let row = &grid[Line(line_idx)];
        let mut buf = String::with_capacity(cols);
        for col in 0..cols {
            let cell = &row[Column(col)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            buf.push(cell.c);
        }
        let trimmed_len = buf.trim_end().len();
        buf.truncate(trimmed_len);
        lines.push(buf);
    }

    drop(guard);

    while lines.first().map(|l| l.is_empty()).unwrap_or(false) {
        lines.remove(0);
    }
    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    if max_lines > 0 && lines.len() > max_lines {
        lines = lines.split_off(lines.len() - max_lines);
    }

    TerminalSnapshot {
        text: lines.join("\n"),
        is_alt_screen,
        screen_lines,
        total_lines,
    }
}
