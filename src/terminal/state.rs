// 终端状态管理 - 封装 alacritty_terminal::Term

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use alacritty_terminal::event::{Event as AlacEvent, EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Column, Direction, Line, Point as AlacPoint};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi;
use alacritty_terminal::vte::ansi::{NamedColor, Rgb};
use alacritty_terminal::Term;
use gpui::{px, Pixels, ScrollWheelEvent, TouchPhase};

use crate::models::settings::{FontWeight, Osc52Policy, TerminalSettings};
use crate::terminal::colors::{ansi_indexed_rgb, hex_to_rgb, palette_for};
use crate::terminal::TerminalScrollHandle;

/// 剪贴板读取请求：应用通过 OSC 52 请求把剪贴板内容写回 PTY。
/// formatter 会把剪贴板文本转换为正确的转义序列。
pub type ClipboardLoadRequest = Arc<dyn Fn(&str) -> String + Send + Sync + 'static>;

/// 处理一段 PTY 输入后，需要终端前端/传输层执行的副作用。
#[derive(Default)]
pub struct TerminalOutput {
    /// 需要回写给远端 PTY 的字节（DA/DSR/CPR 回复、颜色/尺寸查询回复等）。
    pub pty_writes: Vec<u8>,
    /// 窗口标题变化（OSC 0/2）。
    pub title: Option<String>,
    /// 是否触发了响铃（由消费侧按 bell_style 决定视觉/声音反馈）。
    pub bell: bool,
    /// 应用请求写入系统剪贴板的文本（OSC 52 store）。
    pub clipboard_store: Option<String>,
    /// 应用请求读取系统剪贴板并回写 PTY（OSC 52 load）。
    pub clipboard_load: Vec<ClipboardLoadRequest>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_config_enforces_scrollback_cap_and_word_separators() {
        let mut settings = TerminalSettings::default();
        settings.scrollback_lines = u32::MAX;
        settings.word_separators = " ,".to_string();
        let config = TerminalState::term_config(&settings);
        assert_eq!(
            config.scrolling_history,
            TerminalState::MAX_SCROLLBACK_LINES
        );
        assert_eq!(config.semantic_escape_chars, " ,");
    }

    #[test]
    fn osc52_policy_maps_to_alacritty_security_mode() {
        let mut settings = TerminalSettings::default();
        settings.osc52_policy = Osc52Policy::Disabled;
        assert_eq!(
            TerminalState::term_config(&settings).osc52,
            alacritty_terminal::term::Osc52::Disabled
        );
        settings.osc52_policy = Osc52Policy::ReadWrite;
        assert_eq!(
            TerminalState::term_config(&settings).osc52,
            alacritty_terminal::term::Osc52::CopyPaste
        );
    }
}

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

/// 事件代理 - 把 alacritty 事件转发到 TerminalState 的接收端，
/// 由 `input()` 在解析完一段数据后统一 drain 处理。
#[derive(Clone)]
pub struct EventProxy {
    tx: Sender<AlacEvent>,
}

impl EventListener for EventProxy {
    fn send_event(&self, event: AlacEvent) {
        // 接收端与 Term 生命周期一致；发送失败只可能发生在关闭竞态，忽略即可。
        let _ = self.tx.send(event);
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
    /// 远端应用通过 DECSCUSR 显式请求的闪烁状态。
    cursor_blink_override: Option<bool>,
    /// 终端显示区域在窗口中的偏移原点
    bounds_origin: (f32, f32),
    /// alacritty 事件接收端（PtyWrite/Title/Bell/剪贴板/颜色查询等）
    event_rx: Receiver<AlacEvent>,
    /// 上次测量字体的指纹，用于检测设置变化后重新测量。
    font_fingerprint: (String, u32, f32, FontWeight),
    /// 视觉响铃闪烁状态（短暂点亮后清除）。
    bell_flash: bool,
    /// 当前搜索关键字。
    search_query: String,
    /// 搜索命中范围（buffer 坐标），用于高亮。
    search_matches: Vec<Match>,
    /// 当前定位到的命中索引。
    search_current: Option<usize>,
}

impl TerminalState {
    const MAX_SCROLLBACK_LINES: usize = 100_000;

