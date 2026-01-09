// SSH 自动重连模块
//
// 当 SSH 连接断开时，自动尝试重新连接

use gpui::{App, Entity};
use tracing::{debug, error, info, warn};

use crate::models::server::ServerData;
use crate::state::{SessionState, SessionStatus};

use super::config::{AuthMethod, KeepaliveConfig, SshConfig};
use super::event::{ConnectionEvent, HostKeyAction};

/// 从 ServerData 构建 SshConfig（复用 connector 中的逻辑）
fn build_ssh_config(server: &ServerData) -> SshConfig {
    let auth = match &server.auth_type {
        crate::models::server::AuthType::Password => {
            AuthMethod::Password(server.password_encrypted.clone().unwrap_or_default())
        }
        crate::models::server::AuthType::PublicKey => AuthMethod::PublicKey {
            key_path: server.private_key_path.clone().unwrap_or_default().into(),
            passphrase: server.key_passphrase_encrypted.clone(),
        },
    };

    let settings = crate::services::storage::load_settings().unwrap_or_default();
    let connection_settings = &settings.connection;

    let keepalive = KeepaliveConfig {
        enabled: connection_settings.keepalive_interval_secs > 0,
        interval: connection_settings.keepalive_interval_secs as u64,
        max_retries: 3,
    };

    SshConfig {
        host: server.host.clone(),
        port: server.port,
        username: server.username.clone(),
        auth,
        connect_timeout: connection_settings.connection_timeout_secs as u64,
        jump_host: None,
        proxy: None,
        keepalive,
    }
}

/// 启动自动重连
///
/// 在后台尝试重新连接 SSH 会话，直到成功或达到最大重试次数
pub fn start_reconnection(
    server: ServerData,
    tab_id: String,
    terminal_id: String,
    session_state: Entity<SessionState>,
    cx: &App,
) {
    let settings = crate::services::storage::load_settings().unwrap_or_default();
    let max_attempts = settings.connection.reconnect_attempts;
    let interval_secs = settings.connection.reconnect_interval_secs;
    let server_label = server.label.clone();

    info!(
        "[Reconnect] Starting auto-reconnect for {} (max {} attempts, {}s interval)",
        server_label, max_attempts, interval_secs
    );

    cx.spawn(async move |async_cx| {
        let mut attempt = 1u32;

        while attempt <= max_attempts {
            // 更新 UI 显示当前重连尝试
            let tab_id_clone = tab_id.clone();
            let _ = async_cx.update(|cx| {
                session_state.update(cx, |state, cx| {
                    if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_clone) {
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

            // 尝试连接
            let config = build_ssh_config(&server);
            let connection_handle =
                crate::ssh::SshManager::global().connect(config, tab_id.clone());

            // 获取 host_key_tx 用于自动响应（包装为 Option 以便消费后设为 None）
            let mut host_key_tx = Some(connection_handle.host_key_tx);
            let mut event_rx = connection_handle.event_rx;

            // 连接结果
            let mut connected = false;
            let mut error_msg = String::new();

            // 处理连接事件
            while let Some(event) = event_rx.recv().await {
                match event {
                    ConnectionEvent::Connected { session_id } => {
                        info!(
                            "[Reconnect] [{}] Successfully reconnected! Session: {}",
                            server_label, session_id
                        );
                        connected = true;
                        break;
                    }
                    ConnectionEvent::Failed { error } => {
                        error_msg = error;
                        break;
                    }
                    ConnectionEvent::Disconnected { reason } => {
                        error_msg = reason;
                        break;
                    }
                    ConnectionEvent::HostKeyVerification { .. } => {
                        // 重连时自动接受已保存的主机密钥
                        if let Some(tx) = host_key_tx.take() {
                            debug!("[Reconnect] Auto-accepting known host key");
                            let _ = tx.send(HostKeyAction::AcceptOnce);
                        }
                    }
                    ConnectionEvent::HostKeyMismatch { .. } => {
                        // 密钥不匹配时拒绝（安全考虑）
                        error_msg = "Host key mismatch - possible security risk".to_string();
                        if let Some(tx) = host_key_tx.take() {
                            let _ = tx.send(HostKeyAction::Reject);
                        }
                        break;
                    }
                    _ => {}
                }
            }

            if connected {
                // 更新状态为已连接
                let tab_id_clone = tab_id.clone();
                let terminal_id_clone = terminal_id.clone();
                let _ = async_cx.update(|cx| {
                    session_state.update(cx, |state, cx| {
                        if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_clone) {
                            tab.status = SessionStatus::Connected;

                            // 重置终端的 PTY 状态，等待重新初始化
                            if let Some(terminal) = tab
                                .terminals
                                .iter_mut()
                                .find(|t| t.id == terminal_id_clone)
                            {
                                terminal.pty_channel = None;
                                terminal.pty_initialized = false;
                                terminal.pty_error = None;
                            }
                        }

                        // 重启服务
                        state.start_monitor_service(tab_id_clone.clone(), cx);
                        state.start_sftp_service(tab_id_clone.clone(), cx);

                        cx.notify();
                    });

                    // 推送重连成功通知
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
                                .message(crate::i18n::t(&lang, "terminal.reconnected"))
                                .with_type(NotificationType::Success)
                                .w_48()
                                .py_2();
                            window.push_notification(notification, cx);
                        });
                    }
                });

                return; // 成功，退出重连循环
            } else {
                warn!(
                    "[Reconnect] [{}] Attempt {}/{} failed: {}",
                    server_label, attempt, max_attempts, error_msg
                );
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

        // 所有尝试都失败了
        error!(
            "[Reconnect] [{}] All {} attempts failed",
            server_label, max_attempts
        );

        let tab_id_clone = tab_id.clone();
        let _ = async_cx.update(|cx| {
            session_state.update(cx, |state, cx| {
                if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_clone) {
                    tab.status = SessionStatus::Disconnected;
                }
                cx.notify();
            });
        });
    })
    .detach();
}

/// 启动手动重连（用户点击重连按钮）
pub fn start_manual_reconnection(
    tab_id: String,
    terminal_id: String,
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
        start_reconnection(server, tab_id, terminal_id, session_state, cx);
    } else {
        error!("[Reconnect] No server_data found for tab {}", tab_id);
    }
}
