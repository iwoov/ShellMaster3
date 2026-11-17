// SSH 客户端核心实现

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{AuthResult, Handle, KeyboardInteractiveAuthResponse, Prompt};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg};
use russh::{MethodKind, MethodSet};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, watch};
use tokio::time::{sleep_until, timeout, Instant};
use tokio_util::sync::CancellationToken;

use super::config::{AuthMethod, SshConfig};
use super::error::SshError;
use super::event::{ConnectionEvent, ConnectionStage, LogEntry};
use super::handler::SshClientHandler;
use super::net::connect_tcp;
use super::proxy::connect_via_proxy;
use super::session::SshSession;

/// keyboard-interactive 最多交互轮数（防止异常服务器无限要求输入）
const MAX_KEYBOARD_INTERACTIVE_ROUNDS: usize = 8;

/// 可承载 SSH 的字节流：TCP 连接、代理隧道或跳板机转发通道
trait Transport: AsyncRead + AsyncWrite + Unpin + Send + 'static {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> Transport for T {}

type ClientHandle = Handle<SshClientHandler>;
type EstablishFuture<'a> = Pin<Box<dyn Future<Output = Result<ClientHandle, SshError>> + Send + 'a>>;

/// SSH 客户端
/// 负责建立 SSH 连接（含跳板机链）并返回 SshSession
pub struct SshClient {
    /// 连接配置
    config: SshConfig,
    /// 事件发送器
    event_sender: mpsc::UnboundedSender<ConnectionEvent>,
    /// 连接任务被取消时通知各跳停止等待用户确认
    cancel: CancellationToken,
    /// 本次连接中各跳已通过校验的主机密钥
    verified_host_keys: Arc<Mutex<HashMap<String, String>>>,
}

