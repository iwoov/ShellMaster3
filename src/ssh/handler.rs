// SSH 客户端 Handler 实现
// 实现 russh::client::Handler trait，负责主机密钥校验

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use russh::keys::ssh_key::HashAlg;
use russh::keys::PublicKey;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::services::storage;

use super::config::{host_key_id, HostKeyPolicy};
use super::event::{ConnectionEvent, HostKeyAction, HostKeyResponder, LogEntry};

/// SSH 客户端 Handler（每一跳一个）
pub struct SshClientHandler {
    /// 事件发送器（用于通知 UI）
    pub(super) event_sender: mpsc::UnboundedSender<ConnectionEvent>,
    /// 服务器主机名
    pub(super) host: String,
    /// 服务器端口
    pub(super) port: u16,
    /// 主机密钥校验策略
    pub(super) policy: HostKeyPolicy,
    /// 本次连接中已通过校验的指纹（各跳共享），连接成功后存入会话供重连使用
    pub(super) verified: Arc<Mutex<HashMap<String, String>>>,
    /// 拒绝原因：握手因主机密钥失败时，用它生成可读的错误信息
    pub(super) rejection: Arc<Mutex<Option<String>>>,
    /// 是否正在等待用户确认（等待期间握手超时暂停计时）
    pub(super) awaiting_user: watch::Sender<bool>,
    /// 连接任务被取消时结束等待
    pub(super) cancel: CancellationToken,
}

impl SshClientHandler {
    fn log(&self, entry: LogEntry) {
        let _ = self.event_sender.send(ConnectionEvent::Log(entry));
    }

    /// 校验主机密钥，返回是否接受
    async fn verify(&mut self, key_type: &str, fingerprint: &str) -> bool {
        let id = host_key_id(&self.host, self.port);
        let known = match storage::find_known_host(&self.host, self.port) {
            Ok(known) => known.map(|known| known.fingerprint),
            Err(e) => {
                warn!("[SSH] Failed to read known hosts: {}", e);
                self.log(LogEntry::warn(format!("Failed to read known hosts: {}", e)));
                None
            }
        };
        if let Some(expected) = known.as_deref().filter(|expected| *expected != fingerprint) {
            warn!(
                "[SSH] WARNING: Host key mismatch for {}! Expected: {}, Got: {}",
                id, expected, fingerprint
            );
        }

        match decide_host_key(&id, known.as_deref(), fingerprint, &self.policy) {
            HostKeyDecision::Accept => {
                if known.as_deref() == Some(fingerprint) {
                    let _ = storage::update_known_host_last_used(&self.host, self.port);
                    info!("[SSH] Host {} verified (known host)", id);
                } else {
                    info!("[SSH] Host {} matches the key trusted earlier in this session", id);
                }
                true
            }
            HostKeyDecision::Reject(reason) => self.reject(reason),
            HostKeyDecision::AskUser { expected } => {
                let (responder, rx) = HostKeyResponder::new();
                let event = match expected {
                    Some(expected_fingerprint) => ConnectionEvent::HostKeyMismatch {
                        host: self.host.clone(),
                        port: self.port,
                        key_type: key_type.to_string(),
                        expected_fingerprint,
                        actual_fingerprint: fingerprint.to_string(),
                        responder,
                    },
                    None => {
                        info!("[SSH] Unknown host: {}", id);
                        ConnectionEvent::HostKeyVerification {
                            host: self.host.clone(),
                            port: self.port,
                            key_type: key_type.to_string(),
                            fingerprint: fingerprint.to_string(),
                            responder,
                        }
                    }
                };
                let _ = self.event_sender.send(event);
                self.await_user_choice(rx, key_type, fingerprint).await
            }
        }
    }

    /// 等待用户在界面上作出选择
    async fn await_user_choice(
        &mut self,
        rx: oneshot::Receiver<HostKeyAction>,
        key_type: &str,
        fingerprint: &str,
    ) -> bool {
        // 等待用户期间暂停握手超时计时，避免用户核对指纹时连接被判定超时
        self.awaiting_user.send_replace(true);
        let action = tokio::select! {
            // 应答通道被丢弃（界面已关闭）时按拒绝处理
            result = rx => result.unwrap_or(HostKeyAction::Reject),
            _ = self.cancel.cancelled() => HostKeyAction::Reject,
        };
        self.awaiting_user.send_replace(false);

        match action {
            HostKeyAction::AcceptAndSave => {
                if let Err(e) = storage::add_known_host(&self.host, self.port, key_type, fingerprint)
                {
                    warn!("[SSH] Failed to save known host: {}", e);
                    self.log(LogEntry::warn(format!("Failed to save known host: {}", e)));
                }
                info!("[SSH] User accepted and saved key for {}", self.host);
                true
            }
            HostKeyAction::AcceptOnce => {
                info!("[SSH] User accepted key once for {}", self.host);
                true
            }
            HostKeyAction::Reject => self.reject("the host key was rejected".to_string()),
        }
    }

    fn reject(&self, reason: String) -> bool {
        warn!("[SSH] Host key rejected: {}", reason);
        self.log(LogEntry::error(format!("Host key rejected: {}", reason)));
        if let Ok(mut slot) = self.rejection.lock() {
            *slot = Some(reason);
        }
        false
    }
}

