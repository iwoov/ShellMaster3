// SSH 错误类型定义

use thiserror::Error;

/// SSH 错误类型
#[derive(Debug, Error)]
pub enum SshError {
    /// 配置错误
    #[error("Configuration error: {0}")]
    Config(String),

    /// IO 错误（网络连接等）
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// 认证失败
    #[error("Authentication failed: {0}")]
    Auth(String),

    /// SSH 协议错误
    #[error("SSH protocol error: {0}")]
    Protocol(String),

    /// 密钥错误
    #[error("Key error: {0}")]
    Key(String),

    /// 主机密钥校验未通过（用户拒绝、密钥变化等）
    #[error("Host key verification failed: {0}")]
    HostKey(String),

    /// 代理错误
    #[error("Proxy error: {0}")]
    Proxy(String),

    /// 跳板机错误（如无法经跳板机建立到目标主机的转发通道）
    #[error("Jump host error: {0}")]
    JumpHost(String),

    /// 连接跳板机本身时出错，附带出错的跳板机名称
    #[error("Jump host '{label}': {source}")]
    Hop {
        label: String,
        #[source]
        source: Box<SshError>,
    },

    /// 连接超时
    #[error("Connection timeout after {0}s")]
    Timeout(u64),

    /// 通道错误
    #[error("Channel error: {0}")]
    Channel(String),

    /// 会话已断开
    #[error("Session disconnected: {0}")]
    Disconnected(String),
}

impl SshError {
    /// 是否值得自动重试。
    ///
    /// 认证失败、主机密钥校验失败、配置错误等重试也不会成功，
    /// 自动重连遇到这类错误应立即停止并提示用户。
    pub fn is_retryable(&self) -> bool {
        match self {
            SshError::Config(_) | SshError::Auth(_) | SshError::Key(_) | SshError::HostKey(_) => {
                false
            }
            SshError::Hop { source, .. } => source.is_retryable(),
            _ => true,
        }
    }
}

impl From<russh::Error> for SshError {
    fn from(e: russh::Error) -> Self {
        SshError::Protocol(e.to_string())
    }
}

impl From<russh::keys::Error> for SshError {
    fn from(e: russh::keys::Error) -> Self {
        SshError::Key(e.to_string())
    }
}
