// SSH 连接模块
//
// 模块结构:
// - config: 连接配置 (SshConfig, AuthMethod)
// - error: 错误类型 (SshError)
// - event: 连接事件 (ConnectionEvent, ConnectionStage, LogEntry)
// - handler: russh Handler 实现
// - client: SSH 客户端核心
// - session: SSH 会话管理 (SshSession, TerminalChannel, ExecChannel, SftpChannel)
// - connector: 连接启动器 (与 UI 集成)

pub mod client;
pub mod config;
pub mod connector;
pub mod error;
pub mod event;
pub mod handler;
pub mod manager;
pub mod proxy;
pub mod reconnect;
pub mod session;

// 公开导出
pub use client::SshClient;
pub use config::{AuthMethod, KeepaliveConfig, SshConfig};
pub use connector::start_ssh_connection;
pub use error::SshError;
pub use event::{ConnectionEvent, ConnectionStage, LogEntry, LogLevel};
pub use manager::SshManager;
pub use reconnect::{start_manual_reconnection, start_reconnection};
pub use session::{
    CommandOutput, ExecChannel, PtyRequest, SftpChannel, SshSession, TerminalChannel,
};