impl SshClient {
    /// 创建新的 SSH 客户端
    pub fn new(config: SshConfig, event_sender: mpsc::UnboundedSender<ConnectionEvent>) -> Self {
        Self {
            config,
            event_sender,
            cancel: CancellationToken::new(),
            verified_host_keys: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 发送阶段变化事件
    fn emit_stage(&self, stage: ConnectionStage) {
        let _ = self.event_sender.send(ConnectionEvent::StageChanged(stage));
    }

    /// 发送日志事件
    fn log(&self, entry: LogEntry) {
        let _ = self.event_sender.send(ConnectionEvent::Log(entry));
    }

    /// 发送某一跳的日志（跳板机的日志带上跳板机名称）
    fn log_hop(&self, hop: &SshConfig, is_target: bool, message: impl Into<String>) {
        let message = message.into();
        if is_target {
            self.log(LogEntry::info(message));
        } else {
            self.log(LogEntry::info(format!("[{}] {}", hop.display_name(), message)));
        }
    }

    /// 执行连接（异步），返回 SshSession 用于后续操作
    ///
    /// 只负责建立连接；会话注册和 `Connected` 事件由 SshManager 完成，
    /// 保证 UI 收到连接成功时会话已经可用。
    pub async fn connect(&self, session_id: String) -> Result<SshSession, SshError> {
        // 连接任务被中止（例如用户关闭了标签页）时，结束仍在等待中的主机密钥确认
        let _cancel_on_drop = self.cancel.clone().drop_guard();

        self.emit_stage(ConnectionStage::Initializing);
        self.log(LogEntry::info("Starting SSH connection..."));
        self.log(LogEntry::debug(format!(
            "Target: {}@{}:{}",
            self.config.username, self.config.host, self.config.port
        )));

        let config = &self.config;
        let mut jump_handles = Vec::new();
        let handle = self.establish(config, true, &mut jump_handles).await?;

        self.emit_stage(ConnectionStage::EstablishingChannel);
        self.log(LogEntry::info("Establishing secure channel..."));
        self.emit_stage(ConnectionStage::StartingSession);
        self.log(LogEntry::info("Starting session..."));
        self.emit_stage(ConnectionStage::Connected);
        self.log(LogEntry::info("SSH connection established successfully!"));

        let verified_host_keys = self
            .verified_host_keys
            .lock()
            .map(|keys| keys.clone())
            .unwrap_or_default();

        Ok(SshSession::new(
            session_id,
            Arc::new(handle),
            jump_handles,
            verified_host_keys,
            config.host.clone(),
            config.username.clone(),
        ))
    }

    /// 建立到 `hop` 的已认证 SSH 连接。
    ///
    /// 配置了跳板机时，先递归连接跳板机，再通过跳板机的 direct-tcpip 通道转发到本主机；
    /// 跳板机连接按由外到内的顺序存入 `jump_handles`，需与目标会话同生命周期。
    fn establish<'a>(
        &'a self,
        hop: &'a SshConfig,
        is_target: bool,
        jump_handles: &'a mut Vec<Arc<ClientHandle>>,
    ) -> EstablishFuture<'a> {
        Box::pin(async move {
            let limit = Duration::from_secs(hop.connect_timeout);

            let transport: Box<dyn Transport> = if let Some(jump) = hop.jump_host.as_deref() {
                let jump_handle = self
                    .establish(jump, false, jump_handles)
                    .await
                    .map_err(|e| hop_error(jump, e))?;

                if is_target {
                    self.emit_stage(ConnectionStage::ConnectingHost);
                }
                self.log_hop(
                    hop,
                    is_target,
                    format!(
                        "Opening tunnel to {}:{} via jump host '{}'...",
                        hop.host,
                        hop.port,
                        jump.display_name()
                    ),
                );
                let channel = timeout(
                    limit,
                    jump_handle.channel_open_direct_tcpip(
                        hop.host.clone(),
                        u32::from(hop.port),
                        "127.0.0.1",
                        0,
                    ),
                )
                .await
                .map_err(|_| SshError::Timeout(hop.connect_timeout))?
                .map_err(|e| {
                    SshError::JumpHost(format!(
                        "'{}' could not open a tunnel to {}:{}: {}",
                        jump.display_name(),
                        hop.host,
                        hop.port,
                        e
                    ))
                })?;
                jump_handles.push(Arc::new(jump_handle));
                self.log_hop(hop, is_target, "Tunnel established");
                Box::new(channel.into_stream())
            } else if let Some(proxy) = &hop.proxy {
                self.emit_stage(ConnectionStage::ConnectingProxy);
                self.log_hop(
                    hop,
                    is_target,
                    format!(
                        "Connecting to {}:{} via {} proxy {}:{}...",
                        hop.host,
                        hop.port,
                        proxy.proxy_type.label(),
                        proxy.host,
                        proxy.port
                    ),
                );
                let stream = connect_via_proxy(proxy, &hop.host, hop.port, limit).await?;
                if is_target {
                    self.emit_stage(ConnectionStage::ConnectingHost);
                }
                self.log_hop(hop, is_target, "Proxy tunnel established");
                Box::new(stream)
            } else {
                if is_target {
                    self.emit_stage(ConnectionStage::ConnectingHost);
                }
                self.log_hop(
                    hop,
                    is_target,
                    format!("Connecting to {}:{}...", hop.host, hop.port),
                );
                let stream = connect_tcp(&hop.host, hop.port, limit).await?;
                match stream.peer_addr() {
                    Ok(addr) => self.log_hop(
                        hop,
                        is_target,
                        format!("TCP connection established ({})", addr),
                    ),
                    Err(_) => self.log_hop(hop, is_target, "TCP connection established"),
                }
                Box::new(stream)
            };

            // SSH 握手
            if is_target {
                self.emit_stage(ConnectionStage::Handshaking);
            } else {
                self.emit_stage(ConnectionStage::ConnectingJumpHost);
            }
            self.log_hop(hop, is_target, "Starting SSH handshake...");
            let mut handle = self.handshake(hop, transport).await?;
            self.log_hop(hop, is_target, "SSH handshake completed");

            // 认证
            if is_target {
                self.emit_stage(ConnectionStage::Authenticating);
            }
            self.log_hop(
                hop,
                is_target,
                format!("Authenticating as '{}'...", hop.username),
            );
            timeout(limit, self.authenticate(hop, &mut handle))
                .await
                .map_err(|_| SshError::Timeout(hop.connect_timeout))??;
            self.log_hop(hop, is_target, "Authentication successful");

            Ok(handle)
        })
    }

