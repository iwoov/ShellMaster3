// SSH 连接事件定义

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local};
use tokio::sync::oneshot;

/// 连接事件（用于 UI 显示）
#[derive(Clone, Debug)]
pub enum ConnectionEvent {
    /// 阶段变化
    StageChanged(ConnectionStage),
    /// 日志消息
    Log(LogEntry),
    /// 连接成功（会话已注册到 SshManager，可直接打开通道）
    Connected { session_id: String },
    /// 连接失败
    Failed {
        error: String,
        /// 是否值得自动重试（认证失败、主机密钥被拒等为 false）
        retryable: bool,
    },
    /// 需要用户确认未知主机
    HostKeyVerification {
        host: String,
        port: u16,
        key_type: String,
        fingerprint: String,
        /// 用户作出选择后通过它回传
        responder: HostKeyResponder,
    },
    /// 主机密钥变化警告（可能安全风险）
    HostKeyMismatch {
        host: String,
        port: u16,
        key_type: String,
        expected_fingerprint: String,
        actual_fingerprint: String,
        /// 用户作出选择后通过它回传
        responder: HostKeyResponder,
    },
}

/// 用户对主机密钥的响应
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostKeyAction {
    /// 接受并保存到 known hosts
    AcceptAndSave,
    /// 仅此次接受（不保存）
    AcceptOnce,
    /// 拒绝连接
    Reject,
}

/// 主机密钥确认的应答通道。
///
/// 每次确认请求各自携带一个，经跳板机连接时可能先后需要确认多台主机。
/// 所有副本都被丢弃而未应答时，等待方按“拒绝”处理。
#[derive(Clone)]
pub struct HostKeyResponder(Arc<Mutex<Option<oneshot::Sender<HostKeyAction>>>>);

impl HostKeyResponder {
    pub(crate) fn new() -> (Self, oneshot::Receiver<HostKeyAction>) {
        let (tx, rx) = oneshot::channel();
        (Self(Arc::new(Mutex::new(Some(tx)))), rx)
    }

    /// 回传用户的选择；同一个确认请求只有第一次应答生效
    pub fn respond(&self, action: HostKeyAction) {
        let sender = self.0.lock().ok().and_then(|mut slot| slot.take());
        if let Some(sender) = sender {
            let _ = sender.send(action);
        }
    }
}

impl std::fmt::Debug for HostKeyResponder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostKeyResponder")
    }
}

/// 连接阶段
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConnectionStage {
    /// 初始化（解析配置、准备连接）
    Initializing = 0,
    /// 连接代理（如果配置了代理）
    ConnectingProxy = 1,
    /// 连接跳板机（如果配置了跳板机）
    ConnectingJumpHost = 2,
    /// TCP 连接目标主机
    ConnectingHost = 3,
    /// SSH 握手（密钥交换）
    Handshaking = 4,
    /// 身份认证
    Authenticating = 5,
    /// 建立安全通道
    EstablishingChannel = 6,
    /// 启动会话
    StartingSession = 7,
    /// 连接完成
    Connected = 8,
}

impl ConnectionStage {
    /// 获取阶段的本地化名称（简体中文）
    pub fn label_zh(&self) -> &'static str {
        match self {
            Self::Initializing => "初始化连接",
            Self::ConnectingProxy => "连接代理服务器",
            Self::ConnectingJumpHost => "连接跳板机",
            Self::ConnectingHost => "连接目标主机",
            Self::Handshaking => "SSH 握手",
            Self::Authenticating => "验证身份",
            Self::EstablishingChannel => "建立安全通道",
            Self::StartingSession => "启动会话",
            Self::Connected => "连接成功",
        }
    }

    /// 获取阶段的本地化名称（英文）
    pub fn label_en(&self) -> &'static str {
        match self {
            Self::Initializing => "Initializing",
            Self::ConnectingProxy => "Connecting to proxy",
            Self::ConnectingJumpHost => "Connecting to jump host",
            Self::ConnectingHost => "Connecting to host",
            Self::Handshaking => "SSH handshake",
            Self::Authenticating => "Authenticating",
            Self::EstablishingChannel => "Establishing channel",
            Self::StartingSession => "Starting session",
            Self::Connected => "Connected",
        }
    }
}

/// 日志级别
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// 日志条目
#[derive(Clone, Debug)]
pub struct LogEntry {
    /// 时间戳
    pub timestamp: DateTime<Local>,
    /// 日志级别
    pub level: LogLevel,
    /// 消息内容
    pub message: String,
}

impl LogEntry {
    /// 创建新的日志条目
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: Local::now(),
            level,
            message: message.into(),
        }
    }

    /// 创建 Debug 级别日志
    pub fn debug(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Debug, message)
    }

    /// 创建 Info 级别日志
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Info, message)
    }

    /// 创建 Warn 级别日志
    pub fn warn(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Warn, message)
    }

    /// 创建 Error 级别日志
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Error, message)
    }
}
