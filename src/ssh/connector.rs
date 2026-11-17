// SSH 连接启动器
// 负责从 UI 层接收连接请求，并启动异步连接任务

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::{App, Entity};
use tracing::{debug, error, info, warn};

use crate::models::ServerData;
use crate::pages::connecting::page::{ConnectionDetails, HostKeyVerificationState};
use crate::pages::connecting::ConnectingProgress;
use crate::services::storage;
use crate::state::{SessionState, SessionStatus};

use super::config::{HostKeyPolicy, SshConfig};
use super::event::ConnectionEvent;

/// 连接页面上展示的连接方式
fn connection_details(config: &SshConfig) -> ConnectionDetails {
    let jump_chain = config.jump_chain();
    ConnectionDetails {
        host: config.host.clone(),
        port: config.port,
        proxy_desc: config.first_hop_proxy().map(|proxy| {
            format!(
                "{} Proxy ({}:{})",
                proxy.proxy_type.label(),
                proxy.host,
                proxy.port
            )
        }),
        jump_host_desc: (!jump_chain.is_empty()).then(|| {
            jump_chain
                .iter()
                .map(|hop| format!("{} ({}:{})", hop.display_name(), hop.host, hop.port))
                .collect::<Vec<_>>()
                .join(" → ")
        }),
    }
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
    let settings = storage::load_settings().unwrap_or_default();
    let servers = storage::load_servers()
        .map(|config| config.servers)
        .unwrap_or_default();
    let policy = HostKeyPolicy {
        interactive: true,
        strict: settings.connection.strict_host_key_checking,
        trusted: HashMap::new(),
    };

    let config = match SshConfig::from_server(&server, &settings.connection, &servers, policy) {
        Ok(config) => config,
        Err(e) => {
            error!("[SSH] [{}] Invalid connection settings: {}", server.label, e);
            progress_state.update(cx, |p, cx| {
                p.set_error(e.to_string());
                cx.notify();
            });
            return;
        }
    };

    progress_state.update(cx, |p, _| {
        p.set_connection_details(connection_details(&config));
    });

    let server_label = server.label.clone();
    let server_id = server.id.clone();

    // 启动 GPUI 任务：先延迟，再启动连接，再按事件更新界面
    cx.spawn(async move |async_cx| {
        // 阶段1: 300ms 初始动画延迟 - 让连接页面有时间展示"开始连接"动画
        debug!("[SSH] 开始初始连接动画（300ms）...");
        async_cx
            .background_executor()
            .timer(Duration::from_millis(300))
            .await;
        debug!("[SSH] 初始动画完成，开始发起后端连接...");

        // 动画期间用户可能已取消/关闭标签页：这时 close_session 还没有可取消的连接，
        // 必须在这里放弃，否则会为已关闭的标签建立一条没人管理的连接。
        // （检查与下面的 connect 之间没有 await，不会与关闭标签交错）
        let still_connecting = async_cx
            .update(|cx| {
                session_state
                    .read(cx)
                    .tabs
                    .iter()
                    .any(|tab| tab.id == tab_id && tab.status == SessionStatus::Connecting)
            })
            .unwrap_or(false);
        if !still_connecting {
            debug!("[SSH] Tab {} was closed before connecting; skip", tab_id);
            return;
        }

        // 阶段2: 启动实际的后端 SSH 连接
        let start_time = Instant::now();
        let mut event_rx = crate::ssh::SshManager::global()
            .connect(config, tab_id.clone())
            .event_rx;

        // 阶段3: 按事件更新界面（连接任务被取消时通道关闭，循环随之结束）
        while let Some(event) = event_rx.recv().await {
            match event {
                ConnectionEvent::StageChanged(stage) => {
                    let _ = async_cx.update(|cx| {
                        progress_state.update(cx, |p, cx| {
                            p.set_stage(stage);
                            cx.notify();
                        });
                    });
                }
                ConnectionEvent::Log(log) => {
                    let _ = async_cx.update(|cx| {
                        progress_state.update(cx, |p, cx| {
                            p.add_log(log);
                            cx.notify();
                        });
                    });
                }
                ConnectionEvent::HostKeyVerification {
                    host,
                    port,
                    key_type,
                    fingerprint,
                    responder,
                } => {
                    info!(
                        "[SSH] Host key verification needed for {}:{} ({}): {}",
                        host, port, key_type, fingerprint
                    );
                    let _ = async_cx.update(|cx| {
                        progress_state.update(cx, |p, cx| {
                            p.set_host_key_verification(HostKeyVerificationState {
                                host,
                                port,
                                key_type,
                                fingerprint,
                                expected_fingerprint: None,
                                is_mismatch: false,
                                responder,
                            });
                            cx.notify();
                        });
                    });
                }
                ConnectionEvent::HostKeyMismatch {
                    host,
                    port,
                    key_type,
                    expected_fingerprint,
                    actual_fingerprint,
                    responder,
                } => {
                    warn!(
                        "[SSH] WARNING: Host key mismatch for {}:{}! Expected: {}, Got: {}",
                        host, port, expected_fingerprint, actual_fingerprint
                    );
                    let _ = async_cx.update(|cx| {
                        progress_state.update(cx, |p, cx| {
                            p.set_host_key_verification(HostKeyVerificationState {
                                host,
                                port,
                                key_type,
                                fingerprint: actual_fingerprint,
                                expected_fingerprint: Some(expected_fingerprint),
                                is_mismatch: true,
                                responder,
                            });
                            cx.notify();
                        });
                    });
                }
                ConnectionEvent::Connected { session_id } => {
                    info!(
                        "[SSH] [{}] Connection successful! Session: {}",
                        server_label, session_id
                    );
                    info!(
                        "[SSH] Total connection time: {:.2}s",
                        start_time.elapsed().as_secs_f64()
                    );

                    // 更新服务器最后连接时间
                    if let Err(e) = storage::update_server_last_connected(&server_id) {
                        error!("[SSH] Failed to update last connected time: {}", e);
                    }

                    // 阶段4: 300ms 成功动画延迟，让用户看到"连接成功"状态
                    async_cx
                        .background_executor()
                        .timer(Duration::from_millis(300))
                        .await;

                    // 阶段5: 更新会话状态为已连接，触发跳转到session
                    let tab_id = tab_id.clone();
                    let server = server.clone();
                    let _ = async_cx.update(|cx| {
                        session_state.update(cx, |state, cx| {
                            state.update_tab_status(&tab_id, SessionStatus::Connected);

                            // 存储 server_data 用于重连
                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                tab.server_data = Some(server);
                            }

                            // Monitor 和 SFTP 服务将在终端 PTY 创建成功后启动
                            // 这样可以保证 PTY 通道是第一个创建的，能收到服务器欢迎信息

                            cx.notify();
                        });
                    });
                    break;
                }
                ConnectionEvent::Failed { error, .. } => {
                    error!("[SSH] [{}] Connection failed: {}", server_label, error);
                    let _ = async_cx.update(|cx| {
                        progress_state.update(cx, |p, cx| {
                            p.clear_host_key_verification();
                            p.set_error(error);
                            cx.notify();
                        });
                    });
                    break;
                }
            }
        }
    })
    .detach();
}
