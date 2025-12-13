// 终端状态管理 - 封装 alacritty_terminal::Term

use std::sync::Arc;

use alacritty_terminal::event::{Event as AlacEvent, EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Column, Direction, Line, Point as AlacPoint};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi;
use alacritty_terminal::Term;
use gpui::{px, Pixels, ScrollWheelEvent, TouchPhase};

use crate::models::settings::TerminalSettings;
use crate::terminal::TerminalScrollHandle;

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
    /// 终端滚动条句柄（右侧滚动条）
    scroll_handle: TerminalScrollHandle,
    /// 触控板/滚轮累计像素，用于换算行数
    scroll_px: Pixels,
    /// 光标是否可见（用于闪烁动画）
    cursor_visible: bool,
    /// 终端显示区域在窗口中的偏移原点
    bounds_origin: (f32, f32),
}

impl TerminalState {
    const MAX_SCROLLBACK_LINES: usize = 100_000;

    /// 创建新的终端状态
    pub fn new(settings: TerminalSettings) -> Self {
        // 默认尺寸
        let size = TerminalSize::default();

        // 创建终端配置
        let mut config = TermConfig::default();
        config.scrolling_history =
            (settings.scrollback_lines as usize).min(Self::MAX_SCROLLBACK_LINES);

        // 创建终端实例
        let term = Arc::new(FairMutex::new(Term::new(config, &size, EventProxy)));
        let scroll_handle = TerminalScrollHandle::new(
            term.clone(),
            px(size.line_height),
            px(size.line_height * size.lines as f32),
        );

        // 创建 VTE 解析器
        let parser = ansi::Processor::new();

        Self {
            term,
            parser,
            size,
            settings,
            scroll_handle,
            scroll_px: px(0.),
            cursor_visible: true,
            bounds_origin: (0.0, 0.0),
        }
    }

    /// 获取终端实例的锁
    pub fn term(&self) -> &Arc<FairMutex<Term<EventProxy>>> {
        &self.term
    }

    /// 获取滚动条句柄（用于渲染右侧滚动条）
    pub fn scroll_handle(&self) -> TerminalScrollHandle {
        self.scroll_handle.clone()
    }

    /// 获取当前尺寸
    pub fn size(&self) -> &TerminalSize {
        &self.size
    }

    /// 设置终端显示区域在窗口中的偏移原点
    pub fn set_bounds_origin(&mut self, origin_x: f32, origin_y: f32) {
        self.bounds_origin = (origin_x, origin_y);
    }

    /// 获取终端显示区域在窗口中的偏移原点
    pub fn bounds_origin(&self) -> (f32, f32) {
        self.bounds_origin
    }