    /// SSH 握手（版本交换 + 密钥交换 + 主机密钥校验）
    async fn handshake(
        &self,
        hop: &SshConfig,
        transport: Box<dyn Transport>,
    ) -> Result<ClientHandle, SshError> {
        let (awaiting_user_tx, awaiting_user_rx) = watch::channel(false);
        let rejection = Arc::new(Mutex::new(None));
        let handler = SshClientHandler {
            event_sender: self.event_sender.clone(),
            host: hop.host.clone(),
            port: hop.port,
            policy: hop.host_key_policy.clone(),
            verified: self.verified_host_keys.clone(),
            rejection: rejection.clone(),
            awaiting_user: awaiting_user_tx,
            cancel: self.cancel.clone(),
        };

        let connect = russh::client::connect_stream(Arc::new(hop.to_russh_config()), transport, handler);
        let limit = Duration::from_secs(hop.connect_timeout);
        match with_handshake_timeout(connect, limit, awaiting_user_rx).await {
            None => Err(SshError::Timeout(hop.connect_timeout)),
            Some(Ok(handle)) => Ok(handle),
            Some(Err(e)) => {
                let reason = rejection.lock().ok().and_then(|mut reason| reason.take());
                Err(match reason {
                    Some(reason) => SshError::HostKey(reason),
                    None => SshError::from(e),
                })
            }
        }
    }

    /// 执行认证
    async fn authenticate(&self, hop: &SshConfig, handle: &mut ClientHandle) -> Result<(), SshError> {
        match &hop.auth {
            AuthMethod::Password(password) => self.authenticate_password(hop, handle, password).await,
            AuthMethod::PublicKey {
                key_path,
                passphrase,
            } => {
                self.authenticate_publickey(hop, handle, key_path, passphrase.as_deref())
                    .await
            }
        }
    }

    /// 密码认证；服务器不接受 password 方式但开放了 keyboard-interactive 时，自动改用后者
    async fn authenticate_password(
        &self,
        hop: &SshConfig,
        handle: &mut ClientHandle,
        password: &str,
    ) -> Result<(), SshError> {
        self.log(LogEntry::debug("Using password authentication"));

        let remaining_methods = match handle
            .authenticate_password(hop.username.clone(), password)
            .await?
        {
            AuthResult::Success => return Ok(()),
            AuthResult::Failure {
                remaining_methods,
                partial_success,
            } => {
                if partial_success {
                    return Err(partial_success_error(&remaining_methods));
                }
                remaining_methods
            }
        };

        // FreeBSD 默认配置、很多启用 PAM 的服务器和网络设备只通过 keyboard-interactive 接收密码
        if remaining_methods.contains(&MethodKind::KeyboardInteractive) {
            self.log(LogEntry::info(
                "Password method not accepted, trying keyboard-interactive...",
            ));
            return self
                .authenticate_keyboard_interactive(hop, handle, password)
                .await;
        }

        Err(SshError::Auth(format!(
            "password authentication failed (server allows: {})",
            describe_methods(&remaining_methods)
        )))
    }

    /// keyboard-interactive 认证：用已保存的密码自动应答密码提示
    async fn authenticate_keyboard_interactive(
        &self,
        hop: &SshConfig,
        handle: &mut ClientHandle,
        password: &str,
    ) -> Result<(), SshError> {
        let mut responder = KeyboardInteractiveResponder::new(password);
        let mut response = handle
            .authenticate_keyboard_interactive_start(hop.username.clone(), None::<String>)
            .await?;

        for _ in 0..MAX_KEYBOARD_INTERACTIVE_ROUNDS {
            match response {
                KeyboardInteractiveAuthResponse::Success => return Ok(()),
                KeyboardInteractiveAuthResponse::Failure {
                    remaining_methods,
                    partial_success,
                } => {
                    if partial_success {
                        return Err(partial_success_error(&remaining_methods));
                    }
                    return Err(SshError::Auth(
                        "password authentication failed (keyboard-interactive)".to_string(),
                    ));
                }
                KeyboardInteractiveAuthResponse::InfoRequest {
                    instructions,
                    prompts,
                    ..
                } => {
                    if !instructions.trim().is_empty() {
                        self.log(LogEntry::info(format!("Server: {}", instructions.trim())));
                    }
                    let answers = responder.answer(&prompts)?;
                    response = handle
                        .authenticate_keyboard_interactive_respond(answers)
                        .await?;
                }
            }
        }

        Err(SshError::Auth(
            "keyboard-interactive authentication did not complete".to_string(),
        ))
    }

