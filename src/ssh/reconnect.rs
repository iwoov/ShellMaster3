// SSH 断线处理与自动重连模块
//
// - watch_connection：监视会话本身，连接一断开（无论是否有终端打开）就进入断线处理
// - handle_connection_lost：断线处理的唯一入口，开启自动重连则重连，否则标记为已断开
// - start_reconnection：自动/手动重连

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{App, Entity, SharedString};
use gpui_component::notification::NotificationType;
use tracing::{debug, error, info, warn};

use crate::models::server::ServerData;
use crate::models::settings::Language;
use crate::services::storage;
use crate::state::{SessionState, SessionStatus, TerminalPtyState};

use super::config::{HostKeyPolicy, SshConfig};
use super::event::{ConnectionEvent, HostKeyAction};

/// 重连成功后至少保持这么久，再次断开时才继续自动重连；更快断开（如服务器一登录就踢掉）
/// 时改为提示用户手动重连，避免陷入“连上 → 断开 → 重连”的死循环。
/// 首次连接后的第一次断开总会自动重连。
const MIN_STABLE_CONNECTION: Duration = Duration::from_secs(10);

/// 监视标签页当前的 SSH 会话：连接一旦断开，就交给 [`handle_connection_lost`] 处理。
///
/// 在会话注册后调用（首次连接成功、每次重连成功后）。
pub fn watch_connection(tab_id: String, session_state: Entity<SessionState>, cx: &mut App) {
    let Some(session) = crate::ssh::SshManager::global().get_session(&tab_id) else {
        return;
    };
    cx.spawn(async move |async_cx| {
        session.closed().await;

        // 标签页已关闭，或会话已被新连接替换（重连成功）：这条连接的关闭无需处理
        let still_current = crate::ssh::SshManager::global()
            .get_session(&tab_id)
            .is_some_and(|current| Arc::ptr_eq(&current, &session));
        if !still_current {
            return;
        }

        let reason = session.close_reason();
        let _ = async_cx.update(|cx| handle_connection_lost(tab_id, session_state, reason, cx));
    })
    .detach();
}

/// 连接已断开（会话监视、终端读取结束或打开终端时发现）。
///
/// 开启了自动重连就开始重连；否则标记为“已断开”并提示，用户可以点“重新连接”。
/// 只处理“已连接”状态的标签页，重复调用是安全的。
pub fn handle_connection_lost(
    tab_id: String,
    session_state: Entity<SessionState>,
    reason: Option<String>,
    cx: &mut App,
) {
    let Some(tab) = session_state.read(cx).tabs.iter().find(|t| t.id == tab_id) else {
        return;
    };
    // 重连中或已标记断开：说明已经在处理
    if tab.status != SessionStatus::Connected {
        return;
    }
    let server = tab.server_data.clone();
    let dropped_quickly = tab
        .reconnected_at
        .is_some_and(|reconnected_at| reconnected_at.elapsed() < MIN_STABLE_CONNECTION);

    warn!(
        "[Reconnect] Connection of tab {} lost: {}",
        tab_id,
        reason.as_deref().unwrap_or("unknown reason")
    );

    let settings = storage::load_settings().unwrap_or_default();
    let lang = settings.theme.language.clone();
    let reason = reason.map(|reason| describe_reason(&reason, &lang));
    match server {
        Some(server) if settings.connection.auto_reconnect && !dropped_quickly => {
            session_state.update(cx, |state, _| {
                if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.disconnect_reason = reason;
                }
            });
            start_reconnection(server, tab_id, session_state, cx);
        }
        _ => {
            let detail = if dropped_quickly && settings.connection.auto_reconnect {
                Some(
                    crate::i18n::t(&lang, "terminal.dropped_after_connect")
                        .replace("{}", reason.as_deref().unwrap_or("-")),
                )
            } else {
                reason
            };
            session_state.update(cx, |state, cx| {
                if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.status = SessionStatus::Disconnected;
                    tab.disconnect_reason = detail;
                }
                cx.notify();
            });
            push_notification(
                cx,
                crate::i18n::t(&lang, "terminal.disconnected"),
                NotificationType::Warning,
            );
        }
    }
}

