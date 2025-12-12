// Terminal 面板组件 - 包含终端区域和命令输入框

use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::ActiveTheme;
use gpui_component::scroll::ScrollableElement;
use tracing::trace;

use alacritty_terminal::term::TermMode;

use crate::constants::icons;
use crate::state::{SessionState, SessionTab};
use crate::terminal::{hex_to_hsla, keystroke_to_escape, render_terminal_view, TerminalState};

/// 渲染终端面板
pub fn render_terminal_panel(
    tab: &SessionTab,
    command_input: Option<Entity<InputState>>,
    _session_state: Entity<SessionState>,
    terminal_focus_handle: Option<FocusHandle>,
    cx: &App,
) -> impl IntoElement {
    let border_color = cx.theme().border;

    // 获取终端设置
    let settings = crate::services::storage::load_settings().unwrap_or_default();
    let terminal_settings = settings.terminal.clone();

    // 获取终端状态和错误信息
    let terminal_entity = tab.terminal.clone();
    let pty_channel = tab.pty_channel.clone();
    let pty_error = tab.pty_error.clone();

    // 创建终端显示区域的基础 div
    let mut terminal_display = div()
        .id("terminal-display")
        .flex_1()
        .relative()
        .overflow_hidden()
        .cursor_text();

    // 如果有焦点句柄，添加事件监听
    if let Some(focus_handle) = terminal_focus_handle.clone() {
        let focus_for_click = focus_handle.clone();
        terminal_display = terminal_display
            .track_focus(&focus_handle)
            // 点击获取焦点
            .on_mouse_down(MouseButton::Left, move |_, window, _cx| {
                window.focus(&focus_for_click);
            });

        // 滚轮：滚动查看历史（非 ALT_SCREEN），或在 ALT_SCREEN 下发送上/下箭头模拟滚动
        if terminal_entity.is_some() {
            let terminal_for_scroll = terminal_entity.clone();
            let pty_channel_for_scroll = pty_channel.clone();
            terminal_display = terminal_display.on_scroll_wheel(move |event, _window, cx| {
                let Some(terminal) = terminal_for_scroll.clone() else {
                    return;
                };

                let mut bytes_to_send: Option<Vec<u8>> = None;
                let mut handled = false;

                terminal.update(cx, |t, cx| {
                    let Some(scroll_lines) = t.determine_scroll_lines(event, 1.0) else {
                        return;
                    };
                    if scroll_lines == 0 {
                        return;
                    }

                    let mode = t.term_mode();
                    let should_alt_scroll = mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
                        && !event.modifiers.shift;

                    if should_alt_scroll {
                        handled = true;

                        let cmd = if scroll_lines > 0 { b'A' } else { b'B' };
                        let introducer = if mode.contains(TermMode::APP_CURSOR) {
                            b'O'
                        } else {
                            b'['
                        };

                        let mut content =
                            Vec::with_capacity(scroll_lines.unsigned_abs() as usize * 3);
                        for _ in 0..scroll_lines.abs() {
                            content.push(0x1b);
                            content.push(introducer);
                            content.push(cmd);
                        }
                        bytes_to_send = Some(content);
                    } else {
                        t.scroll_by_lines(scroll_lines);
                        handled = true;
                        cx.notify();
                    }
                });

                if handled {
                    cx.stop_propagation();
                }

                if let (Some(channel), Some(bytes)) = (pty_channel_for_scroll.clone(), bytes_to_send)
                {
                    cx.spawn(async move |_async_cx| {
                        if let Err(e) = channel.write(&bytes).await {
                            tracing::error!("[Terminal] PTY write error: {:?}", e);
                        }
                    })
                    .detach();
                }
            });
        }

        // 键盘：PageUp/Down 用于滚动历史（非 ALT_SCREEN），其余按键发送到 PTY
        let terminal_for_key = terminal_entity.clone();
        let pty_channel_for_key = pty_channel.clone();
        terminal_display = terminal_display.on_key_down(move |event, _window, cx| {
            let key = event.keystroke.key.as_str();

            if matches!(key, "pageup" | "pagedown")
                && !event.keystroke.modifiers.control
                && !event.keystroke.modifiers.alt
                && !event.keystroke.modifiers.platform
                && !event.keystroke.modifiers.function
            {
                let mut handled_scroll = false;
                if let Some(terminal) = terminal_for_key.clone() {
                    terminal.update(cx, |t, cx| {
                        if !t.term_mode().contains(TermMode::ALT_SCREEN) {
                            handled_scroll = true;
                            if key == "pageup" {
                                t.scroll_page_up();
                            } else {
                                t.scroll_page_down();
                            }
                            cx.notify();
                        }
                    });
                }

                if handled_scroll {
                    cx.stop_propagation();
                    return;
                }
            }

            let Some(channel) = pty_channel_for_key.clone() else {
                return;
            };

            // 将按键转换为转义序列
            if let Some(bytes) = keystroke_to_escape(&event.keystroke, &event.keystroke.modifiers) {
                trace!(
                    "[Terminal] Key pressed: {:?}, sending {} bytes",
                    event.keystroke.key,
                    bytes.len()
                );

                // 重置光标为可见（有输入时）
                if let Some(terminal) = terminal_for_key.clone() {
                    terminal.update(cx, |t, _| {
                        t.show_cursor();
                    });
                }

                // 发送到 PTY (异步)
                cx.spawn(async move |_async_cx| {
                    if let Err(e) = channel.write(&bytes).await {
                        tracing::error!("[Terminal] PTY write error: {:?}", e);
                    }
                })
                .detach();
            }
        });
    }

    // 右侧滚动条：仅在终端正常可用时显示（需要作为最后的 child，确保绘制在最上层）
    let scroll_handle = if pty_error.is_none() {
        terminal_entity
            .as_ref()
            .map(|terminal| terminal.read(cx).scroll_handle())
    } else {
        None
    };

    // 添加终端内容
    terminal_display = terminal_display.child(if let Some(error) = pty_error {
        // PTY 失败 - 显示错误信息
        render_error_terminal(&terminal_settings, &error, cx)
    } else if let Some(terminal) = terminal_entity {
        // 有终端实例 - 渲染真实终端内容
        render_terminal_content(terminal, &terminal_settings, cx)
    } else {
        // 等待初始化 - 显示加载提示
        render_loading_terminal(&terminal_settings, cx)
    });

    if let Some(scroll_handle) = scroll_handle {
        terminal_display = terminal_display.vertical_scrollbar(&scroll_handle);
    }

    div()
        .size_full()
        .flex()
        .flex_col()
        // 终端显示区域（上方，占据大部分空间）
        .child(terminal_display)
        // 命令输入区域（下方）
        .child(render_command_input(border_color, command_input, cx))
}

