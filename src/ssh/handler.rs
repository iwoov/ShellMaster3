// SSH 客户端 Handler 实现
// 实现 russh::client::Handler trait

use russh::keys::PublicKey;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

use super::event::{ConnectionEvent, HostKeyAction, LogEntry};

/// SSH 客户端 Handler
/// 处理 SSH 连接过程中的各种回调
pub struct SshClientHandler {
    /// 事件发送器（用于通知 UI）
    event_sender: mpsc::UnboundedSender<ConnectionEvent>,
    /// 服务器主机名
    host: String,
    /// 服务器端口
    port: u16,
    /// Host key 响应接收器（用于等待用户确认）
    host_key_response_rx: Arc<Mutex<Option<oneshot::Receiver<HostKeyAction>>>>,
}

impl SshClientHandler {
    /// 创建新的 Handler
    pub fn new(
        event_sender: mpsc::UnboundedSender<ConnectionEvent>,
        host: String,
        port: u16,
        host_key_response_rx: oneshot::Receiver<HostKeyAction>,
    ) -> Self {
        Self {
            event_sender,
            host,
            port,
            host_key_response_rx: Arc::new(Mutex::new(Some(host_key_response_rx))),
        }
    }

    /// 发送日志事件
    fn log(&self, entry: LogEntry) {
        let _ = self.event_sender.send(ConnectionEvent::Log(entry));
    }
}

impl russh::client::Handler for SshClientHandler {
    type Error = russh::Error;

    /// 检查服务器公钥
    /// 实现 known_hosts 检查逻辑
    fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        // 获取密钥指纹（转换为字符串以便比较和存储）
        let fingerprint = server_public_key
            .fingerprint(russh::keys::ssh_key::HashAlg::Sha256)
            .to_string();
        let key_type = server_public_key.algorithm().to_string();

        self.log(LogEntry::info(format!(
            "Server key fingerprint: {}",
            fingerprint
        )));

        self.log(LogEntry::debug(format!("Server key type: {}", key_type)));

        let host = self.host.clone();
        let port = self.port;
        let event_sender = self.event_sender.clone();
        let response_rx = self.host_key_response_rx.clone();

        async move {
            // 检查 known hosts
            match crate::services::storage::find_known_host(&host, port) {
                Ok(Some(known)) => {
                    // 主机已知
                    if known.fingerprint == fingerprint {
                        // 指纹匹配，更新最后使用时间
                        let _ = crate::services::storage::update_known_host_last_used(&host, port);
                        println!("[SSH] Host {} verified (known host)", host);
                        Ok(true)
                    } else {
                        // 指纹不匹配！可能存在安全风险
                        println!(
                            "[SSH] WARNING: Host key mismatch for {}! Expected: {}, Got: {}",
                            host, known.fingerprint, fingerprint
                        );

                        // 发送警告事件给 UI
                        let _ = event_sender.send(ConnectionEvent::HostKeyMismatch {
                            host: host.clone(),
                            port,
                            expected_fingerprint: known.fingerprint.clone(),
                            actual_fingerprint: fingerprint.to_string(),
                        });

                        // 等待用户响应
                        if let Some(rx) = response_rx.lock().await.take() {
                            match rx.await {
                                Ok(HostKeyAction::AcceptAndSave) => {
                                    // 更新 known host
                                    let _ = crate::services::storage::add_known_host(
                                        &host,
                                        port,
                                        &key_type,
                                        &fingerprint,
                                    );
                                    println!("[SSH] User accepted and saved new key for {}", host);
                                    Ok(true)
                                }
                                Ok(HostKeyAction::AcceptOnce) => {
                                    println!("[SSH] User accepted key once for {}", host);
                                    Ok(true)
                                }
                                Ok(HostKeyAction::Reject) | Err(_) => {
                                    println!("[SSH] User rejected connection to {}", host);
                                    Ok(false)
                                }
                            }
                        } else {
                            // 没有响应通道，拒绝连接
                            Ok(false)
                        }
                    }
                }
                Ok(None) => {
                    // 未知主机，需要用户确认
                    println!("[SSH] Unknown host: {}:{}", host, port);

                    // 发送验证请求给 UI
                    let _ = event_sender.send(ConnectionEvent::HostKeyVerification {
                        host: host.clone(),
                        port,
                        key_type: key_type.clone(),
                        fingerprint: fingerprint.to_string(),
                    });

                    // 等待用户响应
                    if let Some(rx) = response_rx.lock().await.take() {
                        match rx.await {
                            Ok(HostKeyAction::AcceptAndSave) => {
                                // 保存到 known hosts
                                let _ = crate::services::storage::add_known_host(
                                    &host,
                                    port,
                                    &key_type,
                                    &fingerprint,
                                );
                                println!("[SSH] User accepted and saved key for {}", host);
                                Ok(true)
                            }
                            Ok(HostKeyAction::AcceptOnce) => {
                                println!("[SSH] User accepted key once for {}", host);
                                Ok(true)
                            }
                            Ok(HostKeyAction::Reject) | Err(_) => {
                                println!("[SSH] User rejected connection to {}", host);
                                Ok(false)
                            }
                        }
                    } else {
                        // 没有响应通道，拒绝连接
                        Ok(false)
                    }
                }
                Err(e) => {
                    // 读取 known hosts 失败，记录错误但仍尝试验证
                    eprintln!("[SSH] Error reading known hosts: {}", e);
                    // 发送验证请求
                    let _ = event_sender.send(ConnectionEvent::HostKeyVerification {
                        host: host.clone(),
                        port,
                        key_type: key_type.clone(),
                        fingerprint: fingerprint.to_string(),
                    });

                    // 等待用户响应
                    if let Some(rx) = response_rx.lock().await.take() {
                        match rx.await {
                            Ok(HostKeyAction::AcceptAndSave) => {
                                let _ = crate::services::storage::add_known_host(
                                    &host,
                                    port,
                                    &key_type,
                                    &fingerprint,
                                );
                                Ok(true)
                            }
                            Ok(HostKeyAction::AcceptOnce) => Ok(true),
                            Ok(HostKeyAction::Reject) | Err(_) => Ok(false),
                        }
                    } else {
                        Ok(false)
                    }
                }
            }
        }
    }
}