    /// 创建新的终端状态
    pub fn new(settings: TerminalSettings) -> Self {
        // 默认尺寸
        let size = TerminalSize::default();

        // 创建终端配置
        let config = Self::term_config(&settings);

        // 创建事件通道与代理
        let (tx, event_rx) = std::sync::mpsc::channel();
        let event_proxy = EventProxy { tx };

        // 创建终端实例
        let term = Arc::new(FairMutex::new(Term::new(config, &size, event_proxy)));
        let scroll_handle = TerminalScrollHandle::new(
            term.clone(),
            px(size.line_height),
            px(size.line_height * size.lines as f32),
        );

        // 创建 VTE 解析器
        let parser = ansi::Processor::new();

        let font_fingerprint = (
            settings.font_family.clone(),
            settings.font_size,
            settings.line_height,
            settings.font_weight.clone(),
        );

        Self {
            term,
            parser,
            size,
            settings,
            scroll_handle,
            scroll_px: px(0.),
            cursor_visible: true,
            cursor_blink_override: None,
            bounds_origin: (0.0, 0.0),
            event_rx,
            font_fingerprint,
            bell_flash: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_current: None,
        }
    }

    fn term_config(settings: &TerminalSettings) -> TermConfig {
        let mut config = TermConfig::default();
        config.scrolling_history =
            (settings.scrollback_lines as usize).min(Self::MAX_SCROLLBACK_LINES);
        if !settings.word_separators.is_empty() {
            config.semantic_escape_chars = settings.word_separators.clone();
        }
        config.osc52 = match settings.osc52_policy {
            Osc52Policy::Disabled => alacritty_terminal::term::Osc52::Disabled,
            Osc52Policy::WriteOnly => alacritty_terminal::term::Osc52::OnlyCopy,
            Osc52Policy::ReadWrite => alacritty_terminal::term::Osc52::CopyPaste,
        };
        config
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

    /// 检测会影响单元格测量的字体设置是否发生变化。
    pub fn font_settings_changed(&self, settings: &TerminalSettings) -> bool {
        self.font_fingerprint
            != (
                settings.font_family.clone(),
                settings.font_size,
                settings.line_height,
                settings.font_weight.clone(),
            )
    }

    pub fn settings_changed(&self, settings: &TerminalSettings) -> bool {
        &self.settings != settings
    }

    pub fn settings(&self) -> &TerminalSettings {
        &self.settings
    }

    pub fn horizontal_padding(&self) -> f32 {
        self.settings.padding as f32
    }

    pub fn cursor_should_blink(&self) -> bool {
        self.settings.cursor_blink && self.cursor_blink_override.unwrap_or(true)
    }

    pub fn cursor_blink_interval_ms(&self) -> u32 {
        self.settings.cursor_blink_interval_ms.clamp(100, 2_000)
    }

    /// 设置视觉响铃闪烁状态。
    pub fn set_bell_flash(&mut self, on: bool) {
        self.bell_flash = on;
    }

    /// 是否处于视觉响铃闪烁状态。
    pub fn is_bell_flash(&self) -> bool {
        self.bell_flash
    }

    /// 应用最新终端设置，并刷新字体指纹（在重新测量 cell metrics 后调用）。
    pub fn apply_settings(&mut self, settings: TerminalSettings) {
        self.font_fingerprint = (
            settings.font_family.clone(),
            settings.font_size,
            settings.line_height,
            settings.font_weight.clone(),
        );
        self.term.lock().set_options(Self::term_config(&settings));
        self.settings = settings;
        if !self.cursor_should_blink() {
            self.cursor_visible = true;
        }
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
    /// 使用 VTE 解析器解析 ANSI 序列，更新终端状态，并返回需要前端/传输层
    /// 处理的副作用（回写 PTY、标题、响铃、剪贴板等）。
    #[must_use]
    pub fn input(&mut self, data: &[u8]) -> TerminalOutput {
        {
            let mut term = self.term.lock();
            self.parser.advance(&mut *term, data);
            if self.settings.scroll_on_output {
                term.scroll_display(Scroll::Bottom);
            }
        }
        self.drain_events()
    }

    /// 向终端输入字符串
    #[allow(dead_code)]
    pub fn input_str(&mut self, s: &str) -> TerminalOutput {
        self.input(s.as_bytes())
    }

    /// 处理 alacritty 在解析过程中产生的事件，转换为 `TerminalOutput`。
    fn drain_events(&mut self) -> TerminalOutput {
        let mut out = TerminalOutput::default();
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AlacEvent::PtyWrite(text) => out.pty_writes.extend_from_slice(text.as_bytes()),
                AlacEvent::Title(title) => out.title = Some(title),
                AlacEvent::ResetTitle => out.title = Some(String::new()),
                AlacEvent::ClipboardStore(_, text) => out.clipboard_store = Some(text),
                AlacEvent::ClipboardLoad(_, formatter) => out.clipboard_load.push(formatter),
                AlacEvent::ColorRequest(index, formatter) => {
                    let rgb = self.color_for_index(index);
                    out.pty_writes.extend_from_slice(formatter(rgb).as_bytes());
                }
                AlacEvent::TextAreaSizeRequest(formatter) => {
                    let reply = formatter(self.size.to_window_size());
                    out.pty_writes.extend_from_slice(reply.as_bytes());
                }
                AlacEvent::Bell => out.bell = true,
                AlacEvent::CursorBlinkingChange => {
                    self.cursor_blink_override = Some(self.term.lock().cursor_style().blinking);
                    self.cursor_visible = true;
                }
                // 以下事件当前无需处理：内容更新已通过 cx.notify() 触发重绘。
                AlacEvent::MouseCursorDirty
                | AlacEvent::Wakeup
                | AlacEvent::Exit
                | AlacEvent::ChildExit(_) => {}
            }
        }
        out
    }

