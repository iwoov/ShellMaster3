// Terminal 面板组件 - 包含终端区域和命令输入框

use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::ActiveTheme;
use tracing::trace;

use alacritty_terminal::term::TermMode;

use crate::constants::icons;
use crate::ssh::session::TerminalChannel;
use crate::state::{SessionState, SessionTab};
use crate::terminal::{
    hex_to_hsla, keystroke_to_escape, render_terminal_view, SendDown, SendEnter, SendEscape,
    SendLeft, SendRight, SendTab, SendUp, TerminalCopy, TerminalPaste, TerminalState,
};

/// 渲染终端面板
pub fn render_terminal_panel(
    tab: &SessionTab,
    command_input: Option<Entity<InputState>>,
    session_state: Entity<SessionState>,
    terminal_focus_handle: Option<FocusHandle>,
    cx: &App,
) -> impl IntoElement {
    let border_color = cx.theme().border;

    // 获取终端设置
    let settings = crate::services::storage::load_settings().unwrap_or_default();
    let terminal_settings = settings.terminal.clone();

    // 获取当前激活的终端实例
    let active_terminal_id = tab.active_terminal_id.clone();
    let active_instance = active_terminal_id
        .as_ref()
        .and_then(|id| tab.terminals.iter().find(|t| &t.id == id));

    // 获取终端状态和错误信息（从当前激活的终端实例）
    let terminal_entity = active_instance.and_then(|inst| inst.terminal.clone());
    let pty_channel = active_instance.and_then(|inst| inst.pty_channel.clone());
    let pty_error = active_instance.and_then(|inst| inst.pty_error.clone());

    // 创建终端显示区域的基础 div
    // 使用 key_context("Terminal") 建立终端专用键盘上下文，用于支持自定义快捷键
    let mut terminal_display = div()
        .id("terminal-display")
        .key_context("Terminal")
        .flex_1()
        .relative()
        .overflow_hidden()
        .cursor_text();

    // 监听终端显示区域尺寸变化，并同步本地/远端 PTY 尺寸
    let tab_id = tab.id.clone();
    let session_state_for_resize = session_state.clone();
    terminal_display = terminal_display.child(
        canvas(
            move |bounds, window, cx| {
                let width = f32::from(bounds.size.width);
                let height = f32::from(bounds.size.height);
                session_state_for_resize.update(cx, |state, cx| {
                    state.sync_terminal_size(&tab_id, width, height, window, cx);
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full(),
    );

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
                    let should_alt_scroll = mode
                        .contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
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

                if let (Some(channel), Some(bytes)) =
                    (pty_channel_for_scroll.clone(), bytes_to_send)
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

        // 处理 Terminal 专用 actions（通过 key binding 触发，覆盖默认焦点切换行为）
        // Tab 键
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &SendTab, _window, cx| {
                if let Some(channel) = channel.clone() {
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, _| t.show_cursor());
                    }
                    cx.spawn(async move |_| {
                        let _ = channel.write(&[0x09]).await; // Tab = 0x09
                    })
                    .detach();
                }
            });
        }
        // Enter 键
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &SendEnter, _window, cx| {
                if let Some(channel) = channel.clone() {
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, _| t.show_cursor());
                    }
                    cx.spawn(async move |_| {
                        let _ = channel.write(&[0x0D]).await; // Enter = 0x0D
                    })
                    .detach();
                }
            });
        }
        // Escape 键
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &SendEscape, _window, cx| {
                if let Some(channel) = channel.clone() {
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, _| t.show_cursor());
                    }
                    cx.spawn(async move |_| {
                        let _ = channel.write(&[0x1B]).await; // Escape = 0x1B
                    })
                    .detach();
                }
            });
        }
        // 方向键：Up
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &SendUp, _window, cx| {
                if let Some(channel) = channel.clone() {
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, _| t.show_cursor());
                    }
                    cx.spawn(async move |_| {
                        let _ = channel.write(&[0x1B, b'[', b'A']).await; // Up arrow
                    })
                    .detach();
                }
            });
        }
        // 方向键：Down
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &SendDown, _window, cx| {
                if let Some(channel) = channel.clone() {
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, _| t.show_cursor());
                    }
                    cx.spawn(async move |_| {
                        let _ = channel.write(&[0x1B, b'[', b'B']).await; // Down arrow
                    })
                    .detach();
                }
            });
        }
        // 方向键：Left
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &SendLeft, _window, cx| {
                if let Some(channel) = channel.clone() {
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, _| t.show_cursor());
                    }
                    cx.spawn(async move |_| {
                        let _ = channel.write(&[0x1B, b'[', b'D']).await; // Left arrow
                    })
                    .detach();
                }
            });
        }
        // 方向键：Right
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &SendRight, _window, cx| {
                if let Some(channel) = channel.clone() {
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, _| t.show_cursor());
                    }
                    cx.spawn(async move |_| {
                        let _ = channel.write(&[0x1B, b'[', b'C']).await; // Right arrow
                    })
                    .detach();
                }
            });
        }
        // 复制：将终端选中文本复制到剪贴板
        {
            terminal_display = terminal_display.on_action(move |_: &TerminalCopy, _window, cx| {
                // TODO: 实现终端文本选择功能后，这里将复制选中的文本
                // 目前先记录日志，后续添加选择功能
                tracing::debug!("[Terminal] Copy action triggered");
                // 阻止事件继续传播
                cx.stop_propagation();
            });
        }
        // 粘贴：从剪贴板读取文本并发送到 PTY
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &TerminalPaste, _window, cx| {
                if let Some(channel) = channel.clone() {
                    // 从剪贴板读取文本
                    if let Some(clipboard_item) = cx.read_from_clipboard() {
                        if let Some(text) = clipboard_item.text() {
                            let bytes = text.as_bytes().to_vec();
                            tracing::debug!("[Terminal] Paste action: {} bytes", bytes.len());

                            // 重置光标为可见
                            if let Some(terminal) = terminal.clone() {
                                terminal.update(cx, |t, _| t.show_cursor());
                            }

                            // 发送到 PTY
                            cx.spawn(async move |_| {
                                if let Err(e) = channel.write(&bytes).await {
                                    tracing::error!("[Terminal] PTY write error on paste: {:?}", e);
                                }
                            })
                            .detach();
                        }
                    }
                }
                // 阻止事件继续传播
                cx.stop_propagation();
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

                // 阻止事件冒泡，确保 Tab 等按键不会被其他组件拦截
                cx.stop_propagation();
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
        render_error_terminal(&terminal_settings, &error, cx).into_any_element()
    } else if let Some(ref terminal) = terminal_entity {
        // 有终端实例 - 渲染真实终端内容
        render_terminal_content(terminal.clone(), &terminal_settings, cx).into_any_element()
    } else {
        // 等待初始化 - 显示加载提示
        render_loading_terminal(&terminal_settings, cx).into_any_element()
    });

    if let Some(scroll_handle) = scroll_handle {
        terminal_display = terminal_display.vertical_scrollbar(&scroll_handle);
    }

    // 创建终端顶部工具栏区域（15px 高度）
    let tab_id_for_toolbar = tab.id.clone();
    let terminals_for_toolbar = tab.terminals.clone();
    let active_id_for_toolbar = tab.active_terminal_id.clone();
    let session_state_for_toolbar = session_state.clone();

    let primary_color = cx.theme().primary;
    let text_color = cx.theme().foreground;
    let muted_color = cx.theme().muted_foreground;

    // 加载当前语言设置（用于动态翻译标签）
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();
    let terminal_label_prefix = crate::i18n::t(&lang, "session.terminal.tab_label");

    let terminal_toolbar = div()
        .id("terminal-toolbar")
        .h(px(20.))
        .w_full()
        .flex_shrink_0()
        .border_b_1()
        .border_color(border_color)
        .flex()
        .items_center()
        .gap_0()
        // 终端标签列表
        .children(
            terminals_for_toolbar
                .iter()
                .enumerate()
                .map(|(idx, term_inst)| {
                    let term_id = term_inst.id.clone();
                    // 动态生成翻译后的标签名
                    let term_label = format!("{} {}", terminal_label_prefix, term_inst.index);
                    let is_active = active_id_for_toolbar.as_ref() == Some(&term_id);
                    let tab_id_for_click = tab_id_for_toolbar.clone();
                    let session_for_click = session_state_for_toolbar.clone();
                    let term_id_for_click = term_id.clone();

                    // 检查是否可以关闭（有多个终端时才可关闭）
                    let can_close = terminals_for_toolbar.len() > 1;
                    let term_id_for_close = term_id.clone();
                    let tab_id_for_close = tab_id_for_toolbar.clone();
                    let session_for_close = session_state_for_toolbar.clone();

                    div()
                        .id(SharedString::from(format!("terminal-tab-{}", term_id)))
                        .h_full()
                        .px_2()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap_1()
                        .cursor_pointer()
                        .when(is_active, |s| s.bg(border_color))
                        .hover(|s| s.bg(border_color.opacity(0.5)))
                        // 点击切换终端
                        .on_click(move |_, _window, cx| {
                            session_for_click.update(cx, |state, cx| {
                                state.activate_terminal_instance(
                                    &tab_id_for_click,
                                    &term_id_for_click,
                                );
                                cx.notify();
                            });
                        })
                        // 标签文本
                        .child(
                            div()
                                .text_xs()
                                .text_color(if is_active { text_color } else { muted_color })
                                .child(term_label),
                        )
                        .when(can_close && is_active, move |s| {
                            s.child(
                                div()
                                    .id(SharedString::from(format!(
                                        "close-terminal-{}",
                                        term_id_for_close.clone()
                                    )))
                                    .size(px(12.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(2.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(Hsla::from(rgb(0xef4444)).opacity(0.3)))
                                    .on_click({
                                        let term_id = term_id_for_close.clone();
                                        let tab_id = tab_id_for_close.clone();
                                        let session = session_for_close.clone();
                                        move |_, _window, cx| {
                                            session.update(cx, |state, cx| {
                                                state.close_terminal_instance(&tab_id, &term_id);
                                                cx.notify();
                                            });
                                            cx.stop_propagation();
                                        }
                                    })
                                    .child(
                                        svg().path(icons::X).size(px(8.)).text_color(muted_color),
                                    ),
                            )
                        })
                }),
        )
        // 添加按钮
        .child({
            let tab_id_for_add = tab_id_for_toolbar.clone();
            let session_for_add = session_state_for_toolbar.clone();

            div()
                .id("add-terminal-btn")
                .h_full()
                .px_1()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|s| s.bg(primary_color.opacity(0.2)))
                .on_click(move |_, _window, cx| {
                    session_for_add.update(cx, |state, cx| {
                        state.add_terminal_instance(&tab_id_for_add);
                        cx.notify();
                    });
                })
                .child(
                    svg()
                        .path(icons::PLUS)
                        .size(px(10.))
                        .text_color(muted_color),
                )
        });

    div()
        .size_full()
        .flex()
        .flex_col()
        // 终端顶部工具栏区域
        .child(terminal_toolbar)
        // 终端显示区域（占据剩余空间）
        .child(terminal_display)
        // 命令输入区域（下方）
        .child(render_command_input(
            border_color,
            command_input,
            pty_channel,
            terminal_entity,
            cx,
        ))
}

