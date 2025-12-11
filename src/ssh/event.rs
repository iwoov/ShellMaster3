// SSH 连接事件定义

use chrono::{DateTime, Local};

/// 连接事件（用于 UI 显示）
#[derive(Clone, Debug)]
pub enum ConnectionEvent {
    /// 阶段变化
    StageChanged(ConnectionStage),
    /// 日志消息
    Log(LogEntry),
    /// 连接成功
    Connected { session_id: String },
    /// 连接失败
    Failed { error: String },
    /// 连接断开
    Disconnected { reason: String },
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

    /// 获取进度百分比 (0.0 - 1.0)
    pub fn progress(&self) -> f32 {
        match self {
            Self::Initializing => 0.0,
            Self::ConnectingProxy => 0.1,
            Self::ConnectingJumpHost => 0.2,
            Self::ConnectingHost => 0.3,
            Self::Handshaking => 0.5,
            Self::Authenticating => 0.7,
            Self::EstablishingChannel => 0.85,
            Self::StartingSession => 0.95,
            Self::Connected => 1.0,
        }
    }

    /// 直接连接时的递进（跳过 Proxy/JumpHost 阶段）
    pub fn next_direct(&self) -> Option<Self> {
        match self {
            Self::Initializing => Some(Self::ConnectingHost),
            Self::ConnectingHost => Some(Self::Handshaking),
            Self::Handshaking => Some(Self::Authenticating),
            Self::Authenticating => Some(Self::EstablishingChannel),
            Self::EstablishingChannel => Some(Self::StartingSession),
            Self::StartingSession => Some(Self::Connected),
            Self::Connected => None,
            // Proxy/JumpHost 阶段在直接连接时跳过
            _ => Some(Self::ConnectingHost),
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

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
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
    /// 详细信息（可选）
    pub details: Option<String>,
}

impl LogEntry {
    /// 创建新的日志条目
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: Local::now(),
            level,
            message: message.into(),
            details: None,
        }
    }

    /// 创建带详情的日志条目
    pub fn with_details(
        level: LogLevel,
        message: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Local::now(),
            level,
            message: message.into(),
            details: Some(details.into()),
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
