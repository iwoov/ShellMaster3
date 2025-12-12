// SSH 连接启动器
// 负责从 UI 层接收连接请求，并启动异步连接任务

use gpui::{App, Entity};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::models::ServerData;
use crate::pages::connecting::ConnectingProgress;
use crate::state::{SessionState, SessionStatus};

use super::config::{AuthMethod, SshConfig};
use super::event::{ConnectionEvent, ConnectionStage, LogEntry};

/// 从 ServerData 构建 SshConfig
fn build_ssh_config(server: &ServerData) -> SshConfig {
    let auth = match &server.auth_type {
        crate::models::server::AuthType::Password => {
            // 密码需要解密（暂时直接使用加密的值，后续实现解密）
            AuthMethod::Password(server.password_encrypted.clone().unwrap_or_default())
        }
        crate::models::server::AuthType::PublicKey => AuthMethod::PublicKey {
            key_path: server.private_key_path.clone().unwrap_or_default().into(),
            passphrase: server.key_passphrase_encrypted.clone(),
        },
    };

    SshConfig {
        host: server.host.clone(),
        port: server.port,
        username: server.username.clone(),
        auth,
        connect_timeout: 30,
        jump_host: None, // TODO: 从 server.jump_host_id 加载
        proxy: None,     // TODO: 从 server.proxy 转换
        keepalive: Default::default(),
    }
}

/// UI 更新消息
enum UiUpdate {
    Stage(ConnectionStage),
    Log(LogEntry),
    Connected(String),
    Failed(String),
    Disconnected(String),
    /// 需要用户确认未知主机
    HostKeyVerification {
        host: String,
        port: u16,
        key_type: String,
        fingerprint: String,
    },
    /// 主机密钥变化警告
    HostKeyMismatch {
        host: String,
        port: u16,
        expected_fingerprint: String,
        actual_fingerprint: String,
    },
}

/// 启动 SSH 连接
///
/// 在 Connecting 页面渲染完成后调用此函数
/// 流程：页面挂载 -> 300ms初始动画 -> 发起后端连接 -> (host key确认) -> 连接成功后300ms成功动画 -> 跳转session
pub fn start_ssh_connection(
    server: ServerData,
    tab_id: String,
    progress_state: Entity<ConnectingProgress>,
    session_state: Entity<SessionState>,
    cx: &mut App,
) {
    // 构建 SSH 配置
    let config = build_ssh_config(&server);
    let server_label = server.label.clone();
    let server_id = server.id.clone();

    // 克隆用于异步任务
    let progress_for_result = progress_state.clone();
    let session_state_for_result = session_state.clone();
    let tab_id_for_result = tab_id.clone();
    let server_label_for_log = server_label.clone();

    // 启动 GPUI 任务：先延迟，再启动连接，再轮询状态
    cx.spawn(async move |async_cx| {
        // 阶段1: 300ms 初始动画延迟 - 让连接页面有时间展示"开始连接"动画
        debug!("[SSH] 开始初始连接动画（300ms）...");
        async_cx
            .background_executor()
            .timer(std::time::Duration::from_millis(300))
            .await;
        debug!("[SSH] 初始动画完成，开始发起后端连接...");

        // 阶段2: 初始动画完成后，启动实际的后端SSH连接
        let start_time = std::time::Instant::now();

        // 创建 UI 更新通道（使用 tokio unbounded channel 实现事件驱动）
        let (ui_sender, mut ui_receiver) = tokio::sync::mpsc::unbounded_channel::<UiUpdate>();

        // 启动 SSH 连接任务，获取连接句柄
        let connection_handle = crate::ssh::SshManager::global().connect(config, tab_id.clone());

        // 将 host_key_tx 包装成可共享的 Arc<Mutex>，以便在需要时使用
        let host_key_tx = Arc::new(Mutex::new(Some(connection_handle.host_key_tx)));

        // 在 SSH 运行时中启动事件处理任务
        let ui_sender_for_events = ui_sender.clone();
        crate::ssh::SshManager::global()
            .runtime()
            .spawn(async move {
                handle_connection_events(connection_handle.event_rx, ui_sender_for_events).await;
            });

        // 阶段3: 异步接收 UI 更新事件（事件驱动，无需轮询）
        while let Some(update) = ui_receiver.recv().await {
            let mut should_break = false;

            match update {
                UiUpdate::Stage(stage) => {
                    let _ = async_cx.update(|cx| {
                        progress_for_result.update(cx, |p, cx| {
                            p.set_stage(stage);
                            cx.notify();
                        });
                    });
                }
                UiUpdate::Log(log) => {
                    let _ = async_cx.update(|cx| {
                        progress_for_result.update(cx, |p, cx| {
                            p.add_log(log);
                            cx.notify();
                        });
                    });
                }
                UiUpdate::HostKeyVerification {
                    host,
                    port,
                    key_type,
                    fingerprint,
                } => {
                    info!(
                        "[SSH] Host key verification needed for {}:{} ({}): {}",
                        host, port, key_type, fingerprint
                    );

                    // 取出 host_key_tx 传给 UI，以便用户选择后发送响应
                    let tx = host_key_tx.lock().unwrap().take();

                    // 更新 UI 状态显示 host key 确认面板
                    let _ = async_cx.update(|cx| {
                        progress_for_result.update(cx, |p, cx| {
                            p.set_host_key_verification(
                                host.clone(),
                                port,
                                key_type.clone(),
                                fingerprint.clone(),
                                false, // 不是 mismatch
                            );
                            // 将发送端存入状态，以便按钮点击时使用
                            if let Some(tx) = tx {
                                p.set_host_key_tx(tx);
                            }
                            cx.notify();
                        });
                    });
                }
                UiUpdate::HostKeyMismatch {
                    host,
                    port,
                    expected_fingerprint,
                    actual_fingerprint,
                } => {
                    warn!(
                        "[SSH] WARNING: Host key mismatch for {}:{}! Expected: {}, Got: {}",
                        host, port, expected_fingerprint, actual_fingerprint
                    );

                    // 取出 host_key_tx 传给 UI
                    let tx = host_key_tx.lock().unwrap().take();

                    // 更新 UI 状态显示警告面板
                    let _ = async_cx.update(|cx| {
                        progress_for_result.update(cx, |p, cx| {
                            p.set_host_key_verification(
                                host.clone(),
                                port,
                                String::new(), // mismatch 时 key_type 不重要
                                actual_fingerprint.clone(),
                                true, // 是 mismatch
                            );
                            // 将发送端存入状态
                            if let Some(tx) = tx {
                                p.set_host_key_tx(tx);
                            }
                            cx.notify();
                        });
                    });
                }
                UiUpdate::Connected(session_id) => {
                    let duration = start_time.elapsed();
                    info!(
                        "[SSH] [{}] Connection successful! Session: {}",
                        server_label_for_log, session_id
                    );
                    info!(
                        "[SSH] Total connection time: {:.2}s",
                        duration.as_secs_f64()
                    );

                    // 更新服务器最后连接时间
                    if let Err(e) =
                        crate::services::storage::update_server_last_connected(&server_id)
                    {
                        error!("[SSH] Failed to update last connected time: {}", e);
                    }

                    // 阶段4: 300ms 成功动画延迟，让用户看到"连接成功"状态
                    debug!("[SSH] 开始连接成功动画（300ms）...");
                    async_cx
                        .background_executor()
                        .timer(std::time::Duration::from_millis(300))
                        .await;
                    debug!("[SSH] 成功动画完成，跳转到session...");

                    // 阶段5: 更新会话状态为已连接，触发跳转到session
                    let tab_id_clone = tab_id_for_result.clone();
                    let _ = async_cx.update(|cx| {
                        session_state_for_result.update(cx, |state, cx| {
                            state.update_tab_status(&tab_id_clone, SessionStatus::Connected);
                            cx.notify();
                        });
                    });

                    should_break = true;
                }
                UiUpdate::Failed(error) => {
                    error!(
                        "[SSH] [{}] Connection failed: {}",
                        server_label_for_log, error
                    );

                    let _ = async_cx.update(|cx| {
                        progress_for_result.update(cx, |p, cx| {
                            p.set_error(error);
                            cx.notify();
                        });
                    });

                    should_break = true;
                }
                UiUpdate::Disconnected(reason) => {
                    info!("[SSH] [{}] Disconnected: {}", server_label_for_log, reason);
                    should_break = true;
                }
            }

            if should_break {
                break;
            }
        }
    })
    .detach();
}

