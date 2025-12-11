// SSH 客户端 Handler 实现
// 实现 russh::client::Handler trait

use russh::keys::PublicKey;
use std::future::Future;
use tokio::sync::mpsc;

use super::event::{ConnectionEvent, LogEntry};

/// SSH 客户端 Handler
/// 处理 SSH 连接过程中的各种回调
pub struct SshClientHandler {
    /// 事件发送器（用于通知 UI）
    event_sender: mpsc::UnboundedSender<ConnectionEvent>,
    /// 服务器主机名（用于日志）
    #[allow(dead_code)]
    host: String,
}

impl SshClientHandler {
    /// 创建新的 Handler
    pub fn new(event_sender: mpsc::UnboundedSender<ConnectionEvent>, host: String) -> Self {
        Self { event_sender, host }
    }

    /// 发送日志事件
    fn log(&self, entry: LogEntry) {
        let _ = self.event_sender.send(ConnectionEvent::Log(entry));
    }
}

impl russh::client::Handler for SshClientHandler {
    type Error = russh::Error;

    /// 检查服务器公钥
    /// 这里简单接受所有公钥，生产环境应该实现 known_hosts 检查
    fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        // 获取密钥指纹
        let fingerprint = server_public_key.fingerprint(russh::keys::ssh_key::HashAlg::Sha256);

        self.log(LogEntry::info(format!(
            "Server key fingerprint: {}",
            fingerprint
        )));

        self.log(LogEntry::debug(format!(
            "Server key type: {}",
            server_public_key.algorithm()
        )));

        // TODO: 实现 known_hosts 检查
        // 目前简单接受所有公钥
        async { Ok(true) }
    }
}
