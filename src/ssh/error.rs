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

    /// 代理错误（预留）
    #[error("Proxy error: {0}")]
    Proxy(String),

    /// 跳板机错误（预留）
    #[error("Jump host error: {0}")]
    JumpHost(String),

    /// 连接超时
    #[error("Connection timeout after {0}s")]
    Timeout(u64),

    /// 通道错误
    #[error("Channel error: {0}")]
    Channel(String),

    /// 会话已断开
    #[error("Session disconnected: {0}")]
    Disconnected(String),

    /// 连接已取消
    #[error("Connection cancelled")]
    Cancelled,
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