    /// 调整终端尺寸
    pub fn resize(&mut self, width: f32, height: f32, cell_width: f32, line_height: f32) {
        let new_size = TerminalSize::from_pixels(width, height, cell_width, line_height);

        let dimensions_changed =
            new_size.columns != self.size.columns || new_size.lines != self.size.lines;
        let metrics_changed = (new_size.cell_width - self.size.cell_width).abs() > f32::EPSILON
            || (new_size.line_height - self.size.line_height).abs() > f32::EPSILON;

        if dimensions_changed || metrics_changed {
            self.size = new_size.clone();
            self.scroll_handle
                .set_line_height(px(self.size.line_height));
        }

        self.scroll_handle.set_viewport_height(px(height));

        // 只有终端行列变化时才需要通知 alacritty 重新布局
        if dimensions_changed {
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

    pub fn term_mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    pub fn scroll_page_up(&mut self) {
        self.term.lock().scroll_display(Scroll::PageUp);
    }

    pub fn scroll_page_down(&mut self) {
        self.term.lock().scroll_display(Scroll::PageDown);
    }

    pub fn scroll_by_lines(&mut self, lines: i32) {
        if lines != 0 {
            self.term.lock().scroll_display(Scroll::Delta(lines));
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    pub fn display_offset(&self) -> usize {
        self.term.lock().grid().display_offset()
    }

    pub fn determine_scroll_lines(
        &mut self,
        e: &ScrollWheelEvent,
        scroll_multiplier: f32,
    ) -> Option<i32> {
        let line_height = px(self.size.line_height);
        match e.touch_phase {
            TouchPhase::Started => {
                self.scroll_px = px(0.);
                None
            }
            TouchPhase::Moved => {
                let old_offset = (self.scroll_px / line_height) as i32;

                self.scroll_px += e.delta.pixel_delta(line_height).y * scroll_multiplier;

                let new_offset = (self.scroll_px / line_height) as i32;

                let viewport_height = line_height * self.size.lines;
                if viewport_height > px(0.) {
                    self.scroll_px %= viewport_height;
                }

                Some(new_offset - old_offset)
            }
            TouchPhase::Ended => None,
        }
    }

    // ==================== 文本选择 API ====================

    /// 像素坐标转换为终端网格坐标
    /// 返回 (AlacPoint, Direction) - 网格点和鼠标在单元格中的位置（左/右半边）
    pub fn pixel_to_grid_point(&self, x: f32, y: f32) -> (AlacPoint, Direction) {
        let display_offset = self.display_offset();
        let cell_width = self.size.cell_width;
        let line_height = self.size.line_height;

        // 计算列号
        let mut col = (x / cell_width).floor() as usize;
        let cell_x = x.max(0.0) % cell_width;
        let half_cell = cell_width / 2.0;

        // 判断鼠标在单元格左半边还是右半边
        let mut side = if cell_x > half_cell {
            Direction::Right
        } else {
            Direction::Left
        };

        // 限制列号范围
        if col >= self.size.columns {
            col = self.size.columns.saturating_sub(1);
            side = Direction::Right;
        }

        // 计算行号（考虑滚动偏移）
        let mut line = (y / line_height).floor() as i32;
        if line >= self.size.lines as i32 {
            line = self.size.lines as i32 - 1;
            side = Direction::Right;
        } else if line < 0 {
            line = 0;
            side = Direction::Left;
        }

        // 应用滚动偏移（display_offset 是向上滚动的行数）
        let grid_line = line - display_offset as i32;

        (AlacPoint::new(Line(grid_line), Column(col)), side)
    }

    /// 开始选择（鼠标按下时调用）
    /// click_count: 1 = 简单选择, 2 = 词选择, 3 = 行选择
    pub fn start_selection(&mut self, x: f32, y: f32, click_count: usize) {
        let (point, side) = self.pixel_to_grid_point(x, y);

        let selection_type = match click_count {
            2 => SelectionType::Semantic,
            3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        };

        let selection = Selection::new(selection_type, point, side);

        let mut term = self.term.lock();
        term.selection = Some(selection);

        tracing::debug!(
            "[Terminal] Start selection: pixel=({:.1}, {:.1}) bounds=({:.1}, {:.1}) grid=({}, {}) type={:?} click_count={}",
            x, y,
            self.bounds_origin.0, self.bounds_origin.1,
            point.line.0, point.column.0,
            selection_type,
            click_count
        );
    }

    /// 更新选择（鼠标拖动时调用）
    pub fn update_selection(&mut self, x: f32, y: f32) {
        let (point, side) = self.pixel_to_grid_point(x, y);

        let mut term = self.term.lock();
        if let Some(ref mut selection) = term.selection {
            selection.update(point, side);
        }
    }

    /// 结束选择（鼠标释放时调用）
    /// 返回选中的文本（如果有）
    pub fn end_selection(&mut self) -> Option<String> {
        self.selection_to_string()
    }

    /// 清除选择
    pub fn clear_selection(&mut self) {
        let mut term = self.term.lock();
        term.selection = None;
    }

    /// 获取当前选中的文本
    pub fn selection_to_string(&self) -> Option<String> {
        let term = self.term.lock();
        term.selection_to_string()
    }

    /// 检查是否有选择
    pub fn has_selection(&self) -> bool {
        let term = self.term.lock();
        term.selection.is_some()
    }
}