    /// 公钥认证
    async fn authenticate_publickey(
        &self,
        hop: &SshConfig,
        handle: &mut ClientHandle,
        key_path: &Path,
        passphrase: Option<&str>,
    ) -> Result<(), SshError> {
        self.log(LogEntry::debug(format!(
            "Using public key authentication: {:?}",
            key_path
        )));

        let key = Arc::new(self.load_private_key(key_path, passphrase).await?);
        let is_rsa = key.algorithm().is_rsa();

        // RSA 密钥要协商签名哈希：不指定时 russh 使用 SHA-1（ssh-rsa），OpenSSH 8.8 起默认拒绝
        let hash_candidates: Vec<Option<HashAlg>> = if is_rsa {
            match handle.best_supported_rsa_hash().await {
                Ok(Some(best)) => vec![best],
                // 服务器没有告知支持的签名算法（RFC 8308）：依次尝试 SHA-512、SHA-256，最后才用 SHA-1
                Ok(None) => vec![Some(HashAlg::Sha512), Some(HashAlg::Sha256), None],
                Err(e) => {
                    self.log(LogEntry::warn(format!(
                        "Could not query server signature algorithms: {}",
                        e
                    )));
                    vec![Some(HashAlg::Sha512), Some(HashAlg::Sha256), None]
                }
            }
        } else {
            vec![None]
        };

        let mut remaining_methods = None;
        for hash_alg in hash_candidates {
            if is_rsa {
                self.log(LogEntry::debug(format!(
                    "Trying RSA signature algorithm {}",
                    rsa_signature_name(hash_alg)
                )));
            }
            let result = handle
                .authenticate_publickey(
                    hop.username.clone(),
                    PrivateKeyWithHashAlg::new(key.clone(), hash_alg),
                )
                .await?;
            match result {
                AuthResult::Success => return Ok(()),
                AuthResult::Failure {
                    remaining_methods: methods,
                    partial_success,
                } => {
                    if partial_success {
                        return Err(partial_success_error(&methods));
                    }
                    let can_retry = methods.contains(&MethodKind::PublicKey);
                    remaining_methods = Some(methods);
                    if !can_retry {
                        break;
                    }
                }
            }
        }

        Err(SshError::Auth(format!(
            "public key authentication failed (server allows: {})",
            remaining_methods
                .as_ref()
                .map(describe_methods)
                .unwrap_or_default()
        )))
    }

    /// 加载私钥文件
    async fn load_private_key(
        &self,
        key_path: &Path,
        passphrase: Option<&str>,
    ) -> Result<russh::keys::PrivateKey, SshError> {
        self.log(LogEntry::debug(format!(
            "Loading private key from {:?}",
            key_path
        )));

        let key_data = tokio::fs::read(key_path)
            .await
            .map_err(|e| SshError::Key(format!("Failed to read key file: {}", e)))?;

        let key = russh::keys::decode_secret_key(&String::from_utf8_lossy(&key_data), passphrase)
            .map_err(|e| {
                if passphrase.is_some() {
                    SshError::Key(format!("Failed to decode key: {}", e))
                } else {
                    SshError::Key(format!("Failed to decode key (no passphrase): {}", e))
                }
            })?;

        self.log(LogEntry::debug("Private key loaded successfully"));
        Ok(key)
    }
}

/// 跳板机连接出错时附带跳板机名称；已经带名称的（更内层的跳板机）保持原样
fn hop_error(jump: &SshConfig, error: SshError) -> SshError {
    match error {
        error @ (SshError::Hop { .. } | SshError::JumpHost(_)) => error,
        error => SshError::Hop {
            label: jump.display_name(),
            source: Box::new(error),
        },
    }
}

fn describe_methods(methods: &MethodSet) -> String {
    if methods.is_empty() {
        return "none".to_string();
    }
    methods
        .iter()
        .map(|method| <&'static str>::from(method))
        .collect::<Vec<_>>()
        .join(", ")
}

fn partial_success_error(remaining_methods: &MethodSet) -> SshError {
    SshError::Auth(format!(
        "partial authentication - the server requires additional methods: {}",
        describe_methods(remaining_methods)
    ))
}

fn rsa_signature_name(hash_alg: Option<HashAlg>) -> &'static str {
    match hash_alg {
        Some(HashAlg::Sha512) => "rsa-sha2-512",
        Some(HashAlg::Sha256) => "rsa-sha2-256",
        _ => "ssh-rsa",
    }
}

