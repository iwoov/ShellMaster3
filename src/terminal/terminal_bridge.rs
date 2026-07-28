// 终端桥接器 - 连接 PTY channel 和终端状态
// 负责尺寸计算、PTY 数据读取循环

use std::sync::Arc;

use gpui::{
    px, App, AsyncApp, ClipboardItem, Entity, Font, FontFeatures, FontStyle,
    FontWeight as GpuiFontWeight, Pixels, Window,
};
use tracing::{debug, error, info, trace, warn};

use crate::models::settings::{
    BellStyle, FontWeight as TerminalFontWeight, Osc52Policy, TerminalSettings,
};
use crate::ssh::session::{PtyRequest, TerminalChannel};
use crate::state::{SessionState, SessionStatus};
use crate::terminal::TerminalState;

/// 返回真正可用于终端网格的像素宽度，统一扣除左右内容起点的 padding。
pub fn terminal_content_width(area_width: f32, padding: u32, cell_width: f32) -> f32 {
    (area_width - padding as f32).max(cell_width)
}

/// 使用 GPUI text_system 精确计算终端尺寸
///
/// 通过测量字体中 'm' 字符的实际 advance width 来精确计算终端的列数和行数
pub fn calculate_terminal_size(
    area_width: f32,
    area_height: f32,
    settings: &TerminalSettings,
    window: &Window,
    _cx: &App,
) -> (u32, u32, f32, f32) {
    let text_system = window.text_system();

    // 构建字体
    let font = Font {
        family: settings.font_family.clone().into(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: match settings.font_weight {
            TerminalFontWeight::Normal => GpuiFontWeight::NORMAL,
            TerminalFontWeight::Bold => GpuiFontWeight::BOLD,
        },
        style: FontStyle::Normal,
    };

    // 解析字体 ID
    let font_id = text_system.resolve_font(&font);
    let font_size: Pixels = px(settings.font_size as f32);

    // 精确测量 'm' 字符的 advance width 作为 cell_width
    let cell_width = text_system
        .advance(font_id, font_size, 'm')
        .map(|size| f32::from(size.width))
        .unwrap_or_else(|_| {
            // fallback: 估算值
            warn!("[Terminal] Warning: Failed to measure font advance, using estimation");
            settings.font_size as f32 * 0.6
        });

    // 行高计算
    let line_height = settings.font_size as f32 * settings.line_height;

    // 计算列数时减去左侧 padding 宽度
    let effective_width = terminal_content_width(area_width, settings.padding, cell_width);
    let cols = (effective_width / cell_width).floor() as u32;
    let rows = (area_height / line_height).floor() as u32;

    debug!(
        "[Terminal] Precise size calculation: cell_width={:.2}px, line_height={:.2}px, cols={}, rows={} (with padding={}px)",
        cell_width, line_height, cols.max(1), rows.max(1), settings.padding
    );

    (cols.max(1), rows.max(1), cell_width, line_height)
}

/// 根据已计算的尺寸创建 PTY 请求
pub fn create_pty_request(
    cols: u32,
    rows: u32,
    cell_width: f32,
    line_height: f32,
    settings: &TerminalSettings,
) -> PtyRequest {
    let term_type = settings.term_type.trim();
    PtyRequest {
        term: if term_type.is_empty() {
            "xterm-256color".to_string()
        } else {
            term_type.to_string()
        },
        col_width: cols,
        row_height: rows,
        pix_width: (cols as f32 * cell_width) as u32,
        pix_height: (rows as f32 * line_height) as u32,
        modes: vec![],
        command: terminal_command(settings),
    }
}

fn terminal_command(settings: &TerminalSettings) -> Option<String> {
    let shell = settings.default_shell.trim();
    if shell.is_empty() {
        return None;
    }
    let args = settings.shell_args.trim();
    Some(if args.is_empty() {
        shell.to_string()
    } else {
        format!("{} {}", shell, args)
    })
}

/// 启动 PTY 读取循环 (fire-and-forget)
/// 读取循环会持续运行直到通道关闭
pub fn start_pty_reader(
    channel: Arc<TerminalChannel>,
    terminal: Entity<TerminalState>,
    session_state: Entity<SessionState>,
    tab_id: String,
    terminal_id: String,
    cx: &App,
) {
    // 使用与 connector.rs 相同的 spawn 模式
    cx.spawn(async move |async_cx| {
        debug!("[PTY Reader] Started");

        let channel_closed_cleanly = loop {
            // 读取 PTY 输出
            let result = channel.read().await;
            match result {
                Ok(Some(data)) if !data.is_empty() => {
                    trace!("[PTY Reader] Received {} bytes", data.len());
                    // 将数据喂给终端，并收集需要回写 PTY 的字节（DA/DSR/CPR 回复、
                    // 颜色/尺寸查询、OSC 52 剪贴板读取等）。
                    let terminal_clone = terminal.clone();
                    let session_state_clone = session_state.clone();
                    let tab_id_for_title = tab_id.clone();
                    let terminal_id_for_title = terminal_id.clone();
                    let write_back = async_cx
                        .update(|cx| {
                            let (out, terminal_settings) = terminal_clone.update(cx, |t, cx| {
                                let out = t.input(&data);
                                let terminal_settings = t.settings().clone();
                                cx.notify();
                                (out, terminal_settings)
                            });

                            let mut bytes = out.pty_writes;

                            // OSC 52：应用请求读取剪贴板并回写 PTY
                            if matches!(terminal_settings.osc52_policy, Osc52Policy::ReadWrite) {
                                for formatter in &out.clipboard_load {
                                    let text = cx
                                        .read_from_clipboard()
                                        .and_then(|c| c.text())
                                        .unwrap_or_default();
                                    bytes.extend_from_slice(formatter(&text).as_bytes());
                                }
                            }

                            // OSC 52：应用请求写入剪贴板
                            if !matches!(terminal_settings.osc52_policy, Osc52Policy::Disabled) {
                                if let Some(store) = out.clipboard_store {
                                    cx.write_to_clipboard(ClipboardItem::new_string(store));
                                }
                            }

                            // 响铃：按 bell_style 触发视觉闪烁或系统声音。
                            if out.bell {
                                if matches!(terminal_settings.bell_style, BellStyle::Visual) {
                                    terminal_clone.update(cx, |t, cx| {
                                        t.set_bell_flash(true);
                                        cx.notify();
                                    });
                                    // 120ms 后清除闪烁（非阻塞）
                                    let terminal_for_bell = terminal_clone.clone();
                                    async_cx
                                        .spawn(async move |acx| {
                                            acx.background_executor()
                                                .timer(std::time::Duration::from_millis(120))
                                                .await;
                                            let _ = acx.update(|cx| {
                                                terminal_for_bell.update(cx, |t, cx| {
                                                    t.set_bell_flash(false);
                                                    cx.notify();
                                                });
                                            });
                                        })
                                        .detach();
                                } else if matches!(terminal_settings.bell_style, BellStyle::Sound) {
                                    play_bell_sound();
                                }
                            }

                            // OSC 0/2：窗口/标签标题
                            if let Some(title) = out.title {
                                let title: String = title.chars().take(128).collect();
                                session_state_clone.update(cx, |state, cx| {
                                    if let Some(tab) =
                                        state.tabs.iter_mut().find(|t| t.id == tab_id_for_title)
                                    {
                                        if let Some(inst) = tab
                                            .terminals
                                            .iter_mut()
                                            .find(|t| t.id == terminal_id_for_title)
                                        {
                                            inst.title = (!title.is_empty()).then_some(title);
                                        }
                                    }
                                    cx.notify();
                                });
                            }

                            bytes
                        })
                        .unwrap_or_default();

                    if !write_back.is_empty() {
                        if let Err(e) = channel.queue_write(write_back) {
                            error!("[PTY Reader] Write-back error: {:?}", e);
                        }
                    }
                }
                Ok(Some(_)) => {
                    // 空数据，短暂等待后继续
                    async_cx
                        .background_executor()
                        .timer(std::time::Duration::from_millis(10))
                        .await;
                }
                Ok(None) => {
                    debug!("[PTY Reader] Channel closed");
                    break true;
                }
                Err(e) => {
                    error!("[PTY Reader] Error: {:?}", e);
                    break false;
                }
            }
        };

        // 断开连接后处理
        {
            let tab_id_for_check = tab_id.clone();
            let terminal_id_for_check = terminal_id.clone();
            let terminal_still_present = async_cx
                .update(|cx| {
                    session_state
                        .read(cx)
                        .tabs
                        .iter()
                        .find(|t| t.id == tab_id_for_check)
                        .map(|tab| tab.terminals.iter().any(|t| t.id == terminal_id_for_check))
                        .unwrap_or(false)
                })
                .unwrap_or(false);

            if !terminal_still_present {
                debug!(
                    "[PTY Reader] Terminal {} in tab {} was closed by user; skip reconnect",
                    terminal_id, tab_id
                );
                return;
            }

            // 单个远端 shell 正常退出不等同于 SSH 传输断线。
            let transport_alive = crate::ssh::manager::SshManager::global()
                .get_session(&tab_id)
                .map(|session| session.is_alive())
                .unwrap_or(false);
            if channel_closed_cleanly && transport_alive {
                let tab_id_for_exit = tab_id.clone();
                let terminal_id_for_exit = terminal_id.clone();
                let _ = async_cx.update(|cx| {
                    session_state.update(cx, |state, cx| {
                        if let Some(instance) = state
                            .tabs
                            .iter_mut()
                            .find(|tab| tab.id == tab_id_for_exit)
                            .and_then(|tab| {
                                tab.terminals
                                    .iter_mut()
                                    .find(|terminal| terminal.id == terminal_id_for_exit)
                            })
                        {
                            instance.pty_channel = None;
                            instance.pty_state = crate::state::TerminalPtyState::Failed(
                                "terminal.shell_exited".to_string(),
                            );
                        }
                        cx.notify();
                    });
                });
                return;
            }

            // 读取设置和 server_data
            let (auto_reconnect, server_data) = async_cx
                .update(|cx| {
                    let settings = crate::services::storage::load_settings().unwrap_or_default();
                    let server_data = session_state
                        .read(cx)
                        .tabs
                        .iter()
                        .find(|t| t.id == tab_id)
                        .and_then(|t| t.server_data.clone());
                    (settings.connection.auto_reconnect, server_data)
                })
                .unwrap_or((false, None));

            if auto_reconnect {
                if let Some(server) = server_data {
                    info!(
                        "[PTY Reader] Connection lost, starting auto-reconnect for {}",
                        server.label
                    );

                    // 启动自动重连
                    let tab_id_clone = tab_id.clone();
                    let terminal_id_clone = terminal_id.clone();
                    let _ = async_cx.update(|cx| {
                        crate::ssh::start_reconnection(
                            server,
                            tab_id_clone,
                            terminal_id_clone,
                            session_state.clone(),
                            cx,
                        );
                    });
                } else {
                    warn!("[PTY Reader] No server_data available for reconnection");
                    set_disconnected_status(&async_cx, &session_state, &tab_id);
                }
            } else {
                info!("[PTY Reader] Auto-reconnect disabled, setting status to Disconnected");
                set_disconnected_status(&async_cx, &session_state, &tab_id);
            }
        }

        debug!("[PTY Reader] Stopped");
    })
    .detach();
}

fn play_bell_sound() {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("afplay")
        .arg("/System/Library/Sounds/Tink.aiff")
        .spawn();

    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("canberra-gtk-play")
        .args(["-i", "bell"])
        .spawn();

    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("rundll32")
        .arg("user32.dll,MessageBeep")
        .spawn();

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    match result {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(error) => debug!("[Terminal] Unable to play bell sound: {:?}", error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_width_subtracts_padding_and_never_collapses_grid() {
        assert_eq!(terminal_content_width(800.0, 8, 10.0), 792.0);
        assert_eq!(terminal_content_width(4.0, 8, 10.0), 10.0);
    }

    #[test]
    fn pty_request_uses_cell_aligned_pixels_and_custom_term() {
        let mut settings = TerminalSettings::default();
        settings.term_type = "screen-256color".to_string();
        let request = create_pty_request(80, 24, 9.5, 17.0, &settings);
        assert_eq!(request.term, "screen-256color");
        assert_eq!(request.pix_width, 760);
        assert_eq!(request.pix_height, 408);
    }

    #[test]
    fn custom_shell_is_forwarded_as_remote_command() {
        let mut settings = TerminalSettings::default();
        settings.default_shell = "/bin/zsh".to_string();
        settings.shell_args = "-l".to_string();
        let request = create_pty_request(80, 24, 8.0, 16.0, &settings);
        assert_eq!(request.command.as_deref(), Some("/bin/zsh -l"));
    }
}

/// 设置会话状态为断开并发送通知
fn set_disconnected_status(
    async_cx: &AsyncApp,
    session_state: &Entity<SessionState>,
    tab_id: &str,
) {
    let tab_id_clone = tab_id.to_string();
    let session_state_clone = session_state.clone();
    let _ = async_cx.update(|cx| {
        session_state_clone.update(cx, |state, cx| {
            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_clone) {
                tab.status = SessionStatus::Disconnected;
            }
            cx.notify();
        });

        // 推送断开通知
        if let Some(window) = cx.active_window() {
            use gpui::AppContext as _;
            let _ = cx.update_window(window, |_, window, cx| {
                use gpui::Styled;
                use gpui_component::notification::{Notification, NotificationType};
                use gpui_component::WindowExt;

                let lang = crate::services::storage::load_settings()
                    .map(|s| s.theme.language)
                    .unwrap_or_default();

                let notification = Notification::new()
                    .message(crate::i18n::t(&lang, "terminal.disconnected"))
                    .with_type(NotificationType::Warning)
                    .w_48()
                    .py_2();
                window.push_notification(notification, cx);
            });
        }
    });
}