/// 处理连接事件，转发到 UI 更新通道
async fn handle_connection_events(
    mut receiver: mpsc::UnboundedReceiver<ConnectionEvent>,
    ui_sender: mpsc::UnboundedSender<UiUpdate>,
) {
    while let Some(event) = receiver.recv().await {
        match event {
            ConnectionEvent::StageChanged(stage) => {
                let _ = ui_sender.send(UiUpdate::Stage(stage));
            }
            ConnectionEvent::Log(entry) => {
                let _ = ui_sender.send(UiUpdate::Log(entry));
            }
            ConnectionEvent::Connected { session_id } => {
                debug!("[SSH Event] Connected! Session ID: {}", session_id);
                let _ = ui_sender.send(UiUpdate::Connected(session_id));
            }
            ConnectionEvent::Failed { error } => {
                debug!("[SSH Event] Failed: {}", error);
                let _ = ui_sender.send(UiUpdate::Failed(error));
            }
            ConnectionEvent::Disconnected { reason } => {
                debug!("[SSH Event] Disconnected: {}", reason);
                let _ = ui_sender.send(UiUpdate::Disconnected(reason));
            }
            ConnectionEvent::HostKeyVerification {
                host,
                port,
                key_type,
                fingerprint,
            } => {
                debug!(
                    "[SSH Event] Host key verification: {}:{} ({}) {}",
                    host, port, key_type, fingerprint
                );

                // 通知 UI 显示确认面板，用户会在 UI 中选择操作
                // host_key_tx 会在 UI 轮询循环中传给 ConnectingProgress
                let _ = ui_sender.send(UiUpdate::HostKeyVerification {
                    host,
                    port,
                    key_type,
                    fingerprint,
                });
            }
            ConnectionEvent::HostKeyMismatch {
                host,
                port,
                expected_fingerprint,
                actual_fingerprint,
            } => {
                debug!(
                    "[SSH Event] Host key MISMATCH: {}:{} expected {} got {}",
                    host, port, expected_fingerprint, actual_fingerprint
                );

                // 通知 UI 显示警告面板，用户会在 UI 中选择操作
                let _ = ui_sender.send(UiUpdate::HostKeyMismatch {
                    host,
                    port,
                    expected_fingerprint,
                    actual_fingerprint,
                });
            }
        }
    }
}
