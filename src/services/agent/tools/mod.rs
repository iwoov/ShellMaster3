// Agent 终端/SSH 工具集合

pub mod read_screen;
pub mod run_terminal;
pub mod send_keys;
pub mod ssh_exec;

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::ssh::session::{SshSession, TerminalChannel};
use crate::terminal::SharedTerm;

/// 传给所有终端工具的线程安全会话上下文。
#[derive(Clone)]
pub struct AgentSessionContext {
    pub ssh_session: Option<Arc<SshSession>>,
    pub active_pty: Option<Arc<TerminalChannel>>,
    pub active_term: Option<SharedTerm>,
    pub command_timeout_secs: u64,
    pub cancel_token: CancellationToken,
}

/// 工具执行错误类型。
#[derive(Debug, thiserror::Error)]
pub enum AgentToolError {
    #[error("当前会话未连接 SSH")]
    NotConnected,
    #[error("当前没有已打开的可用终端")]
    NoActiveTerminal,
    #[error("执行已取消")]
    Cancelled,
    #[error("执行超时（超过 {0} 秒）")]
    Timeout(u64),
    #[error("SSH 执行失败: {0}")]
    Ssh(String),
    #[error("参数无效: {0}")]
    InvalidArgs(String),
}

/// 限制单次返回给模型的文本长度，防止超长输出撑爆上下文窗口。
pub fn truncate_output(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_string(), false);
    }
    // 尽量保留末尾内容（命令行报错和结果通常在末尾），并在 UTF-8 字符边界截断
    let mut start = text.len().saturating_sub(max_bytes);
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    (
        format!(
            "[... 输出过长，已省略前 {} 字节 ...]\n{}",
            start,
            &text[start..]
        ),
        true,
    )
}