/// 渲染真实终端内容
fn render_terminal_content(
    terminal: Entity<TerminalState>,
    settings: &crate::models::settings::TerminalSettings,
    cx: &App,
) -> impl IntoElement {
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
    pty_channel: Option<Arc<TerminalChannel>>,
    terminal: Option<Entity<TerminalState>>,
    cx: &App,
) -> impl IntoElement {
    let primary = cx.theme().primary;

    // 克隆用于闭包
    let input_for_click = command_input.clone();
    let channel_for_click = pty_channel.clone();
    let terminal_for_click = terminal.clone();

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
                        .on_click(move |_, window, cx| {
                            // 获取输入框和 PTY channel
                            let Some(input) = input_for_click.clone() else {
                                return;
                            };
                            let Some(channel) = channel_for_click.clone() else {
                                return;
                            };

                            // 读取输入内容
                            let content = input.read(cx).value().to_string();
                            if content.is_empty() {
                                return;
                            }

                            // 将内容转换为字节并追加回车符
                            let mut bytes = content.into_bytes();
                            bytes.push(0x0d); // CR (回车)

                            // 重置光标可见（有输入时）
                            if let Some(terminal) = terminal_for_click.clone() {
                                terminal.update(cx, |t, _| {
                                    t.show_cursor();
                                });
                            }

                            // 异步发送到 PTY
                            cx.spawn(async move |_async_cx| {
                                if let Err(e) = channel.write(&bytes).await {
                                    tracing::error!("[Terminal] PTY write error: {:?}", e);
                                }
                            })
                            .detach();

                            // 清空输入框
                            input.update(cx, |state, cx| {
                                state.set_value(String::new(), window, cx);
                            });
                        })
                        .child(
                            svg()
                                .path(icons::SEND)
                                .size(px(14.))
                                .text_color(gpui::white()),
                        ),
                ),
        )
}
