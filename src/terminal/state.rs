// 终端状态管理 - 封装 alacritty_terminal::Term

use std::sync::Arc;

use alacritty_terminal::event::{Event as AlacEvent, EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::vte::ansi;
use alacritty_terminal::Term;

use crate::models::settings::TerminalSettings;

/// 终端尺寸信息
#[derive(Clone, Debug)]
pub struct TerminalSize {
    /// 单元格宽度 (pixels)
    pub cell_width: f32,
    /// 行高 (pixels)
    pub line_height: f32,
    /// 列数
    pub columns: usize,
    /// 行数
    pub lines: usize,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            cell_width: 8.0,
            line_height: 16.0,
            columns: 80,
            lines: 24,
        }
    }
}

impl TerminalSize {
    /// 从像素尺寸计算终端尺寸
    pub fn from_pixels(width: f32, height: f32, cell_width: f32, line_height: f32) -> Self {
        let columns = (width / cell_width).floor() as usize;
        let lines = (height / line_height).floor() as usize;

        Self {
            cell_width,
            line_height,
            columns: columns.max(1),
            lines: lines.max(1),
        }
    }

    /// 转换为 alacritty WindowSize
    pub fn to_window_size(&self) -> WindowSize {
        WindowSize {
            num_lines: self.lines as u16,
            num_cols: self.columns as u16,
            cell_width: self.cell_width as u16,
            cell_height: self.line_height as u16,
        }
    }
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }

    fn last_column(&self) -> alacritty_terminal::index::Column {
        alacritty_terminal::index::Column(self.columns.saturating_sub(1))
    }

    fn bottommost_line(&self) -> alacritty_terminal::index::Line {
        alacritty_terminal::index::Line(self.lines as i32 - 1)
    }

    fn topmost_line(&self) -> alacritty_terminal::index::Line {
        alacritty_terminal::index::Line(0)
    }
}

/// 事件代理 - 接收终端事件
#[derive(Clone)]
pub struct EventProxy;

impl EventListener for EventProxy {
    fn send_event(&self, _event: AlacEvent) {
        // TODO: 处理终端事件（如标题变化、铃声等）
    }
}

/// 终端状态
pub struct TerminalState {
    /// alacritty 终端实例
    term: Arc<FairMutex<Term<EventProxy>>>,
    /// VTE 解析器
    parser: ansi::Processor,
    /// 当前尺寸
    size: TerminalSize,
    /// 终端设置
    #[allow(dead_code)]
    settings: TerminalSettings,
    /// 光标是否可见（用于闪烁动画）
    cursor_visible: bool,
}

impl TerminalState {
    /// 创建新的终端状态
    pub fn new(settings: TerminalSettings) -> Self {
        // 默认尺寸
        let size = TerminalSize::default();

        // 创建终端配置
        let config = TermConfig::default();

        // 创建终端实例
        let term = Term::new(config, &size, EventProxy);

        // 创建 VTE 解析器
        let parser = ansi::Processor::new();

        Self {
            term: Arc::new(FairMutex::new(term)),
            parser,
            size,
            settings,
            cursor_visible: true,
        }
    }

    /// 获取终端实例的锁
    pub fn term(&self) -> &Arc<FairMutex<Term<EventProxy>>> {
        &self.term
    }

    /// 获取当前尺寸
    pub fn size(&self) -> &TerminalSize {
        &self.size
    }

    /// 调整终端尺寸
    pub fn resize(&mut self, width: f32, height: f32, cell_width: f32, line_height: f32) {
        let new_size = TerminalSize::from_pixels(width, height, cell_width, line_height);

        // 只有尺寸变化时才更新
        if new_size.columns != self.size.columns || new_size.lines != self.size.lines {
            self.size = new_size.clone();

            // 更新终端尺寸
            let mut term = self.term.lock();
            term.resize(new_size);
        }
    }

    /// 向终端输入数据（来自 PTY）
    /// 使用 VTE 解析器解析 ANSI 序列，并更新终端状态
    pub fn input(&mut self, data: &[u8]) {
        let mut term = self.term.lock();
        self.parser.advance(&mut *term, data);
    }

    /// 向终端输入字符串
    pub fn input_str(&mut self, s: &str) {
        self.input(s.as_bytes());
    }

    /// 切换光标可见性（用于闪烁动画）
    pub fn toggle_cursor_visibility(&mut self) {
        self.cursor_visible = !self.cursor_visible;
    }

    /// 获取光标可见状态
    pub fn is_cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    /// 重置光标为可见（例如有输入时）
    pub fn show_cursor(&mut self) {
        self.cursor_visible = true;
    }
}
