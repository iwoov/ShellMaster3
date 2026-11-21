// Terminal 面板组件 - 包含终端区域和命令输入框

use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_component::scroll::ScrollableElement;
use tracing::trace;

use alacritty_terminal::term::TermMode;

use crate::components::common::button::{
    icon_button_custom, icon_button_sized, icon_toggle_button, primary_button, IconButtonLook,
    ICON_BUTTON_SIZE,
};
use crate::components::common::icon::icon_sized;
use crate::components::common::input::text_area;
use crate::components::common::panel::{panel_header, spinner, HEADER_CONTROL_HEIGHT};
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::ssh::session::TerminalChannel;
use crate::state::{SessionState, SessionStatus, SessionTab, TerminalPtyState};
use crate::terminal::{
    hex_to_hsla, keystroke_to_escape, mouse_mode_enabled, mouse_report_bytes, named_key_to_escape,
    paste_to_bytes, render_terminal_view, should_report_motion, should_use_platform_text_input,
    SendBackTab, SendDown, SendEnter, SendEscape, SendLeft, SendRight, SendTab, SendUp,
    TerminalCopy, TerminalInputHandler, TerminalPaste, TerminalSearch, TerminalSelectAll,
    TerminalState, MOUSE_LEFT, MOUSE_MIDDLE, MOUSE_RIGHT, MOUSE_WHEEL_DOWN, MOUSE_WHEEL_UP,
};
use crate::theme::Palette;

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
    let p = crate::theme::palette(cx);

    // 获取终端设置（内存中的已保存设置，渲染时不读盘）
    let settings = crate::services::storage::cached_settings();
    let terminal_settings = settings.terminal.clone();
    let lang = settings.theme.language.clone();

    // 获取当前激活的终端实例
    let active_terminal_id = tab.active_terminal_id.clone();
    let active_instance = active_terminal_id
        .as_ref()
        .and_then(|id| tab.terminals.iter().find(|t| &t.id == id));

    // 获取终端状态和错误信息（从当前激活的终端实例）
    let terminal_entity = active_instance.and_then(|inst| inst.terminal.clone());
    let pty_channel = active_instance.and_then(|inst| inst.pty_channel.clone());
    let pty_state = active_instance
        .map(|inst| inst.pty_state.clone())
        .unwrap_or(TerminalPtyState::NotStarted);

    // 获取会话状态用于显示重连/断开状态
    let session_status = tab.status.clone();
    let tab_id_for_reconnect = tab.id.clone();

    // 创建终端显示区域的基础 div
    // 使用 key_context("Terminal") 建立终端专用键盘上下文，用于支持自定义快捷键
    // 背景用终端配色：网格四周的留白和网格是同一个颜色
    let terminal_bg = terminal_background(&terminal_settings);
    let mut terminal_display = div()
        .id("terminal-display")
        .key_context("Terminal")
        .flex_1()
        .min_h(px(0.))
        .relative()
        .overflow_hidden()
        .bg(terminal_bg)
        .cursor_text();

    // 监听终端显示区域尺寸变化，并同步本地/远端 PTY 尺寸
    // 同时记录终端区域在窗口中的位置，用于鼠标坐标转换
    let tab_id = tab.id.clone();
    let session_state_for_resize = session_state.clone();
    let terminal_for_bounds = terminal_entity.clone();
    let terminal_for_input = terminal_entity.clone();
    let channel_for_input = pty_channel.clone();
    let focus_for_input = terminal_focus_handle.clone();
    // 测量用的 canvas 放在留白以内的内层容器里（见下方 `grid_area`）：行列数、鼠标和输入法坐标都按内层计算
    let grid_measure = canvas(
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
                            old.0,
                            old.1,
                            origin_x,
                            origin_y
                        );
                    }
                    t.set_bounds_origin(origin_x, origin_y);
                });
            }
        },
        move |bounds, _, window, cx| {
            if let (Some(focus), Some(terminal), Some(channel)) = (
                focus_for_input.as_ref(),
                terminal_for_input.clone(),
                channel_for_input.clone(),
            ) {
                window.handle_input(
                    focus,
                    TerminalInputHandler::new(terminal, channel, bounds),
                    cx,
                );
            }
        },
    )
    .absolute()
    .size_full();

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
        // Shift+Tab / BackTab（CSI Z）
        {
            let channel = pty_channel.clone();
            let terminal = terminal_entity.clone();
            terminal_display = terminal_display.on_action(move |_: &SendBackTab, _window, cx| {
                if let Some(channel) = channel.clone() {
                    let modifiers = Modifiers {
                        shift: true,
                        ..Default::default()
                    };
                    let bytes = named_key_to_escape("tab", &modifiers, TermMode::empty())
                        .unwrap_or_else(|| b"\x1b[Z".to_vec());
                    if let Some(terminal) = terminal.clone() {
                        terminal.update(cx, |t, _| t.show_cursor());
                    }
                    let _ = channel.queue_write(bytes);
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

            // 不要在 keydown 阶段发送可打印字符。让平台 InputHandler 处理它们，
            // 否则拼音的每个字母会被提前写入 PTY，系统输入法永远无法进入组合态。
            // 控制键、Alt/Meta 和具名键仍继续走下方的终端转义序列映射。
            if should_use_platform_text_input(&event.keystroke) {
                return;
            }

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

    // 右侧滚动条：有终端内容就显示，断开或 shell 退出后也能滚动查看历史
    // （需要作为最后的 child，确保绘制在最上层）
    let scroll_handle = terminal_entity
        .as_ref()
        .map(|terminal| terminal.read(cx).scroll_handle());

    // 状态提示：连接状态优先（重连中 / 已断开），其次是当前终端自身的状态（shell 已退出 / 打开失败）。
    // 有终端内容时一律叠加在历史之上，不再用错误页替换整个终端。
    let terminal_id_for_overlay = active_terminal_id.clone().unwrap_or_default();
    let overlay: Option<AnyElement> = match &session_status {
        SessionStatus::Reconnecting {
            attempt,
            max_attempts,
        } => {
            Some(render_reconnecting_overlay(*attempt, *max_attempts, &lang, &p).into_any_element())
        }
        SessionStatus::Disconnected => Some(
            render_disconnected_overlay(
                tab_id_for_reconnect.clone(),
                tab.disconnect_reason.clone(),
                session_state.clone(),
                &lang,
                &p,
            )
            .into_any_element(),
        ),
        _ => match &pty_state {
            TerminalPtyState::Exited => Some(
                render_shell_exited_overlay(
                    tab.id.clone(),
                    terminal_id_for_overlay.clone(),
                    session_state.clone(),
                    &lang,
                    &p,
                )
                .into_any_element(),
            ),
            TerminalPtyState::Failed(message) => Some(
                render_open_failed_overlay(
                    message.clone(),
                    tab.id.clone(),
                    terminal_id_for_overlay.clone(),
                    session_state.clone(),
                    &lang,
                    &p,
                )
                .into_any_element(),
            ),
            _ => None,
        },
    };

    // 网格区域：四周留白以内（左边距由设置里的「左侧内边距」在绘制时加上）
    let mut grid_area = div()
        .absolute()
        .top(GRID_INSET_TOP)
        .bottom(GRID_INSET_BOTTOM)
        .left_0()
        .right(GRID_INSET_RIGHT)
        .child(grid_measure);
    if let Some(terminal) = &terminal_entity {
        grid_area = grid_area.child(render_terminal_content(
            terminal.clone(),
            &terminal_settings,
            cx,
        ));
    }
    terminal_display = terminal_display.child(grid_area);

    // 状态卡片盖在整个终端区域上；没有状态时，终端还没打开（含正在打开 PTY）就显示初始化提示
    let initializing = terminal_entity.is_none() || pty_state == TerminalPtyState::Opening;
    if let Some(overlay) = overlay {
        terminal_display = terminal_display.child(render_overlay_layer(overlay, &p));
    } else if initializing {
        terminal_display =
            terminal_display.child(render_initializing_layer(&terminal_settings, &lang));
    }

    if let Some(scroll_handle) = scroll_handle {
        terminal_display = terminal_display.vertical_scrollbar(&scroll_handle);
    }

    // 搜索栏：搜索打开时浮在终端右上角
    if session_state.read(cx).search_open {
        if let Some(search_input) = session_state.read(cx).search_input.clone() {
            let (cur, total) = terminal_entity
                .as_ref()
                .map(|t| {
                    let s = t.read(cx);
                    (s.search_current_number(), s.search_match_count())
                })
                .unwrap_or((0, 0));
            let has_query = !search_input.read(cx).value().is_empty();
            terminal_display = terminal_display.child(render_search_bar(
                &search_input,
                (cur, total),
                has_query,
                session_state.clone(),
                &lang,
                &p,
            ));
        }
    }

    // 右键菜单始终注册；鼠标上报或直接粘贴成功时会停止事件传播。
    // 因而 Shift+右键仍可强制打开本地菜单。
    let menu_lang = lang.clone();
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

    let search_open = session_state.read(cx).search_open;
    let terminal_tabs = render_terminal_tabs(tab, search_open, session_state.clone(), &lang, &p);

    div()
        .size_full()
        .flex()
        .flex_col()
        // 终端标签条（设置里可关闭）
        .children(terminal_settings.show_tab_bar.then_some(terminal_tabs))
        // 终端显示区域（占据剩余空间）
        .child(terminal_area)
        // 命令输入区域（下方，设置里可关闭）
        .children(terminal_settings.show_command_input.then(|| {
            render_command_input(command_input, pty_channel, terminal_entity, &lang, &p, cx)
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

/// 终端网格四周的留白：上 6、下 4，右边留出滚动条的位置（左边距由设置里的「左侧内边距」决定）
const GRID_INSET_TOP: Pixels = px(6.);
const GRID_INSET_BOTTOM: Pixels = px(4.);
const GRID_INSET_RIGHT: Pixels = px(12.);

/// 终端标签的最大宽度（名称部分）
const TERMINAL_TAB_LABEL_MAX: Pixels = px(160.);
/// 终端标签内部用于联动悬停样式（显示关闭钮）的分组名
const TERMINAL_TAB_GROUP: &str = "terminal-tab";

/// 终端背景色（按设置里的不透明度）
fn terminal_background(settings: &crate::models::settings::TerminalSettings) -> Hsla {
    let opacity = (settings.background_opacity as f32 / 100.0).clamp(0.0, 1.0);
    hex_to_hsla(&settings.background_color).opacity(opacity)
}

/// 终端标签条：各终端的标签、新建终端，右侧是搜索
fn render_terminal_tabs(
    tab: &SessionTab,
    search_open: bool,
    session_state: Entity<SessionState>,
    lang: &Language,
    p: &Palette,
) -> impl IntoElement {
    let label_prefix = i18n::t(lang, "session.terminal.tab_label");
    let can_close = tab.terminals.len() > 1;

    let tabs = tab.terminals.iter().map(|instance| {
        let is_active = tab.active_terminal_id.as_ref() == Some(&instance.id);
        // 使用稳定的本地编号作为标签，避免远端 OSC 标题使标签频繁变化。
        let label = format!("{} {}", label_prefix, instance.index);
        let (hover_bg, text) = (p.hover, if is_active { p.text } else { p.text_muted });

        let activate = {
            let session_state = session_state.clone();
            let tab_id = tab.id.clone();
            let terminal_id = instance.id.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                session_state.update(cx, |state, cx| {
                    state.activate_terminal_instance(&tab_id, &terminal_id);
                    cx.notify();
                });
            }
        };

        div()
            .id(SharedString::from(format!("terminal-tab-{}", instance.id)))
            .group(TERMINAL_TAB_GROUP)
            .flex_none()
            .h(HEADER_CONTROL_HEIGHT)
            .pl(px(10.))
            .pr(px(if can_close { 4. } else { 10. }))
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .when(is_active, |tab| tab.bg(p.selected))
            .when(!is_active, |tab| tab.hover(move |s| s.bg(hover_bg)))
            .on_click(activate)
            .child(icon_sized(
                icons::TERMINAL,
                13.,
                if is_active { p.primary } else { p.text_faint },
            ))
            .child(
                div()
                    .max_w(TERMINAL_TAB_LABEL_MAX)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(12.))
                    .font_weight(if is_active {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(text)
                    .child(label),
            )
            // 关闭按钮：有多个终端时才显示，当前标签一直显示，其余标签悬停时显示
            .when(can_close, |row| {
                let session_state = session_state.clone();
                let tab_id = tab.id.clone();
                let terminal_id = instance.id.clone();
                row.child(
                    icon_button_sized(
                        SharedString::from(format!("close-terminal-{}", instance.id)),
                        icons::X,
                        px(18.),
                        11.,
                        i18n::t(lang, "terminal.tab.close"),
                        p,
                    )
                    .rounded(px(5.))
                    .when(!is_active, |button| {
                        button
                            .invisible()
                            .group_hover(TERMINAL_TAB_GROUP, |s| s.visible())
                    })
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        session_state.update(cx, |state, cx| {
                            state.close_terminal_instance(&tab_id, &terminal_id, cx);
                            cx.notify();
                        });
                    }),
                )
            })
    });

    let add_button = {
        let session_state = session_state.clone();
        let tab_id = tab.id.clone();
        icon_button_sized(
            "add-terminal-btn",
            icons::PLUS,
            px(26.),
            13.,
            i18n::t(lang, "terminal.tab.new"),
            p,
        )
        .on_click(move |_, _, cx| {
            session_state.update(cx, |state, cx| {
                state.add_terminal_instance(&tab_id);
                cx.notify();
            });
        })
    };

    let search_button = {
        let session_state = session_state.clone();
        icon_toggle_button(
            "terminal-search-toggle",
            icons::SEARCH,
            search_open,
            i18n::t(lang, "terminal.search.open"),
            p,
        )
        .on_click(move |_, window, cx| {
            session_state.update(cx, |state, cx| state.toggle_terminal_search(window, cx));
        })
    };

    panel_header(p)
        .id("terminal-toolbar")
        .w_full()
        .px(px(6.))
        .gap(px(2.))
        .bg(p.background)
        .children(tabs)
        .child(add_button)
        .child(div().flex_1())
        .child(search_button)
}

/// 搜索栏：浮在终端右上角的卡片
fn render_search_bar(
    search_input: &Entity<InputState>,
    (current, total): (usize, usize),
    has_query: bool,
    session_state: Entity<SessionState>,
    lang: &Language,
    p: &Palette,
) -> impl IntoElement {
    // 有关键字但没有命中时，计数标红
    let count_color = if has_query && total == 0 {
        p.danger
    } else {
        p.text_faint
    };
    let step = |id: &'static str, icon: &'static str, tooltip: &'static str, forward: bool| {
        let session_state = session_state.clone();
        icon_button_sized(id, icon, px(24.), 13., i18n::t(lang, tooltip), p).on_click(
            move |_, _, cx| {
                session_state.update(cx, |state, cx| {
                    if let Some(terminal) = state.active_terminal_entity() {
                        terminal.update(cx, |t, cx| {
                            if forward {
                                t.search_next_match();
                            } else {
                                t.search_prev_match();
                            }
                            cx.notify();
                        });
                    }
                    cx.notify();
                });
            },
        )
    };

    div()
        .id("terminal-search-bar")
        .absolute()
        .top(px(8.))
        .right(px(16.))
        .h(px(36.))
        .pl(px(10.))
        .pr(px(4.))
        .flex()
        .items_center()
        .gap(px(4.))
        .rounded(px(8.))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .shadow_md()
        .cursor_default()
        // 阻止在搜索栏上的鼠标操作触发终端选择
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(icon_sized(icons::SEARCH, 13., p.text_faint))
        .child(
            div().w(px(180.)).child(
                Input::new(search_input)
                    .appearance(false)
                    .text_size(px(13.)),
            ),
        )
        .child(
            div()
                .flex_none()
                .min_w(px(36.))
                .text_size(px(12.))
                .text_color(count_color)
                .text_right()
                .child(format!("{current}/{total}")),
        )
        .child(
            div()
                .flex_none()
                .w(px(1.))
                .h(px(16.))
                .mx(px(2.))
                .bg(p.border),
        )
        .child(step(
            "terminal-search-prev",
            icons::CHEVRON_UP,
            "terminal.search.previous",
            false,
        ))
        .child(step(
            "terminal-search-next",
            icons::CHEVRON_DOWN,
            "terminal.search.next",
            true,
        ))
        .child({
            let session_state = session_state.clone();
            icon_button_sized(
                "terminal-search-close",
                icons::X,
                px(24.),
                13.,
                i18n::t(lang, "terminal.search.close"),
                p,
            )
            .on_click(move |_, window, cx| {
                session_state.update(cx, |state, cx| state.close_terminal_search(window, cx));
            })
        })
}

/// 状态卡片所在的层：盖住整个终端区域的遮罩，卡片居中
fn render_overlay_layer(card: AnyElement, p: &Palette) -> Div {
    div()
        .absolute()
        .inset_0()
        .bg(p.overlay)
        .flex()
        .items_center()
        .justify_center()
        .p(px(16.))
        .child(card)
}

/// 终端还没打开时的提示：终端配色上的加载图标和文字
fn render_initializing_layer(
    settings: &crate::models::settings::TerminalSettings,
    lang: &Language,
) -> Div {
    let fg = hex_to_hsla(&settings.foreground_color).opacity(0.6);
    div()
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .child(spinner("terminal-initializing", 20., fg))
        .child(
            div()
                .text_size(px(13.))
                .text_color(fg)
                .child(i18n::t(lang, "session.terminal.placeholder")),
        )
}

/// 状态卡片上的操作按钮
struct StatusAction {
    id: &'static str,
    label: &'static str,
    icon: &'static str,
    on_click: Box<dyn Fn(&mut App) + 'static>,
}

/// 状态卡片左上的图形
enum StatusIcon {
    Icon(&'static str),
    Spinner,
}

/// 叠加在终端历史之上的状态卡片：与弹窗同款的外框，状态色图标块、标题、说明和一个主按钮
fn render_status_card(
    icon: StatusIcon,
    tone: Hsla,
    title: &'static str,
    detail: Option<String>,
    action: Option<StatusAction>,
    p: &Palette,
) -> Div {
    div()
        .w(px(360.))
        .max_w_full()
        .p(px(20.))
        .rounded(px(12.))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .shadow_lg()
        .cursor_default()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .flex_none()
                .size(px(40.))
                .rounded(px(10.))
                .bg(tone.opacity(0.14))
                .flex()
                .items_center()
                .justify_center()
                .child(match icon {
                    StatusIcon::Icon(path) => icon_sized(path, 20., tone).into_any_element(),
                    StatusIcon::Spinner => {
                        spinner("terminal-status-spinner", 20., tone).into_any_element()
                    }
                }),
        )
        .child(
            div()
                .mt(px(4.))
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text)
                .text_center()
                .child(title),
        )
        .children(
            detail
                .filter(|detail| !detail.trim().is_empty())
                .map(|detail| {
                    div()
                        .text_size(px(12.))
                        .text_color(p.text_muted)
                        .text_center()
                        .child(detail)
                }),
        )
        .children(action.map(|action| {
            let on_click = action.on_click;
            div().mt(px(8.)).child(
                primary_button(action.id, action.icon, action.label, p)
                    .on_click(move |_, _, cx| on_click(cx)),
            )
        }))
}

/// 自动重连中
fn render_reconnecting_overlay(
    attempt: u32,
    max_attempts: u32,
    lang: &Language,
    p: &Palette,
) -> Div {
    render_status_card(
        StatusIcon::Spinner,
        p.warning,
        i18n::t(lang, "terminal.reconnecting"),
        Some(format!(
            "{} {}/{}",
            i18n::t(lang, "terminal.reconnect_attempt"),
            attempt,
            max_attempts
        )),
        None,
        p,
    )
}

/// 连接已断开（自动重连失败或未开启自动重连），提供手动重连
fn render_disconnected_overlay(
    tab_id: String,
    reason: Option<String>,
    session_state: Entity<SessionState>,
    lang: &Language,
    p: &Palette,
) -> Div {
    render_status_card(
        StatusIcon::Icon(icons::UNPLUG),
        p.warning,
        i18n::t(lang, "terminal.disconnected"),
        reason,
        Some(StatusAction {
            id: "reconnect-btn",
            label: i18n::t(lang, "terminal.reconnect"),
            icon: icons::REFRESH,
            on_click: Box::new(move |cx| {
                crate::ssh::start_manual_reconnection(tab_id.clone(), session_state.clone(), cx);
            }),
        }),
        p,
    )
}

/// 远端 shell 已退出（连接仍在），可重新启动
fn render_shell_exited_overlay(
    tab_id: String,
    terminal_id: String,
    session_state: Entity<SessionState>,
    lang: &Language,
    p: &Palette,
) -> Div {
    render_status_card(
        StatusIcon::Icon(icons::TERMINAL),
        p.text_muted,
        i18n::t(lang, "terminal.shell_exited"),
        None,
        Some(StatusAction {
            id: "restart-shell-btn",
            label: i18n::t(lang, "terminal.restart_shell"),
            icon: icons::REFRESH,
            on_click: Box::new(move |cx| {
                session_state.update(cx, |state, cx| {
                    state.restart_terminal_instance(&tab_id, &terminal_id);
                    cx.notify();
                });
            }),
        }),
        p,
    )
}

/// 打开终端失败（连接正常，如服务器拒绝分配 PTY）
fn render_open_failed_overlay(
    message: String,
    tab_id: String,
    terminal_id: String,
    session_state: Entity<SessionState>,
    lang: &Language,
    p: &Palette,
) -> Div {
    render_status_card(
        StatusIcon::Icon(icons::X),
        p.danger,
        i18n::t(lang, "terminal.open_failed"),
        Some(message),
        Some(StatusAction {
            id: "retry-terminal-btn",
            label: i18n::t(lang, "terminal.retry"),
            icon: icons::REFRESH,
            on_click: Box::new(move |cx| {
                session_state.update(cx, |state, cx| {
                    state.restart_terminal_instance(&tab_id, &terminal_id);
                    cx.notify();
                });
            }),
        }),
        p,
    )
}

/// 把命令框里的内容发送到终端（追加回车），然后清空命令框
fn send_command(
    input: &Entity<InputState>,
    channel: &Arc<TerminalChannel>,
    terminal: Option<&Entity<TerminalState>>,
    window: &mut Window,
    cx: &mut App,
) {
    let content = input.read(cx).value().to_string();
    if content.is_empty() {
        return;
    }

    let mut bytes = content.into_bytes();
    bytes.push(0x0d); // CR (回车)

    // 重置光标可见（有输入时）
    if let Some(terminal) = terminal {
        terminal.update(cx, |t, _| t.show_cursor());
    }

    if let Err(e) = channel.queue_write(bytes) {
        tracing::error!("[Terminal] PTY write error: {:?}", e);
    }

    input.update(cx, |state, cx| {
        state.set_value(String::new(), window, cx);
    });
}

/// 渲染命令输入区域：带底色和边框的多行输入框（自动增高），右侧是发送按钮（输入为空时禁用）
fn render_command_input(
    command_input: Option<Entity<InputState>>,
    pty_channel: Option<Arc<TerminalChannel>>,
    terminal: Option<Entity<TerminalState>>,
    lang: &Language,
    p: &Palette,
    cx: &App,
) -> impl IntoElement {
    let can_send = pty_channel.is_some()
        && command_input
            .as_ref()
            .is_some_and(|input| !input.read(cx).value().trim().is_empty());

    let send_button = icon_button_custom(
        "send-command-btn",
        icons::SEND,
        IconButtonLook {
            size: ICON_BUTTON_SIZE,
            icon_size: 14.,
            icon_color: p.primary,
            hover_icon_color: p.primary,
            rest_bg: None,
            enabled: can_send,
        },
        i18n::t(lang, "terminal.send"),
        p,
    )
    .when(can_send, |button| {
        let input = command_input.clone();
        let channel = pty_channel.clone();
        let terminal = terminal.clone();
        button.on_click(move |_, window, cx| {
            if let (Some(input), Some(channel)) = (input.as_ref(), channel.as_ref()) {
                send_command(input, channel, terminal.as_ref(), window, cx);
            }
        })
    });

    div()
        .id("command-input-area")
        .flex_shrink_0()
        .px(px(8.))
        .py(px(6.))
        .bg(p.background)
        .border_t_1()
        .border_color(p.border)
        .flex()
        .items_end()
        .gap(px(6.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .children(command_input.as_ref().map(|input| text_area(input))),
        )
        .child(div().flex_none().pb(px(1.)).child(send_button))
}
