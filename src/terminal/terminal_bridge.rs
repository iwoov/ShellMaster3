// 终端桥接器 - 连接 PTY channel 和终端状态
// 负责尺寸计算、PTY 数据读取循环

use std::sync::Arc;

use gpui::*;
use tracing::{debug, error, info, trace, warn};

use crate::models::settings::TerminalSettings;
use crate::ssh::session::{PtyRequest, TerminalChannel};
use crate::state::{SessionState, SessionStatus};
use crate::terminal::{TerminalState, TERMINAL_PADDING_LEFT};

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
        weight: FontWeight::NORMAL,
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
    let effective_width = area_width - TERMINAL_PADDING_LEFT;
    let cols = (effective_width / cell_width).floor() as u32;
    let rows = (area_height / line_height).floor() as u32;

    debug!(
        "[Terminal] Precise size calculation: cell_width={:.2}px, line_height={:.2}px, cols={}, rows={} (with padding={}px)",
        cell_width, line_height, cols.max(1), rows.max(1), TERMINAL_PADDING_LEFT
    );

    (cols.max(1), rows.max(1), cell_width, line_height)
}

/// 根据已计算的尺寸创建 PTY 请求
pub fn create_pty_request(cols: u32, rows: u32, pix_width: f32, pix_height: f32) -> PtyRequest {
    PtyRequest {
        term: "xterm-256color".to_string(),
        col_width: cols,
        row_height: rows,
        pix_width: pix_width as u32,
        pix_height: pix_height as u32,
        modes: vec![],
    }
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

        let disconnect_reason = loop {
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
                            let out =
                                terminal_clone.update(cx, |t, cx| {
                                    let out = t.input(&data);
                                    cx.notify();
                                    out
                                });

                            let mut bytes = out.pty_writes;

                            // OSC 52：应用请求读取剪贴板并回写 PTY
                            for formatter in &out.clipboard_load {
                                let text = cx
                                    .read_from_clipboard()
                                    .and_then(|c| c.text())
                                    .unwrap_or_default();
                                bytes.extend_from_slice(formatter(&text).as_bytes());
                            }

                            // OSC 52：应用请求写入剪贴板
                            if let Some(store) = out.clipboard_store {
                                cx.write_to_clipboard(ClipboardItem::new_string(store));
                            }

                            // 响铃：按 bell_style 触发视觉闪烁（声音暂以视觉反馈代替）
                            if out.bell {
                                let bell_style = crate::services::storage::load_settings()
                                    .map(|s| s.terminal.bell_style)
                                    .unwrap_or_default();
                                if !matches!(
                                    bell_style,
                                    crate::models::settings::BellStyle::None
                                ) {
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
                                }
                            }

                            // OSC 0/2：窗口/标签标题
                            if let Some(title) = out.title {
                                session_state_clone.update(cx, |state, cx| {
                                    if let Some(tab) = state
                                        .tabs
                                        .iter_mut()
                                        .find(|t| t.id == tab_id_for_title)
                                    {
                                        if let Some(inst) = tab
                                            .terminals
                                            .iter_mut()
                                            .find(|t| t.id == terminal_id_for_title)
                                        {
                                            inst.title =
                                                (!title.is_empty()).then_some(title);
                                        }
                                    }
                                    cx.notify();
                                });
                            }

                            bytes
                        })
                        .unwrap_or_default();

                    if !write_back.is_empty() {
                        if let Err(e) = channel.write(&write_back).await {
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
                    break Some("terminal.disconnected".to_string());
                }
                Err(e) => {
                    error!("[PTY Reader] Error: {:?}", e);
                    break Some(format!("{:?}", e));
                }
            }
        };

        // 断开连接后处理
        if disconnect_reason.is_some() {
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

/// 发送数据到 PTY
pub async fn send_to_pty(channel: &TerminalChannel, data: &[u8]) {
    if let Err(e) = channel.write(data).await {
        error!("[PTY] Write error: {:?}", e);
    }
}
