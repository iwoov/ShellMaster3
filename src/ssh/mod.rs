// SSH 连接模块
//
// 模块结构:
// - config: 连接配置 (SshConfig, AuthMethod)，从保存的服务器配置构建（含跳板机、代理）
// - error: 错误类型 (SshError)
// - event: 连接事件 (ConnectionEvent, ConnectionStage, LogEntry)
// - handler: russh Handler 实现（主机密钥校验）
// - net: TCP 连接建立（异步 DNS + 多地址回退）
// - proxy: HTTP / SOCKS4 / SOCKS5 代理
// - client: SSH 客户端核心（跳板机链、握手、认证）
// - session: SSH 会话管理 (SshSession, TerminalChannel, ExecChannel)
// - manager: 全局会话与连接任务管理
// - connector: 连接启动器 (与 UI 集成)
// - reconnect: 自动重连

pub mod client;
pub mod config;
pub mod connector;
pub mod error;
pub mod event;
pub mod forwarding;
pub mod handler;
pub mod manager;
pub mod net;
pub mod proxy;
pub mod reconnect;
pub mod session;

#[cfg(test)]
mod tests;

// 公开导出
pub use connector::start_ssh_connection;
pub use manager::SshManager;
pub use reconnect::{handle_connection_lost, start_manual_reconnection};
