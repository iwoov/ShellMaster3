// Terminal 面板组件 - 包含终端区域和命令输入框

use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::ActiveTheme;
use tracing::trace;

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

        // 如果有 PTY 通道，添加键盘事件处理
        if let Some(channel) = pty_channel.clone() {
            let terminal_for_key = terminal_entity.clone();
            terminal_display = terminal_display.on_key_down(move |event, _window, cx| {
                // 将按键转换为转义序列
                if let Some(bytes) =
                    keystroke_to_escape(&event.keystroke, &event.keystroke.modifiers)
                {
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
                    let channel_clone = channel.clone();
                    let bytes_clone = bytes.clone();
                    cx.spawn(async move |_async_cx| {
                        if let Err(e) = channel_clone.write(&bytes_clone).await {
                            tracing::error!("[Terminal] PTY write error: {:?}", e);
                        }
                    })
                    .detach();
                }
            });
        }
    }

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