    /// 为 OSC 颜色查询解析索引对应的 RGB 值。
    /// index < 256 为调色板索引；256/257/258 分别为前景/背景/光标。
    fn color_for_index(&self, index: usize) -> Rgb {
        let (r, g, b) = if index < 16 {
            palette_for(&self.settings.color_scheme).ansi[index]
        } else if index < 256 {
            ansi_indexed_rgb(index as u8)
        } else if index == NamedColor::Foreground as usize {
            hex_to_rgb(&self.settings.foreground_color)
        } else if index == NamedColor::Background as usize {
            hex_to_rgb(&self.settings.background_color)
        } else if index == NamedColor::Cursor as usize {
            hex_to_rgb(&self.settings.cursor_color)
        } else {
            hex_to_rgb(&self.settings.foreground_color)
        };
        Rgb { r, g, b }
    }

    /// 切换光标可见性（用于闪烁动画）
    pub fn toggle_cursor_visibility(&mut self) {
        if self.cursor_should_blink() {
            self.cursor_visible = !self.cursor_visible;
        } else {
            self.cursor_visible = true;
        }
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

    /// 清除 scrollback 历史（不影响当前屏幕，避免与远端 shell 失步）。
    pub fn clear_scrollback(&mut self) {
        self.term.lock().grid_mut().clear_history();
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

    // ==================== 搜索 API ====================

    /// 单次搜索最多收集的命中数量上限（避免超大 scrollback 卡顿）。
    const MAX_SEARCH_MATCHES: usize = 5000;

    /// 运行搜索：在整个缓冲区（含 scrollback）内收集所有命中并定位到第一个。
    pub fn run_search(&mut self, query: &str) {
        self.search_query = query.to_string();
        self.search_matches.clear();
        self.search_current = None;

        if query.is_empty() {
            return;
        }
        let mut regex = match RegexSearch::new(query) {
            Ok(r) => r,
            Err(_) => return, // 非法正则/模式：视为无命中
        };

        let last_col = Column(self.size.columns.saturating_sub(1));
        let bottom = Line(self.size.lines as i32 - 1);
        {
            let term = self.term.lock();
            let top = term.grid().topmost_line();
            let start = AlacPoint::new(top, Column(0));
            let end = AlacPoint::new(bottom, last_col);
            let iter = RegexIter::new(start, end, Direction::Right, &term, &mut regex);
            for m in iter.take(Self::MAX_SEARCH_MATCHES) {
                self.search_matches.push(m);
            }
        }

        if !self.search_matches.is_empty() {
            self.search_current = Some(0);
            self.scroll_to_current_match();
        }
    }

    /// 定位到下一个命中。
    pub fn search_next_match(&mut self) {
        let n = self.search_matches.len();
        if n == 0 {
            return;
        }
        let cur = self.search_current.unwrap_or(0);
        self.search_current = Some((cur + 1) % n);
        self.scroll_to_current_match();
    }

    /// 定位到上一个命中。
    pub fn search_prev_match(&mut self) {
        let n = self.search_matches.len();
        if n == 0 {
            return;
        }
        let cur = self.search_current.unwrap_or(0);
        self.search_current = Some((cur + n - 1) % n);
        self.scroll_to_current_match();
    }

    /// 清除搜索状态。
    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.search_matches.clear();
        self.search_current = None;
    }

    /// 命中数量。
    pub fn search_match_count(&self) -> usize {
        self.search_matches.len()
    }

    /// 当前命中序号（1-based，无命中返回 0）。
    pub fn search_current_number(&self) -> usize {
        self.search_current.map(|i| i + 1).unwrap_or(0)
    }

    /// 命中范围切片（用于渲染高亮）。
    pub fn search_matches(&self) -> &[Match] {
        &self.search_matches
    }

    /// 当前命中索引（用于区分高亮当前项）。
    pub fn search_current_index(&self) -> Option<usize> {
        self.search_current
    }

    /// 把显示滚动到当前命中处。
    fn scroll_to_current_match(&mut self) {
        if let Some(idx) = self.search_current {
            if let Some(m) = self.search_matches.get(idx) {
                let point = *m.start();
                self.term.lock().scroll_to_point(point);
            }
        }
    }

    /// 全选（含 scrollback 历史）。
    pub fn select_all(&mut self) {
        let columns = self.size.columns.saturating_sub(1);
        let bottom_line = self.size.lines as i32 - 1;
        let mut term = self.term.lock();
        let top_line = term.grid().topmost_line();
        let start = AlacPoint::new(top_line, Column(0));
        let end = AlacPoint::new(Line(bottom_line), Column(columns));
        let mut selection = Selection::new(SelectionType::Simple, start, Direction::Left);
        selection.update(end, Direction::Right);
        term.selection = Some(selection);
    }

    /// 获取当前选中的文本
    pub fn selection_to_string(&self) -> Option<String> {
        let term = self.term.lock();
        term.selection_to_string()
    }

    /// 获取用于复制的选中文本，应用终端复制相关设置。
    pub fn selected_text_for_copy(&self) -> Option<String> {
        let text = self.selection_to_string()?;
        if !self.settings.trim_trailing_whitespace {
            return Some(text);
        }

        let mut result = String::with_capacity(text.len());
        for (idx, line) in text.split('\n').enumerate() {
            if idx > 0 {
                result.push('\n');
            }
            result.push_str(line.trim_end());
        }
        Some(result)
    }

    /// 检查是否有选择
    pub fn has_selection(&self) -> bool {
        let term = self.term.lock();
        term.selection.is_some()
    }

    /// 导出可见区域文本（最多保留最后 max_lines 行，去除行尾空白与首尾空行）。
    /// 用于把终端最近输出作为上下文发给 AI。
    pub fn visible_text(&self, max_lines: usize) -> String {
        use alacritty_terminal::term::cell::Flags;
        let term = self.term.lock();
        let content = term.renderable_content();

        let mut lines: Vec<String> = Vec::new();
        let mut cur_line: i32 = i32::MIN;
        let mut buf = String::new();
        for indexed in content.display_iter {
            // 跳过宽字符占位符，避免重复字符
            if indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            let line = indexed.point.line.0;
            if line != cur_line {
                if cur_line != i32::MIN {
                    lines.push(std::mem::take(&mut buf));
                }
                cur_line = line;
            }
            buf.push(indexed.cell.c);
        }
        if cur_line != i32::MIN {
            lines.push(buf);
        }

        // 去除每行行尾空白
        for l in lines.iter_mut() {
            let trimmed_len = l.trim_end().len();
            l.truncate(trimmed_len);
        }
        // 去除首尾空行
        while lines.first().map(|l| l.is_empty()).unwrap_or(false) {
            lines.remove(0);
        }
        while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
            lines.pop();
        }
        // 仅保留最后 max_lines 行
        if max_lines > 0 && lines.len() > max_lines {
            lines = lines.split_off(lines.len() - max_lines);
        }

        lines.join("\n")
    }
}
