// SSH 客户端核心实现

use std::net::ToSocketAddrs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use russh::client::Handle;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use super::config::{AuthMethod, SshConfig};
use super::error::SshError;
use super::event::{ConnectionEvent, ConnectionStage, HostKeyAction, LogEntry};
use super::handler::SshClientHandler;
use super::session::SshSession;

/// SSH 客户端
/// 负责建立 SSH 连接并返回 SshSession
pub struct SshClient {
    /// 连接配置
    config: SshConfig,
    /// 事件发送器
    event_sender: mpsc::UnboundedSender<ConnectionEvent>,
    /// Host key 响应接收器
    host_key_response_rx: Option<oneshot::Receiver<HostKeyAction>>,
}

impl SshClient {
    /// 创建新的 SSH 客户端
    pub fn new(
        config: SshConfig,
        event_sender: mpsc::UnboundedSender<ConnectionEvent>,
        host_key_response_rx: oneshot::Receiver<HostKeyAction>,
    ) -> Self {
        Self {
            config,
            event_sender,
            host_key_response_rx: Some(host_key_response_rx),
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

    /// 执行连接（异步）
    /// 返回 SshSession 用于后续操作
    pub async fn connect(&mut self, session_id: String) -> Result<SshSession, SshError> {
        // 阶段 1: 初始化
        self.emit_stage(ConnectionStage::Initializing);
        self.log(LogEntry::info("Starting SSH connection..."));
        self.log(LogEntry::debug(format!(
            "Target: {}@{}:{}",
            self.config.username, self.config.host, self.config.port
        )));

        // 解析地址
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let socket_addr = addr
            .to_socket_addrs()
            .map_err(|e| SshError::Config(format!("Failed to resolve address: {}", e)))?
            .next()
            .ok_or_else(|| SshError::Config("No valid address found".to_string()))?;

        // 阶段 2: TCP 连接
        self.emit_stage(ConnectionStage::ConnectingHost);
        self.log(LogEntry::info(format!("Connecting to {}...", socket_addr)));

        let connect_timeout = Duration::from_secs(self.config.connect_timeout);
        let tcp_stream = timeout(connect_timeout, TcpStream::connect(socket_addr))
            .await
            .map_err(|_| SshError::Timeout(self.config.connect_timeout))?
            .map_err(SshError::Io)?;

        self.log(LogEntry::info("TCP connection established"));

        // 阶段 3: SSH 握手
        self.emit_stage(ConnectionStage::Handshaking);
        self.log(LogEntry::info("Starting SSH handshake..."));

        let russh_config = Arc::new(self.config.to_russh_config());

        // 获取 host key response receiver
        let host_key_rx = self
            .host_key_response_rx
            .take()
            .expect("host_key_response_rx should be set");

        let handler = SshClientHandler::new(
            self.event_sender.clone(),
            self.config.host.clone(),
            self.config.port,
            host_key_rx,
        );

        let mut handle = timeout(
            connect_timeout,
            russh::client::connect_stream(russh_config, tcp_stream, handler),
        )
        .await
        .map_err(|_| SshError::Timeout(self.config.connect_timeout))?
        .map_err(SshError::from)?;

        self.log(LogEntry::info("SSH handshake completed"));

        // 阶段 4: 认证
        self.emit_stage(ConnectionStage::Authenticating);
        self.log(LogEntry::info(format!(
            "Authenticating as '{}'...",
            self.config.username
        )));

        self.authenticate(&mut handle).await?;

        self.log(LogEntry::info("Authentication successful"));

        // 阶段 5: 建立通道
        self.emit_stage(ConnectionStage::EstablishingChannel);
        self.log(LogEntry::info("Establishing secure channel..."));

        // 阶段 6: 启动会话
        self.emit_stage(ConnectionStage::StartingSession);
        self.log(LogEntry::info("Starting session..."));

        // 阶段 7: 连接完成
        self.emit_stage(ConnectionStage::Connected);
        self.log(LogEntry::info("SSH connection established successfully!"));

        // 发送连接成功事件
        let _ = self.event_sender.send(ConnectionEvent::Connected {
            session_id: session_id.clone(),
        });

        // 创建 SshSession
        let session = SshSession::new(
            session_id,
            Arc::new(handle),
            self.config.host.clone(),
            self.config.username.clone(),
        );

        Ok(session)
    }

    /// 执行认证
    async fn authenticate(&self, handle: &mut Handle<SshClientHandler>) -> Result<(), SshError> {
        use russh::client::AuthResult;

        match &self.config.auth {
            AuthMethod::Password(password) => {
                self.log(LogEntry::debug("Using password authentication"));

                let auth_result = handle
                    .authenticate_password(&self.config.username, password)
                    .await
                    .map_err(SshError::from)?;

                match auth_result {
                    AuthResult::Success => {}
                    AuthResult::Failure {
                        remaining_methods,
                        partial_success,
                    } => {
                        if partial_success {
                            return Err(SshError::Auth(
                                "Partial authentication - additional auth required".to_string(),
                            ));
                        }
                        return Err(SshError::Auth(format!(
                            "Password authentication failed. Server suggests: {:?}",
                            remaining_methods
                        )));
                    }
                }
            }
            AuthMethod::PublicKey {
                key_path,
                passphrase,
            } => {
                self.log(LogEntry::debug(format!(
                    "Using public key authentication: {:?}",
                    key_path
                )));

                let key = self
                    .load_private_key(key_path, passphrase.as_deref())
                    .await?;

                // Wrap the key in PrivateKeyWithHashAlg
                let key_with_alg = russh::keys::PrivateKeyWithHashAlg::new(
                    Arc::new(key),
                    None, // Use default hash algorithm
                );

                let auth_result = handle
                    .authenticate_publickey(&self.config.username, key_with_alg)
                    .await
                    .map_err(SshError::from)?;

                match auth_result {
                    AuthResult::Success => {}
                    AuthResult::Failure {
                        remaining_methods,
                        partial_success,
                    } => {
                        if partial_success {
                            return Err(SshError::Auth(
                                "Partial authentication - additional auth required".to_string(),
                            ));
                        }
                        return Err(SshError::Auth(format!(
                            "Public key authentication failed. Server suggests: {:?}",
                            remaining_methods
                        )));
                    }
                }
            }
            AuthMethod::KeyboardInteractive => {
                // 预留：交互式键盘认证
                return Err(SshError::Auth(
                    "Keyboard interactive authentication not yet implemented".to_string(),
                ));
            }
        }

        Ok(())
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

        // 读取密钥文件
        let key_data = tokio::fs::read(key_path)
            .await
            .map_err(|e| SshError::Key(format!("Failed to read key file: {}", e)))?;

        // 解析密钥（根据是否有密码）
        let key = if let Some(pass) = passphrase {
            russh::keys::decode_secret_key(&String::from_utf8_lossy(&key_data), Some(pass))
                .map_err(|e| SshError::Key(format!("Failed to decode key: {}", e)))?
        } else {
            russh::keys::decode_secret_key(&String::from_utf8_lossy(&key_data), None).map_err(
                |e| SshError::Key(format!("Failed to decode key (no passphrase): {}", e)),
            )?
        };

        self.log(LogEntry::debug("Private key loaded successfully"));
        Ok(key)
    }
}