/// 把断开/重连失败的原因（russh 或系统错误信息）转成用户看得懂的说明；
/// 认不出的原样保留，不丢信息
fn describe_reason(reason: &str, lang: &Language) -> String {
    let t = |key| crate::i18n::t(lang, key);
    let lower = reason.trim().to_lowercase();

    if let Some(rest) = reason.trim().strip_prefix("the server closed the connection") {
        let message = rest.trim_start_matches(':').trim();
        return if message.is_empty() {
            t("terminal.reason.server_closed").to_string()
        } else {
            t("terminal.reason.server_closed_with").replace("{}", message)
        };
    }

    let key = if lower.starts_with("authentication failed") {
        "terminal.reason.auth"
    } else if lower.starts_with("host key verification failed") {
        "terminal.reason.host_key"
    } else if lower.contains("keepalive") {
        "terminal.reason.no_response"
    } else if lower.contains("connection refused") {
        "terminal.reason.refused"
    } else if lower.starts_with("connection timeout") || lower.contains("timed out") {
        "terminal.reason.timeout"
    } else if lower.contains("failed to resolve") || lower.contains("nodename nor servname") {
        "terminal.reason.resolve"
    } else if lower.contains("network is unreachable")
        || lower.contains("no route to host")
        || lower.contains("host is down")
    {
        "terminal.reason.unreachable"
    } else if lower.contains("early eof")
        || lower.contains("connection reset")
        || lower.contains("broken pipe")
        || lower.contains("connection aborted")
        || lower.contains("session disconnected")
        || lower == "disconnected"
    {
        "terminal.reason.interrupted"
    } else {
        return reason.trim().to_string();
    };
    t(key).to_string()
}

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
        let mut last_error: Option<String> = None;

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
                                tab.reconnected_at = Some(Instant::now());
                                tab.disconnect_reason = None;

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

                        // 在每个终端的历史里画一条分隔线，标出旧会话结束、新会话开始的位置
                        // （只写入本地终端显示，不发送到服务器）
                        let lang = storage::load_settings()
                            .map(|s| s.theme.language)
                            .unwrap_or_default();
                        let marker = format!(
                            "\r\n\x1b[90m── {} ──\x1b[0m\r\n",
                            crate::i18n::t(&lang, "terminal.reconnected")
                        );
                        let terminals: Vec<_> = session_state
                            .read(cx)
                            .tabs
                            .iter()
                            .find(|t| t.id == tab_id)
                            .map(|tab| {
                                tab.terminals
                                    .iter()
                                    .filter_map(|terminal| terminal.terminal.clone())
                                    .collect()
                            })
                            .unwrap_or_default();
                        for terminal in terminals {
                            terminal.update(cx, |terminal, cx| {
                                let _ = terminal.input_str(&marker);
                                cx.notify();
                            });
                        }

                        // 继续监视新连接
                        watch_connection(tab_id.clone(), session_state.clone(), cx);

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
                    last_error = Some(error);
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
            // 断开提示里显示最后的失败原因
            let lang = storage::load_settings()
                .map(|s| s.theme.language)
                .unwrap_or_default();
            let stop_reason = stop_reason.map(|reason| describe_reason(&reason, &lang));
            let reason = stop_reason
                .clone()
                .or_else(|| last_error.map(|reason| describe_reason(&reason, &lang)));
            session_state.update(cx, |state, cx| {
                if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.status = SessionStatus::Disconnected;
                    tab.disconnect_reason = reason;
                    tab_exists = true;
                }
                cx.notify();
            });
            if !tab_exists {
                return;
            }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_disconnect_reasons_are_readable() {
        let zh = Language::Chinese;
        assert_eq!(
            describe_reason("early eof", &zh),
            crate::i18n::t(&zh, "terminal.reason.interrupted")
        );
        assert_eq!(
            describe_reason("IO error: Connection refused (os error 61)", &zh),
            crate::i18n::t(&zh, "terminal.reason.refused")
        );
        assert_eq!(
            describe_reason("Connection timeout after 30s", &zh),
            crate::i18n::t(&zh, "terminal.reason.timeout")
        );
        assert_eq!(
            describe_reason("the server closed the connection: server maintenance", &zh),
            "服务器关闭了连接：server maintenance"
        );
        assert_eq!(
            describe_reason("the server closed the connection", &zh),
            crate::i18n::t(&zh, "terminal.reason.server_closed")
        );
        // 认不出的原样保留
        assert_eq!(describe_reason("  something odd  ", &zh), "something odd");
    }
}
