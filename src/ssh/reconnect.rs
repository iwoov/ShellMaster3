// SSH 自动重连模块
//
// 当 SSH 连接断开时，自动尝试重新连接

use gpui::{App, Entity, SharedString};
use gpui_component::notification::NotificationType;
use tracing::{debug, error, info, warn};

use crate::models::server::ServerData;
use crate::services::storage;
use crate::state::{SessionState, SessionStatus, TerminalPtyState};

use super::config::{HostKeyPolicy, SshConfig};
use super::event::{ConnectionEvent, HostKeyAction};

/// 单次重连尝试的结果
enum AttemptOutcome {
    Connected(ServerData),
    Failed { error: String, retryable: bool },
}

/// 启动自动重连
///
/// 在后台尝试重新连接 SSH 会话，直到成功、达到最大重试次数，
/// 或遇到重试也不会成功的错误（认证失败、主机密钥不符等）。
pub fn start_reconnection(
    server: ServerData,
    tab_id: String,
    session_state: Entity<SessionState>,
    cx: &mut App,
) {
    let settings = storage::load_settings().unwrap_or_default();
    let max_attempts = settings.connection.reconnect_attempts.max(1);
    let interval_secs = settings.connection.reconnect_interval_secs;
    let server_label = server.label.clone();

    // 同一会话只允许一个重连协调器，避免多个 PTY reader 同时建立重复连接。
    let already_reconnecting = session_state
        .read(cx)
        .tabs
        .iter()
        .find(|tab| tab.id == tab_id)
        .map(|tab| matches!(tab.status, SessionStatus::Reconnecting { .. }))
        .unwrap_or(true);
    if already_reconnecting {
        debug!(
            "[Reconnect] Reconnection already active or tab was closed: {}",
            tab_id
        );
        return;
    }
    session_state.update(cx, |state, cx| {
        if let Some(tab) = state.tabs.iter_mut().find(|tab| tab.id == tab_id) {
            tab.status = SessionStatus::Reconnecting {
                attempt: 1,
                max_attempts,
            };
        }
        cx.notify();
    });

    // 本标签页之前确认过的主机密钥：重连时只接受同一把密钥，其他一律拒绝
    let trusted = crate::ssh::SshManager::global()
        .get_session(&tab_id)
        .map(|session| session.verified_host_keys().clone())
        .unwrap_or_default();

    info!(
        "[Reconnect] Starting auto-reconnect for {} (max {} attempts, {}s interval)",
        server_label, max_attempts, interval_secs
    );

    cx.spawn(async move |async_cx| {
        let mut attempt = 1u32;
        let mut stop_reason: Option<String> = None;

        while attempt <= max_attempts {
            let tab_exists = async_cx
                .update(|cx| {
                    session_state
                        .read(cx)
                        .tabs
                        .iter()
                        .any(|tab| tab.id == tab_id)
                })
                .unwrap_or(false);
            if !tab_exists {
                debug!("[Reconnect] Tab {} closed; cancel reconnect loop", tab_id);
                return;
            }

            // 更新 UI 显示当前重连尝试
            let _ = async_cx.update(|cx| {
                session_state.update(cx, |state, cx| {
                    if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                        tab.status = SessionStatus::Reconnecting {
                            attempt,
                            max_attempts,
                        };
                    }
                    cx.notify();
                });
            });

            info!(
                "[Reconnect] [{}] Attempt {}/{}",
                server_label, attempt, max_attempts
            );

            match attempt_reconnect(&server, &tab_id, &trusted).await {
                AttemptOutcome::Connected(current) => {
                    info!("[Reconnect] [{}] Successfully reconnected", server_label);
                    let _ = async_cx.update(|cx| {
                        session_state.update(cx, |state, cx| {
                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                tab.status = SessionStatus::Connected;
                                tab.server_data = Some(current);

                                // 重置服务启动标志，让 PTY 创建时重新启动 Monitor/SFTP
                                tab.services_started = false;

                                // 重置本标签页下的所有终端，让它们在新连接上重新打开 PTY
                                // （各自保留终端历史；非当前终端在切换过去时再打开）
                                for terminal in &mut tab.terminals {
                                    terminal.pty_channel = None;
                                    terminal.pty_state = TerminalPtyState::NotStarted;
                                    terminal.last_sent_pty_size = None;
                                }
                            }
                            cx.notify();
                        });

                        let lang = storage::load_settings()
                            .map(|s| s.theme.language)
                            .unwrap_or_default();
                        push_notification(
                            cx,
                            crate::i18n::t(&lang, "terminal.reconnected"),
                            NotificationType::Success,
                        );
                    });
                    return;
                }
                AttemptOutcome::Failed { error, retryable } => {
                    warn!(
                        "[Reconnect] [{}] Attempt {}/{} failed: {}",
                        server_label, attempt, max_attempts, error
                    );
                    if !retryable {
                        stop_reason = Some(error);
                        break;
                    }
                }
            }

            attempt += 1;

            // 如果还有更多尝试，等待间隔时间
            if attempt <= max_attempts {
                debug!(
                    "[Reconnect] Waiting {}s before next attempt...",
                    interval_secs
                );
                async_cx
                    .background_executor()
                    .timer(std::time::Duration::from_secs(interval_secs as u64))
                    .await;
            }
        }

        match &stop_reason {
            Some(reason) => error!(
                "[Reconnect] [{}] Stopped reconnecting: {}",
                server_label, reason
            ),
            None => error!(
                "[Reconnect] [{}] All {} attempts failed",
                server_label, max_attempts
            ),
        }

        let _ = async_cx.update(|cx| {
            let mut tab_exists = false;
            session_state.update(cx, |state, cx| {
                if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.status = SessionStatus::Disconnected;
                    tab_exists = true;
                }
                cx.notify();
            });
            if !tab_exists {
                return;
            }

            let lang = storage::load_settings()
                .map(|s| s.theme.language)
                .unwrap_or_default();
            let message: SharedString = match stop_reason {
                Some(reason) => crate::i18n::t(&lang, "terminal.reconnect_stopped")
                    .replace("{}", &reason)
                    .into(),
                None => crate::i18n::t(&lang, "terminal.reconnect_failed").into(),
            };
            push_notification(cx, message, NotificationType::Error);
        });
    })
    .detach();
}