/// 主机密钥校验结论
#[derive(Debug, PartialEq, Eq)]
enum HostKeyDecision {
    Accept,
    Reject(String),
    /// 需要用户确认；`expected` 为 known hosts 中保存的原指纹（密钥变化时）
    AskUser { expected: Option<String> },
}

/// 根据 known hosts 记录、本标签页已信任的指纹和策略决定如何处理服务器密钥
fn decide_host_key(
    id: &str,
    known: Option<&str>,
    fingerprint: &str,
    policy: &HostKeyPolicy,
) -> HostKeyDecision {
    // 用户在本标签页之前确认过同一把密钥（选了“仅本次信任”）
    let trusted_earlier = policy
        .trusted
        .get(id)
        .is_some_and(|trusted| trusted == fingerprint);

    match known {
        Some(known) if known == fingerprint => HostKeyDecision::Accept,
        Some(known) => {
            if policy.strict {
                return HostKeyDecision::Reject(format!(
                    "the host key of {} has changed (saved: {}, received: {}) and strict host key checking is enabled. \
                     If the server key was replaced on purpose, remove the old entry from Known Hosts and connect again",
                    id, known, fingerprint
                ));
            }
            if trusted_earlier {
                return HostKeyDecision::Accept;
            }
            if !policy.interactive {
                return HostKeyDecision::Reject(format!(
                    "the host key of {} has changed (saved: {}, received: {})",
                    id, known, fingerprint
                ));
            }
            HostKeyDecision::AskUser {
                expected: Some(known.to_string()),
            }
        }
        None => {
            if trusted_earlier {
                return HostKeyDecision::Accept;
            }
            if !policy.interactive {
                return HostKeyDecision::Reject(format!(
                    "the host key of {} ({}) was never confirmed for this session",
                    id, fingerprint
                ));
            }
            HostKeyDecision::AskUser { expected: None }
        }
    }
}

impl russh::client::Handler for SshClientHandler {
    type Error = russh::Error;

    /// 检查服务器公钥（known hosts 校验）
    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let fingerprint = server_public_key.fingerprint(HashAlg::Sha256).to_string();
        let key_type = server_public_key.algorithm().to_string();

        self.log(LogEntry::info(format!(
            "Server key fingerprint for {}:{}: {}",
            self.host, self.port, fingerprint
        )));
        self.log(LogEntry::debug(format!("Server key type: {}", key_type)));

        let accepted = self.verify(&key_type, &fingerprint).await;
        if accepted {
            if let Ok(mut verified) = self.verified.lock() {
                verified.insert(host_key_id(&self.host, self.port), fingerprint);
            }
        }
        Ok(accepted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "example.com:22";
    const OLD: &str = "SHA256:old";
    const NEW: &str = "SHA256:new";

    fn policy(interactive: bool, strict: bool, trusted: Option<&str>) -> HostKeyPolicy {
        HostKeyPolicy {
            interactive,
            strict,
            trusted: trusted
                .map(|fingerprint| [(ID.to_string(), fingerprint.to_string())].into())
                .unwrap_or_default(),
        }
    }

    #[test]
    fn known_matching_key_is_accepted() {
        let decision = decide_host_key(ID, Some(NEW), NEW, &policy(false, true, None));
        assert_eq!(decision, HostKeyDecision::Accept);
    }

    #[test]
    fn unknown_key_prompts_only_when_interactive() {
        assert_eq!(
            decide_host_key(ID, None, NEW, &policy(true, false, None)),
            HostKeyDecision::AskUser { expected: None }
        );
        assert!(matches!(
            decide_host_key(ID, None, NEW, &policy(false, false, None)),
            HostKeyDecision::Reject(_)
        ));
        // 重连时：同一把之前确认过的密钥直接接受，换了密钥则拒绝
        assert_eq!(
            decide_host_key(ID, None, NEW, &policy(false, false, Some(NEW))),
            HostKeyDecision::Accept
        );
        assert!(matches!(
            decide_host_key(ID, None, NEW, &policy(false, false, Some(OLD))),
            HostKeyDecision::Reject(_)
        ));
    }

    #[test]
    fn changed_key_accepted_once_is_honoured_on_reconnect() {
        // known hosts 里是旧密钥，用户在本标签页对新密钥选了“仅本次信任”
        assert_eq!(
            decide_host_key(ID, Some(OLD), NEW, &policy(false, false, Some(NEW))),
            HostKeyDecision::Accept
        );
        assert!(matches!(
            decide_host_key(ID, Some(OLD), NEW, &policy(false, false, None)),
            HostKeyDecision::Reject(_)
        ));
        assert_eq!(
            decide_host_key(ID, Some(OLD), NEW, &policy(true, false, None)),
            HostKeyDecision::AskUser {
                expected: Some(OLD.to_string())
            }
        );
    }

    #[test]
    fn strict_mode_rejects_changed_key_even_if_trusted_before() {
        let decision = decide_host_key(ID, Some(OLD), NEW, &policy(true, true, Some(NEW)));
        assert!(matches!(decision, HostKeyDecision::Reject(reason) if reason.contains("strict")));
    }
}