/// 用已保存的密码应答 keyboard-interactive 提示。
///
/// 只应答一次不回显的提示（即密码）。之后若再次要求输入，说明密码错误或需要
/// 二次验证码等额外输入，后者暂不支持，直接报错而不是乱填。
struct KeyboardInteractiveResponder<'a> {
    password: &'a str,
    password_sent: bool,
}

impl<'a> KeyboardInteractiveResponder<'a> {
    fn new(password: &'a str) -> Self {
        Self {
            password,
            password_sent: false,
        }
    }

    fn answer(&mut self, prompts: &[Prompt]) -> Result<Vec<String>, SshError> {
        let mut answers = Vec::with_capacity(prompts.len());
        for prompt in prompts {
            if !prompt.echo && !self.password_sent {
                self.password_sent = true;
                answers.push(self.password.to_string());
            } else if self.password_sent && prompt.prompt.to_lowercase().contains("password") {
                return Err(SshError::Auth(
                    "password authentication failed (keyboard-interactive)".to_string(),
                ));
            } else {
                return Err(SshError::Auth(format!(
                    "the server asks for additional input ({}), which is not supported yet",
                    prompt.prompt.trim()
                )));
            }
        }
        Ok(answers)
    }
}

/// 带超时地运行握手。超时只统计网络耗时：等待用户确认主机密钥期间暂停计时，
/// 用户作出选择后重新开始计时。返回 None 表示超时。
async fn with_handshake_timeout<F: Future>(
    future: F,
    limit: Duration,
    mut awaiting_user: watch::Receiver<bool>,
) -> Option<F::Output> {
    tokio::pin!(future);
    let mut deadline = Instant::now() + limit;
    let mut watching = true;

    loop {
        let paused = watching && *awaiting_user.borrow();
        tokio::select! {
            output = &mut future => return Some(output),
            _ = sleep_until(deadline), if !paused => return None,
            changed = awaiting_user.changed(), if watching => {
                if changed.is_err() {
                    // 发送端已释放（握手已经结束），之后只等待 future 自身完成
                    watching = false;
                    deadline = Instant::now() + limit;
                } else if !*awaiting_user.borrow() {
                    // 用户已作出选择，重新开始计时
                    deadline = Instant::now() + limit;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(text: &str, echo: bool) -> Prompt {
        Prompt {
            prompt: text.to_string(),
            echo,
        }
    }

    #[test]
    fn keyboard_interactive_answers_password_prompt_once() {
        let mut responder = KeyboardInteractiveResponder::new("secret");
        assert_eq!(
            responder.answer(&[prompt("Password: ", false)]).unwrap(),
            vec!["secret".to_string()]
        );
        // 有些服务器先发一条不含提示的说明
        assert!(responder.answer(&[]).unwrap().is_empty());
        // 再次要求密码说明密码错误
        let err = responder.answer(&[prompt("Password: ", false)]).unwrap_err();
        assert!(err.to_string().contains("password authentication failed"));
    }

    #[test]
    fn keyboard_interactive_refuses_unknown_prompts() {
        let mut responder = KeyboardInteractiveResponder::new("secret");
        responder.answer(&[prompt("Password: ", false)]).unwrap();
        let err = responder
            .answer(&[prompt("Verification code: ", false)])
            .unwrap_err();
        assert!(err.to_string().contains("Verification code"));

        let mut responder = KeyboardInteractiveResponder::new("secret");
        assert!(responder.answer(&[prompt("Username: ", true)]).is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn handshake_timeout_pauses_while_waiting_for_user() {
        let (tx, rx) = watch::channel(false);
        let limit = Duration::from_secs(10);
        let future = async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            tx.send_replace(true);
            // 用户花了 60 秒核对指纹
            tokio::time::sleep(Duration::from_secs(60)).await;
            tx.send_replace(false);
            tokio::time::sleep(Duration::from_secs(5)).await;
            "done"
        };
        assert_eq!(with_handshake_timeout(future, limit, rx).await, Some("done"));
    }

    #[tokio::test(start_paused = true)]
    async fn handshake_timeout_still_applies_to_network_time() {
        let (_tx, rx) = watch::channel(false);
        let future = tokio::time::sleep(Duration::from_secs(30));
        assert_eq!(
            with_handshake_timeout(future, Duration::from_secs(10), rx).await,
            None
        );
    }
}