/// 进行一次重连尝试
async fn attempt_reconnect(
    server: &ServerData,
    tab_id: &str,
    trusted: &std::collections::HashMap<String, String>,
) -> AttemptOutcome {
    // 每次都重新读取配置：用户可能刚修改了密码、代理或跳板机
    let settings = storage::load_settings().unwrap_or_default();
    let servers = storage::load_servers()
        .map(|config| config.servers)
        .unwrap_or_default();
    let current = servers
        .iter()
        .find(|s| s.id == server.id)
        .cloned()
        .unwrap_or_else(|| server.clone());

    let policy = HostKeyPolicy {
        interactive: false,
        strict: settings.connection.strict_host_key_checking,
        trusted: trusted.clone(),
    };
    let config = match SshConfig::from_server(&current, &settings.connection, &servers, policy) {
        Ok(config) => config,
        Err(e) => {
            return AttemptOutcome::Failed {
                error: e.to_string(),
                retryable: e.is_retryable(),
            }
        }
    };

    let mut event_rx = crate::ssh::SshManager::global()
        .connect(config, tab_id.to_string())
        .event_rx;

    while let Some(event) = event_rx.recv().await {
        match event {
            ConnectionEvent::Connected { session_id } => {
                debug!("[Reconnect] Session {} registered", session_id);
                return AttemptOutcome::Connected(current);
            }
            ConnectionEvent::Failed { error, retryable } => {
                return AttemptOutcome::Failed { error, retryable };
            }
            // 非交互策略下不会请求确认；即便出现也一律拒绝
            ConnectionEvent::HostKeyVerification { responder, .. }
            | ConnectionEvent::HostKeyMismatch { responder, .. } => {
                responder.respond(HostKeyAction::Reject);
            }
            ConnectionEvent::StageChanged(_) | ConnectionEvent::Log(_) => {}
        }
    }

    // 事件通道提前关闭：连接任务被取消（例如标签页已关闭）
    AttemptOutcome::Failed {
        error: "connection attempt was cancelled".to_string(),
        retryable: true,
    }
}

/// 在当前窗口推送通知
fn push_notification(cx: &mut App, message: impl Into<SharedString>, kind: NotificationType) {
    use gpui::{px, AppContext as _, Styled};
    use gpui_component::notification::Notification;
    use gpui_component::WindowExt;

    let Some(window) = cx.active_window() else {
        return;
    };
    let message = message.into();
    // 错误通知带有原因，给更宽的空间
    let is_error = matches!(kind, NotificationType::Error);
    let _ = cx.update_window(window, |_, window, cx| {
        let notification = Notification::new().message(message).with_type(kind);
        let notification = if is_error {
            notification.w(px(360.)).py_2()
        } else {
            notification.w_48().py_2()
        };
        window.push_notification(notification, cx);
    });
}

/// 启动手动重连（用户点击重连按钮）
pub fn start_manual_reconnection(
    tab_id: String,
    session_state: Entity<SessionState>,
    cx: &mut App,
) {
    // 从 session tab 中获取 server_data
    let server_data = session_state
        .read(cx)
        .tabs
        .iter()
        .find(|t| t.id == tab_id)
        .and_then(|t| t.server_data.clone());

    if let Some(server) = server_data {
        info!(
            "[Reconnect] Manual reconnect requested for {}",
            server.label
        );
        start_reconnection(server, tab_id, session_state, cx);
    } else {
        error!("[Reconnect] No server_data found for tab {}", tab_id);
    }
}
