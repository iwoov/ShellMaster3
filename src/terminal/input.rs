// 终端文本输入/IME 桥接。
//
// 普通按键仍由 terminal_page 的 key down 处理器负责控制键和转义序列；
// 可打印文本则通过 GPUI InputHandler 进入这里。这样系统输入法可以先维护
// composition（拼音、日文假名等），只在确认候选后把最终 UTF-8 文本写入 PTY。

use std::ops::Range;
use std::sync::Arc;

use gpui::{App, Bounds, Entity, InputHandler, Pixels, Point, UTF16Selection, Window};

use crate::ssh::session::TerminalChannel;
use crate::terminal::TerminalState;

#[derive(Clone)]
pub struct TerminalInputHandler {
    terminal: Entity<TerminalState>,
    channel: Arc<TerminalChannel>,
    element_bounds: Bounds<Pixels>,
}

impl TerminalInputHandler {
    pub fn new(
        terminal: Entity<TerminalState>,
        channel: Arc<TerminalChannel>,
        element_bounds: Bounds<Pixels>,
    ) -> Self {
        Self {
            terminal,
            channel,
            element_bounds,
        }
    }

    fn send_committed_text(&self, text: &str, cx: &mut App) {
        self.terminal.update(cx, |terminal, cx| {
            terminal.clear_ime_preedit();
            terminal.show_cursor();
            cx.notify();
        });

        if text.is_empty() {
            return;
        }

        // 终端 Enter 使用 CR。正常 Enter 会由快捷键 action 处理，这个转换也能
        // 保证平台偶尔通过文本输入接口提交换行时不会发送错误的 LF。
        let bytes = if text.contains('\n') {
            text.replace('\n', "\r").into_bytes()
        } else {
            text.as_bytes().to_vec()
        };

        if let Err(error) = self.channel.queue_write(bytes) {
            tracing::error!("[Terminal] PTY write error on text input: {:?}", error);
        }
    }
}

impl InputHandler for TerminalInputHandler {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        let range = self.terminal.read(cx).ime_selected_range();
        Some(UTF16Selection {
            range,
            reversed: false,
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.terminal.read(cx).ime_marked_range()
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        let text = self
            .terminal
            .read(cx)
            .ime_text_for_range(range_utf16.clone())?;
        *adjusted_range = Some(range_utf16);
        Some(text)
    }

    fn replace_text_in_range(
        &mut self,
        _replacement_range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut App,
    ) {
        self.send_committed_text(text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) {
        self.terminal.update(cx, |terminal, cx| {
            terminal.set_ime_preedit(new_text, new_selected_range);
            terminal.show_cursor();
            cx.notify();
        });
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut App) {
        self.terminal.update(cx, |terminal, cx| {
            terminal.clear_ime_preedit();
            cx.notify();
        });
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        Some(
            self.terminal
                .read(cx)
                .ime_cursor_bounds(self.element_bounds),
        )
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        Some(0)
    }

    // 终端更需要按键连发，而不是 macOS 长按字符时的重音字符选择器。
    fn apple_press_and_hold_enabled(&mut self) -> bool {
        false
    }
}