/// 渲染真实终端内容
fn render_terminal_content(
    terminal: Entity<TerminalState>,
    settings: &crate::models::settings::TerminalSettings,
    cx: &App,
) -> Div {
    let state = terminal.read(cx);
    let term = state.term();
    let size = state.size();
    let cursor_visible = state.is_cursor_visible();

    // 使用 renderer 中的 render_terminal_view 函数
    render_terminal_view(&term.lock(), size, settings, cursor_visible, cx)
}

/// 渲染错误状态的终端
fn render_error_terminal(
    settings: &crate::models::settings::TerminalSettings,
    error: &str,
    _cx: &App,
) -> Div {
    let bg_color = hex_to_hsla(&settings.background_color);
    let error_color = Hsla::from(rgb(0xef4444)); // red-500

    div()
        .size_full()
        .bg(bg_color)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(svg().path(icons::X).size(px(32.)).text_color(error_color))
                .child(
                    div()
                        .text_color(error_color)
                        .text_sm()
                        .child(format!("PTY Error: {}", error)),
                ),
        )
}

/// 渲染加载中的终端
fn render_loading_terminal(settings: &crate::models::settings::TerminalSettings, _cx: &App) -> Div {
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
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(svg().path(icons::LOADER).size(px(24.)).text_color(fg_color))
                .child(
                    div()
                        .text_color(fg_color.opacity(0.6))
                        .text_sm()
                        .child("Initializing terminal..."),
                ),
        )
}

/// 渲染命令输入区域
fn render_command_input(
    border_color: Hsla,
    command_input: Option<Entity<InputState>>,
    cx: &App,
) -> impl IntoElement {
    let primary = cx.theme().primary;

    div()
        .id("command-input-area")
        .flex_shrink_0()
        .border_t_1()
        .border_color(border_color)
        .p_1()
        .child(
            // 输入框容器
            div()
                .w_full()
                .flex()
                .items_end()
                .gap_1()
                // 输入框
                .child(
                    div().flex_1().children(
                        command_input
                            .as_ref()
                            .map(|input| Input::new(input).appearance(false)),
                    ),
                )
                // 发送按钮
                .child(
                    div()
                        .id("send-command-btn")
                        .size(px(24.))
                        .rounded(px(4.))
                        .bg(primary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |s| s.bg(primary.opacity(0.8)))
                        .child(
                            svg()
                                .path(icons::SEND)
                                .size(px(14.))
                                .text_color(gpui::white()),
                        ),
                ),
        )
}
