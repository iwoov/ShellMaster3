// Terminal 面板组件 - 包含终端区域和命令输入框

use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_component::scroll::ScrollableElement;
use gpui_component::ActiveTheme;
use tracing::trace;

use alacritty_terminal::term::TermMode;

use crate::constants::icons;
use crate::ssh::session::TerminalChannel;
use crate::state::{SessionState, SessionStatus, SessionTab};
use crate::terminal::{
    hex_to_hsla, keystroke_to_escape, mouse_mode_enabled, mouse_report_bytes, named_key_to_escape,
    paste_to_bytes, render_terminal_view, should_report_motion, SendDown, SendEnter, SendEscape,
    SendLeft, SendRight, SendTab, SendUp, TerminalCopy, TerminalPaste, TerminalSearch,
    TerminalSelectAll, TerminalState, MOUSE_LEFT, MOUSE_MIDDLE, MOUSE_RIGHT, MOUSE_WHEEL_DOWN,
    MOUSE_WHEEL_UP,
};

/// 构建终端右键上下文菜单（复制/粘贴/全选/清除回滚）。
fn build_terminal_context_menu(
    menu: PopupMenu,
    terminal: &Option<Entity<TerminalState>>,
    channel: &Option<Arc<TerminalChannel>>,
    lang: crate::models::settings::Language,
) -> PopupMenu {
    use crate::i18n::t;
    let copy_label = t(&lang, "terminal.context_menu.copy").to_string();
    let paste_label = t(&lang, "terminal.context_menu.paste").to_string();
    let select_all_label = t(&lang, "terminal.context_menu.select_all").to_string();
    let clear_label = t(&lang, "terminal.context_menu.clear").to_string();

    // 复制
    let t_copy = terminal.clone();
    let mut menu = menu.item(PopupMenuItem::new(copy_label).on_click(move |_, _, cx| {
        if let Some(terminal) = t_copy.clone() {
            let text = terminal.read(cx).selected_text_for_copy();
            if let Some(text) = text {
                if !text.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
        }
    }));

    // 粘贴
    let t_paste = terminal.clone();
    let c_paste = channel.clone();
    menu = menu.item(PopupMenuItem::new(paste_label).on_click(move |_, _, cx| {
        let Some(channel) = c_paste.clone() else {
            return;
        };
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let Some(text) = item.text() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let mode = t_paste
            .as_ref()
            .map(|t| t.read(cx).term_mode())
            .unwrap_or_else(TermMode::empty);
        let bytes = paste_to_bytes(&text, mode);
        if let Some(t) = t_paste.clone() {
            t.update(cx, |t, _| t.show_cursor());
        }
        let _ = channel.queue_write(bytes);
    }));

    menu = menu.separator();

    // 全选
    let t_all = terminal.clone();
    menu = menu.item(
        PopupMenuItem::new(select_all_label).on_click(move |_, _, cx| {
            if let Some(terminal) = t_all.clone() {
                terminal.update(cx, |t, cx| {
                    t.select_all();
                    cx.notify();
                });
            }
        }),
    );

    // 清除回滚
    let t_clear = terminal.clone();
    menu = menu.item(PopupMenuItem::new(clear_label).on_click(move |_, _, cx| {
        if let Some(terminal) = t_clear.clone() {
            terminal.update(cx, |t, cx| {
                t.clear_scrollback();
                cx.notify();
            });
        }
    }));

    menu
}

/// 计算鼠标位置对应的 1-based 终端可视区坐标。
fn mouse_grid_pos(
    terminal: &Entity<TerminalState>,
    cx: &App,
    pos: Point<Pixels>,
) -> (usize, usize) {
    let t = terminal.read(cx);
    let (ox, oy) = t.bounds_origin();
    let size = t.size();
    let rel_x = (f32::from(pos.x) - ox - t.horizontal_padding()).max(0.0);
    let rel_y = (f32::from(pos.y) - oy).max(0.0);
    let col = ((rel_x / size.cell_width) as usize + 1).min(size.columns.max(1));
    let row = ((rel_y / size.line_height) as usize + 1).min(size.lines.max(1));
    (col, row)
}

/// 尝试把鼠标事件上报给启用了鼠标模式的远端应用。
/// 成功上报返回 `true`（调用方应跳过本地选择/滚动）；Shift 按下则强制本地行为。
#[allow(clippy::too_many_arguments)]
fn try_report_mouse(
    terminal: &Option<Entity<TerminalState>>,
    channel: &Option<Arc<TerminalChannel>>,
    cx: &mut App,
    pos: Point<Pixels>,
    base_button: u8,
    is_motion: bool,
    released: bool,
    mods: &Modifiers,
) -> bool {
    if mods.shift {
        return false;
    }
    let Some(terminal) = terminal.as_ref() else {
        return false;
    };
    let Some(channel) = channel.clone() else {
        return false;
    };
    let mode = terminal.read(cx).term_mode();
    if !mouse_mode_enabled(mode) {
        return false;
    }
    let (col, row) = mouse_grid_pos(terminal, cx, pos);
    let Some(bytes) = mouse_report_bytes(base_button, is_motion, released, col, row, mods, mode)
    else {
        return false;
    };
    let _ = channel.queue_write(bytes);
    true
}

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
    let pty_error = active_instance.and_then(|inst| inst.pty_state.error_message());

    // 获取会话状态用于显示重连/断开状态
    let session_status = tab.status.clone();
    let tab_id_for_reconnect = tab.id.clone();
    let terminal_id_for_reconnect = active_terminal_id.clone().unwrap_or_default();

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
    // 同时记录终端区域在窗口中的位置，用于鼠标坐标转换
    let tab_id = tab.id.clone();
    let session_state_for_resize = session_state.clone();
    let terminal_for_bounds = terminal_entity.clone();
    terminal_display = terminal_display.child(
        canvas(
            move |bounds, window, cx| {
                let width = f32::from(bounds.size.width);
                let height = f32::from(bounds.size.height);
                let origin_x = f32::from(bounds.origin.x);
                let origin_y = f32::from(bounds.origin.y);

                // 更新尺寸；未启动/失败的 PTY 使用真实 canvas bounds 初始化。
                session_state_for_resize.update(cx, |state, cx| {
                    state.sync_or_initialize_terminal_size(&tab_id, width, height, window, cx);
                });

                // 更新 bounds origin（用于鼠标坐标转换）
                if let Some(terminal) = terminal_for_bounds.clone() {
                    terminal.update(cx, |t, _| {
                        let old = t.bounds_origin();
                        if (old.0 - origin_x).abs() > 1.0 || (old.1 - origin_y).abs() > 1.0 {
                            tracing::debug!(
                                "[Terminal] Bounds origin updated: ({:.1}, {:.1}) -> ({:.1}, {:.1})",
                                old.0, old.1, origin_x, origin_y
                            );
                        }
                        t.set_bounds_origin(origin_x, origin_y);
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full(),
    );

    // 如果有焦点句柄，添加事件监听
    if let Some(focus_handle) = terminal_focus_handle.clone() {
        let focus_for_click = focus_handle.clone();
        terminal_display = terminal_display.track_focus(&focus_handle);

        // 鼠标按下：获取焦点并开始选择
        {
            let terminal = terminal_entity.clone();
            let focus = focus_for_click.clone();
            let channel_for_mouse = pty_channel.clone();
            terminal_display =
                terminal_display.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    // 先获取焦点
                    window.focus(&focus);

                    // 若远端启用鼠标模式（且未按 Shift），上报点击而非本地选择
                    if try_report_mouse(
                        &terminal,
                        &channel_for_mouse,
                        cx,
                        event.position,
                        MOUSE_LEFT,
                        false,
                        false,
                        &event.modifiers,
                    ) {
                        cx.stop_propagation();
                        return;
                    }

                    // 开始选择
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, cx| {
                            // 获取终端区域在窗口中的偏移，转换为相对坐标（减去 padding）
                            let (origin_x, origin_y) = t.bounds_origin();
                            let rel_x: f32 =
                                f32::from(event.position.x) - origin_x - t.horizontal_padding();
                            let rel_y: f32 = f32::from(event.position.y) - origin_y;

                            t.start_selection(rel_x, rel_y, event.click_count);
                            cx.notify();
                        });
                    }
                });
        }

        // 鼠标移动（拖动）：更新选择
        {
            let terminal = terminal_entity.clone();
            let channel_for_move = pty_channel.clone();
            terminal_display = terminal_display.on_mouse_move(move |event, _window, cx| {
                // 鼠标模式下上报移动/拖动（1003 任意移动；1002 仅拖动）
                if !event.modifiers.shift {
                    if let Some(terminal_ref) = terminal.as_ref() {
                        let mode = terminal_ref.read(cx).term_mode();
                        let button_pressed = event.pressed_button.is_some();
                        if mouse_mode_enabled(mode) && should_report_motion(mode, button_pressed) {
                            let base = match event.pressed_button {
                                Some(gpui::MouseButton::Middle) => MOUSE_MIDDLE,
                                Some(gpui::MouseButton::Right) => MOUSE_RIGHT,
                                _ => MOUSE_LEFT,
                            };
                            if try_report_mouse(
                                &terminal,
                                &channel_for_move,
                                cx,
                                event.position,
                                base,
                                true,
                                false,
                                &event.modifiers,
                            ) {
                                return;
                            }
                        }
                    }
                }

                // 只有按住左键拖动时才更新选择
                if event.pressed_button != Some(gpui::MouseButton::Left) {
                    return;
                }

                if let Some(terminal) = terminal.clone() {
                    terminal.update(cx, |t, cx| {
                        // 获取终端区域在窗口中的偏移，转换为相对坐标（减去 padding）
                        let (origin_x, origin_y) = t.bounds_origin();
                        let rel_x: f32 =
                            f32::from(event.position.x) - origin_x - t.horizontal_padding();
                        let rel_y: f32 = f32::from(event.position.y) - origin_y;

                        // 拖动到终端上/下边缘之外时自动滚动 scrollback
                        let size = t.size();
                        let viewport_h = size.line_height * size.lines as f32;
                        if rel_y < 0.0 {
                            t.scroll_by_lines(1);
                        } else if rel_y > viewport_h {
                            t.scroll_by_lines(-1);
                        }

                        t.update_selection(rel_x, rel_y);
                        cx.notify();
                    });
                }
            });
        }

        // 鼠标释放：结束选择
        {
            let terminal = terminal_entity.clone();
            let copy_on_select = terminal_settings.copy_on_select;
            let channel_for_up = pty_channel.clone();
            terminal_display =
                terminal_display.on_mouse_up(MouseButton::Left, move |event, _window, cx| {
                    // 鼠标模式下上报释放
                    if try_report_mouse(
                        &terminal,
                        &channel_for_up,
                        cx,
                        event.position,
                        MOUSE_LEFT,
                        false,
                        true,
                        &event.modifiers,
                    ) {
                        cx.stop_propagation();
                        return;
                    }

                    if let Some(terminal) = terminal.clone() {
                        let selected_text = terminal.update(cx, |t, cx| {
                            let _ = t.end_selection();
                            let selected_text = t.selected_text_for_copy();
                            // 选择结束后不清除选择，保留高亮显示
                            cx.notify();
                            selected_text
                        });
                        if copy_on_select {
                            if let Some(text) = selected_text {
                                if !text.is_empty() {
                                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                                }
                            }
                        }
                    }
                });
        }

        // 右键按下：仅在鼠标模式下上报（非鼠标模式由下方上下文菜单处理）
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            let right_click_paste = terminal_settings.right_click_paste;
            terminal_display =
                terminal_display.on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                    if try_report_mouse(
                        &terminal,
                        &channel,
                        cx,
                        event.position,
                        MOUSE_RIGHT,
                        false,
                        false,
                        &event.modifiers,
                    ) {
                        cx.stop_propagation();
                        return;
                    }

                    if right_click_paste && !event.modifiers.shift {
                        let Some(channel) = channel.clone() else {
                            return;
                        };
                        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text())
                        else {
                            return;
                        };
                        let mode = terminal
                            .as_ref()
                            .map(|terminal| terminal.read(cx).term_mode())
                            .unwrap_or_else(TermMode::empty);
                        let bytes = paste_to_bytes(&text, mode);
                        let _ = channel.queue_write(bytes);
                        cx.stop_propagation();
                    }
                });
        }

        // 右键释放：鼠标模式下上报
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display =
                terminal_display.on_mouse_up(MouseButton::Right, move |event, _window, cx| {
                    if try_report_mouse(
                        &terminal,
                        &channel,
                        cx,
                        event.position,
                        MOUSE_RIGHT,
                        false,
                        true,
                        &event.modifiers,
                    ) {
                        cx.stop_propagation();
                    }
                });
        }

        // 中键按下/释放：鼠标模式下上报
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display =
                terminal_display.on_mouse_down(MouseButton::Middle, move |event, _window, cx| {
                    if try_report_mouse(
                        &terminal,
                        &channel,
                        cx,
                        event.position,
                        MOUSE_MIDDLE,
                        false,
                        false,
                        &event.modifiers,
                    ) {
                        cx.stop_propagation();
                    }
                });
        }
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display =
                terminal_display.on_mouse_up(MouseButton::Middle, move |event, _window, cx| {
                    if try_report_mouse(
                        &terminal,
                        &channel,
                        cx,
                        event.position,
                        MOUSE_MIDDLE,
                        false,
                        true,
                        &event.modifiers,
                    ) {
                        cx.stop_propagation();
                    }
                });
        }

        // 滚轮：滚动查看历史（非 ALT_SCREEN），或在 ALT_SCREEN 下发送上/下箭头模拟滚动
        if terminal_entity.is_some() {
            let terminal_for_scroll = terminal_entity.clone();
            let pty_channel_for_scroll = pty_channel.clone();
            let scroll_multiplier = terminal_settings.scroll_multiplier.clamp(0.1, 10.0);
            terminal_display = terminal_display.on_scroll_wheel(move |event, _window, cx| {
                let Some(terminal) = terminal_for_scroll.clone() else {
                    return;
                };

                let mut bytes_to_send: Option<Vec<u8>> = None;
                let mut handled = false;

                terminal.update(cx, |t, cx| {
                    let Some(scroll_lines) = t.determine_scroll_lines(event, scroll_multiplier)
                    else {
                        return;
                    };
                    if scroll_lines == 0 {
                        return;
                    }

                    let mode = t.term_mode();

                    // 鼠标模式：上报滚轮（优先于本地滚动与 alt-scroll 箭头模拟）
                    if mouse_mode_enabled(mode) && !event.modifiers.shift {
                        let base = if scroll_lines > 0 {
                            MOUSE_WHEEL_UP
                        } else {
                            MOUSE_WHEEL_DOWN
                        };
                        let (ox, oy) = t.bounds_origin();
                        let size = t.size();
                        let rel_x =
                            (f32::from(event.position.x) - ox - t.horizontal_padding()).max(0.0);
                        let rel_y = (f32::from(event.position.y) - oy).max(0.0);
                        let col = ((rel_x / size.cell_width) as usize + 1).min(size.columns.max(1));
                        let row = ((rel_y / size.line_height) as usize + 1).min(size.lines.max(1));
                        let mut content = Vec::new();
                        for _ in 0..scroll_lines.abs() {
                            if let Some(b) = mouse_report_bytes(
                                base,
                                false,
                                false,
                                col,
                                row,
                                &event.modifiers,
                                mode,
                            ) {
                                content.extend_from_slice(&b);
                            }
                        }
                        if !content.is_empty() {
                            bytes_to_send = Some(content);
                            handled = true;
                        }
                        return;
                    }

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
                    if let Err(e) = channel.queue_write(bytes) {
                        tracing::error!("[Terminal] PTY write error: {:?}", e);
                    }
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
                    let _ = channel.queue_write(vec![0x09]); // Tab = 0x09
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
                    let _ = channel.queue_write(vec![0x0D]); // Enter = 0x0D
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
                    let _ = channel.queue_write(vec![0x1B]); // Escape = 0x1B
                }
            });
        }
        // 方向键：Up
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &SendUp, _window, cx| {
                if let Some(channel) = channel.clone() {
                    let mode = terminal
                        .as_ref()
                        .map(|terminal| terminal.read(cx).term_mode())
                        .unwrap_or_else(TermMode::empty);
                    let bytes = named_key_to_escape("up", &Modifiers::default(), mode)
                        .unwrap_or_else(|| vec![0x1B, b'[', b'A']);
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, _| t.show_cursor());
                    }
                    let _ = channel.queue_write(bytes);
                }
            });
        }
        // 方向键：Down
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &SendDown, _window, cx| {
                if let Some(channel) = channel.clone() {
                    let mode = terminal
                        .as_ref()
                        .map(|terminal| terminal.read(cx).term_mode())
                        .unwrap_or_else(TermMode::empty);
                    let bytes = named_key_to_escape("down", &Modifiers::default(), mode)
                        .unwrap_or_else(|| vec![0x1B, b'[', b'B']);
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, _| t.show_cursor());
                    }
                    let _ = channel.queue_write(bytes);
                }
            });
        }
        // 方向键：Left
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &SendLeft, _window, cx| {
                if let Some(channel) = channel.clone() {
                    let mode = terminal
                        .as_ref()
                        .map(|terminal| terminal.read(cx).term_mode())
                        .unwrap_or_else(TermMode::empty);
                    let bytes = named_key_to_escape("left", &Modifiers::default(), mode)
                        .unwrap_or_else(|| vec![0x1B, b'[', b'D']);
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, _| t.show_cursor());
                    }
                    let _ = channel.queue_write(bytes);
                }
            });
        }
        // 方向键：Right
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &SendRight, _window, cx| {
                if let Some(channel) = channel.clone() {
                    let mode = terminal
                        .as_ref()
                        .map(|terminal| terminal.read(cx).term_mode())
                        .unwrap_or_else(TermMode::empty);
                    let bytes = named_key_to_escape("right", &Modifiers::default(), mode)
                        .unwrap_or_else(|| vec![0x1B, b'[', b'C']);
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, _| t.show_cursor());
                    }
                    let _ = channel.queue_write(bytes);
                }
            });
        }
        // 复制：将终端选中文本复制到剪贴板
        {
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &TerminalCopy, _window, cx| {
                if let Some(terminal) = terminal.clone() {
                    let selected_text = terminal.update(cx, |t, _| t.selected_text_for_copy());
                    if let Some(text) = selected_text {
                        if !text.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            tracing::debug!("[Terminal] Copied {} chars to clipboard", text.len());
                        }
                    } else {
                        tracing::debug!("[Terminal] No text selected for copy");
                    }
                }
                // 阻止事件继续传播
                cx.stop_propagation();
            });
        }
        // 搜索：切换搜索栏
        {
            let session = session_state.clone();
            terminal_display = terminal_display.on_action(move |_: &TerminalSearch, window, cx| {
                session.update(cx, |state, cx| {
                    state.toggle_terminal_search(window, cx);
                });
                cx.stop_propagation();
            });
        }
        // 全选：选中整个缓冲区（含 scrollback）
        {
            let terminal = terminal_entity.clone();
            terminal_display =
                terminal_display.on_action(move |_: &TerminalSelectAll, _window, cx| {
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, cx| {
                            t.select_all();
                            cx.notify();
                        });
                    }
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
                            let mode = terminal
                                .as_ref()
                                .map(|terminal| terminal.read(cx).term_mode())
                                .unwrap_or_else(TermMode::empty);
                            let bytes = paste_to_bytes(&text, mode);
                            tracing::debug!("[Terminal] Paste action: {} bytes", bytes.len());

                            // 重置光标为可见
                            if let Some(terminal) = terminal.clone() {
                                terminal.update(cx, |t, _| t.show_cursor());
                            }

                            if let Err(e) = channel.queue_write(bytes) {
                                tracing::error!("[Terminal] PTY write error on paste: {:?}", e);
                            }
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

            let mode = terminal_for_key
                .as_ref()
                .map(|terminal| terminal.read(cx).term_mode())
                .unwrap_or_else(TermMode::empty);

            // 将按键转换为转义序列
            if let Some(bytes) =
                keystroke_to_escape(&event.keystroke, &event.keystroke.modifiers, mode)
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

                if let Err(e) = channel.queue_write(bytes) {
                    tracing::error!("[Terminal] PTY write error: {:?}", e);
                }

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
        render_error_terminal(
            &terminal_settings,
            &error,
            tab.id.clone(),
            active_terminal_id.clone().unwrap_or_default(),
            session_state.clone(),
            cx,
        )
        .into_any_element()
    } else if let Some(ref terminal) = terminal_entity {
        // 有终端实例 - 渲染真实终端内容
        // 如果处于重连或断开状态，在终端内容上叠加状态覆盖层
        let terminal_content =
            render_terminal_content(terminal.clone(), &terminal_settings, cx).into_any_element();

        match &session_status {
            SessionStatus::Reconnecting {
                attempt,
                max_attempts,
            } => {
                // 重连中 - 显示终端历史 + 重连覆盖层
                render_terminal_with_overlay(
                    terminal_content,
                    &terminal_settings,
                    render_reconnecting_overlay(*attempt, *max_attempts, cx),
                    cx,
                )
                .into_any_element()
            }
            SessionStatus::Disconnected => {
                // 断开连接 - 显示终端历史 + 断开覆盖层 + 重连按钮
                render_terminal_with_overlay(
                    terminal_content,
                    &terminal_settings,
                    render_disconnected_overlay(
                        tab_id_for_reconnect.clone(),
                        terminal_id_for_reconnect.clone(),
                        session_state.clone(),
                        cx,
                    ),
                    cx,
                )
                .into_any_element()
            }
            _ => terminal_content,
        }
    } else {
        // 等待初始化 - 显示加载提示
        render_loading_terminal(&terminal_settings, cx).into_any_element()
    });

    if let Some(scroll_handle) = scroll_handle {
        terminal_display = terminal_display.vertical_scrollbar(&scroll_handle);
    }

    // 搜索栏：搜索打开时在终端右上角浮层显示
    if session_state.read(cx).search_open {
        if let Some(search_input) = session_state.read(cx).search_input.clone() {
            let (cur, total) = terminal_entity
                .as_ref()
                .map(|t| {
                    let s = t.read(cx);
                    (s.search_current_number(), s.search_match_count())
                })
                .unwrap_or((0, 0));
            let count_text = format!("{}/{}", cur, total);

            let bar_bg = cx.theme().popover;
            let bar_border = cx.theme().border;
            let muted = cx.theme().muted_foreground;

            let sess_prev = session_state.clone();
            let sess_next = session_state.clone();
            let sess_close = session_state.clone();

            let icon_btn = |id: &'static str, icon: &'static str| {
                div()
                    .id(id)
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.))
                    .cursor_pointer()
                    .hover(|s| s.bg(muted.opacity(0.2)))
                    .child(svg().path(icon).size(px(12.)).text_color(muted))
            };

            let search_bar = div()
                .absolute()
                .top(px(4.))
                .right(px(16.))
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(bar_border)
                .bg(bar_bg)
                // 阻止在搜索栏上的鼠标操作触发终端选择
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(svg().path(icons::SEARCH).size(px(12.)).text_color(muted))
                .child(
                    div()
                        .w(px(150.))
                        .child(Input::new(&search_input).appearance(false)),
                )
                .child(div().text_xs().text_color(muted).child(count_text))
                .child(icon_btn("terminal-search-prev", icons::ARROW_UP).on_click(
                    move |_, _window, cx| {
                        sess_prev.update(cx, |state, cx| {
                            if let Some(t) = state.active_terminal_entity() {
                                t.update(cx, |t, cx| {
                                    t.search_prev_match();
                                    cx.notify();
                                });
                            }
                            cx.notify();
                        });
                    },
                ))
                .child(
                    icon_btn("terminal-search-next", icons::CHEVRON_DOWN).on_click(
                        move |_, _window, cx| {
                            sess_next.update(cx, |state, cx| {
                                if let Some(t) = state.active_terminal_entity() {
                                    t.update(cx, |t, cx| {
                                        t.search_next_match();
                                        cx.notify();
                                    });
                                }
                                cx.notify();
                            });
                        },
                    ),
                )
                .child(icon_btn("terminal-search-close", icons::X).on_click(
                    move |_, window, cx| {
                        sess_close.update(cx, |state, cx| {
                            state.close_terminal_search(window, cx);
                        });
                    },
                ));

            terminal_display = terminal_display.child(search_bar);
        }
    }

    // 右键菜单始终注册；鼠标上报或直接粘贴成功时会停止事件传播。
    // 因而 Shift+右键仍可强制打开本地菜单。
    let menu_lang = settings.theme.language.clone();
    let terminal_area: AnyElement = if terminal_entity.is_none() {
        terminal_display.into_any_element()
    } else {
        let ct = terminal_entity.clone();
        let cc = pty_channel.clone();
        terminal_display
            .context_menu(move |menu, _window, _cx| {
                build_terminal_context_menu(menu, &ct, &cc, menu_lang.clone())
            })
            .into_any_element()
    };

    // 创建终端顶部工具栏区域（15px 高度）
    let tab_id_for_toolbar = tab.id.clone();
    let terminals_for_toolbar = tab.terminals.clone();
    let active_id_for_toolbar = tab.active_terminal_id.clone();
    let session_state_for_toolbar = session_state.clone();

    let primary_color = cx.theme().primary;
    let text_color = cx.theme().foreground;
    let muted_color = cx.theme().muted_foreground;

    // 加载当前语言设置（用于动态翻译标签）
    let lang = settings.theme.language.clone();
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
                .map(|(_idx, term_inst)| {
                    let term_id = term_inst.id.clone();
                    // 优先使用应用通过 OSC 设置的标题，否则回退到翻译后的默认标签名
                    let term_label = match term_inst.title.as_deref() {
                        Some(title) if !title.is_empty() => title.to_string(),
                        _ => format!("{} {}", terminal_label_prefix, term_inst.index),
                    };
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
                                .max_w(px(220.))
                                .overflow_hidden()
                                .text_ellipsis()
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
                                                state
                                                    .close_terminal_instance(&tab_id, &term_id, cx);
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
        .children(terminal_settings.show_tab_bar.then_some(terminal_toolbar))
        // 终端显示区域（占据剩余空间）
        .child(terminal_area)
        // 命令输入区域（下方）
        .children(terminal_settings.show_command_input.then(|| {
            render_command_input(
                border_color,
                command_input,
                pty_channel,
                terminal_entity,
                cx,
            )
        }))
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
    let bell_flash = state.is_bell_flash();
    let search_matches = state.search_matches();
    let search_current = state.search_current_index();

    // 使用 renderer 中的 render_terminal_view 函数
    render_terminal_view(
        &term.lock(),
        size,
        settings,
        cursor_visible,
        bell_flash,
        search_matches,
        search_current,
        cx,
    )
}

/// 渲染错误状态的终端
fn render_error_terminal(
    settings: &crate::models::settings::TerminalSettings,
    error: &str,
    tab_id: String,
    terminal_id: String,
    session_state: Entity<SessionState>,
    cx: &App,
) -> Div {
    let bg_color = hex_to_hsla(&settings.background_color);

    // 获取语言设置
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    // 判断是否是断开连接
    let is_disconnected = error == "terminal.disconnected";
    let is_shell_exited = error == "terminal.shell_exited";

    // 根据类型选择颜色和图标
    let (color, icon, message) = if is_disconnected {
        (
            Hsla::from(rgb(0xf59e0b)), // 橙色 (amber-500)
            icons::CIRCLE,
            crate::i18n::t(&lang, "terminal.disconnected").to_string(),
        )
    } else if is_shell_exited {
        (
            Hsla::from(rgb(0xf59e0b)),
            icons::TERMINAL,
            crate::i18n::t(&lang, "terminal.shell_exited").to_string(),
        )
    } else {
        (
            Hsla::from(rgb(0xef4444)), // 红色 (red-500)
            icons::X,
            format!("{}: {}", crate::i18n::t(&lang, "terminal.error"), error),
        )
    };

    let primary = cx.theme().primary;
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
                .child(svg().path(icon).size(px(32.)).text_color(color))
                .child(div().text_color(color).text_sm().child(message))
                .children(is_shell_exited.then(|| {
                    div()
                        .id("restart-shell-btn")
                        .mt_2()
                        .px_4()
                        .py_2()
                        .rounded_md()
                        .bg(primary)
                        .text_color(Hsla::from(rgb(0xffffff)))
                        .text_sm()
                        .cursor_pointer()
                        .hover(|style| style.opacity(0.9))
                        .child(crate::i18n::t(&lang, "terminal.restart_shell"))
                        .on_click(move |_, _, cx| {
                            session_state.update(cx, |state, cx| {
                                state.restart_terminal_instance(&tab_id, &terminal_id);
                                cx.notify();
                            });
                        })
                })),
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

/// 渲染终端内容 + 覆盖层
fn render_terminal_with_overlay(
    terminal_content: AnyElement,
    _settings: &crate::models::settings::TerminalSettings,
    overlay: impl IntoElement,
    _cx: &App,
) -> Div {
    div()
        .size_full()
        .relative()
        // 终端内容层（底层，半透明）
        .child(div().size_full().opacity(0.6).child(terminal_content))
        // 覆盖层（顶层）
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(overlay),
        )
}

/// 渲染重连中覆盖层
fn render_reconnecting_overlay(attempt: u32, max_attempts: u32, _cx: &App) -> Div {
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    let amber_color = Hsla::from(rgb(0xf59e0b));

    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_3()
        .p_6()
        .rounded_lg()
        .bg(Hsla::from(rgb(0x000000)).opacity(0.7))
        .child(
            svg()
                .path(icons::LOADER)
                .size(px(32.))
                .text_color(amber_color),
        )
        .child(
            div()
                .text_color(amber_color)
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(crate::i18n::t(&lang, "terminal.reconnecting")),
        )
        .child(
            div()
                .text_color(Hsla::from(rgb(0xffffff)).opacity(0.6))
                .text_xs()
                .child(format!(
                    "{} {}/{}",
                    crate::i18n::t(&lang, "terminal.reconnect_attempt"),
                    attempt,
                    max_attempts
                )),
        )
}

/// 渲染断开连接覆盖层（带重连按钮）
fn render_disconnected_overlay(
    tab_id: String,
    terminal_id: String,
    session_state: Entity<SessionState>,
    cx: &App,
) -> Div {
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    let amber_color = Hsla::from(rgb(0xf59e0b));
    let primary = cx.theme().primary;

    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_3()
        .p_6()
        .rounded_lg()
        .bg(Hsla::from(rgb(0x000000)).opacity(0.7))
        .child(
            svg()
                .path(icons::LOADER)
                .size(px(32.))
                .text_color(amber_color),
        )
        .child(
            div()
                .text_color(amber_color)
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(crate::i18n::t(&lang, "terminal.disconnected")),
        )
        // 重连按钮
        .child(
            div()
                .id("reconnect-btn")
                .mt_2()
                .px_4()
                .py_2()
                .rounded_md()
                .bg(primary)
                .cursor_pointer()
                .hover(|s| s.opacity(0.9))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    svg()
                        .path(icons::REFRESH)
                        .size(px(14.))
                        .text_color(Hsla::from(rgb(0xffffff))),
                )
                .child(
                    div()
                        .text_color(Hsla::from(rgb(0xffffff)))
                        .text_sm()
                        .child(crate::i18n::t(&lang, "terminal.reconnect")),
                )
                .on_click(move |_, _, cx| {
                    crate::ssh::start_manual_reconnection(
                        tab_id.clone(),
                        terminal_id.clone(),
                        session_state.clone(),
                        cx,
                    );
                }),
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
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |s| s.bg(primary.opacity(0.1)))
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

                            if let Err(e) = channel.queue_write(bytes) {
                                tracing::error!("[Terminal] PTY write error: {:?}", e);
                            }

                            // 清空输入框
                            input.update(cx, |state, cx| {
                                state.set_value(String::new(), window, cx);
                            });
                        })
                        .child(svg().path(icons::SEND).size(px(14.)).text_color(primary)),
                ),
        )
}
