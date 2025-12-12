// SSH 会话管理
// 连接成功后的会话对象，提供多通道支持

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use russh::client::Handle;
use russh::client::Msg;
use russh::ChannelMsg;
use tokio::sync::Mutex;

use super::error::SshError;
use super::handler::SshClientHandler;

/// PTY 请求参数
#[derive(Clone, Debug)]
pub struct PtyRequest {
    /// 终端类型
    pub term: String,
    /// 列数
    pub col_width: u32,
    /// 行数
    pub row_height: u32,
    /// 像素宽度
    pub pix_width: u32,
    /// 像素高度
    pub pix_height: u32,
    /// 终端模式
    pub modes: Vec<(russh::Pty, u32)>,
}

impl Default for PtyRequest {
    fn default() -> Self {
        Self {
            term: "xterm-256color".to_string(),
            col_width: 80,
            row_height: 24,
            pix_width: 0,
            pix_height: 0,
            modes: vec![],
        }
    }
}

/// SSH 会话（连接成功后）
/// 内部持有 Handle，支持并发打开多个通道
pub struct SshSession {
    /// 会话 ID
    id: String,
    /// 共享的 russh Handle（Arc 包装）
    handle: Arc<Handle<SshClientHandler>>,
    /// 服务器主机名
    host: String,
    /// 用户名
    username: String,
    /// 连接状态
    is_connected: AtomicBool,
}

impl SshSession {
    /// 创建新的会话
    pub fn new(
        id: String,
        handle: Arc<Handle<SshClientHandler>>,
        host: String,
        username: String,
    ) -> Self {
        Self {
            id,
            handle,
            host,
            username,
            is_connected: AtomicBool::new(true),
        }
    }

    /// 获取会话 ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 获取主机名
    pub fn host(&self) -> &str {
        &self.host
    }

    /// 获取用户名
    pub fn username(&self) -> &str {
        &self.username
    }

    /// 检查会话是否活跃
    pub fn is_alive(&self) -> bool {
        self.is_connected.load(Ordering::Relaxed)
    }

    /// 标记会话断开
    pub fn mark_disconnected(&self) {
        self.is_connected.store(false, Ordering::Relaxed);
    }

    /// 获取 Handle 引用（供高级用途）
    pub fn handle(&self) -> Arc<Handle<SshClientHandler>> {
        self.handle.clone()
    }

    /// 打开终端 Shell 通道
    pub async fn open_terminal(&self, pty: PtyRequest) -> Result<TerminalChannel, SshError> {
        if !self.is_alive() {
            return Err(SshError::Disconnected(
                "Session is disconnected".to_string(),
            ));
        }

        // 打开会话通道
        let channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(SshError::from)?;

        // 请求 PTY
        channel
            .request_pty(
                false, // want_reply
                &pty.term,
                pty.col_width,
                pty.row_height,
                pty.pix_width,
                pty.pix_height,
                &pty.modes,
            )
            .await
            .map_err(SshError::from)?;

        // 请求 Shell
        channel.request_shell(false).await.map_err(SshError::from)?;

        Ok(TerminalChannel::new(channel, self.handle.clone()))
    }

    /// 打开执行通道（用于 Monitor 执行命令）
    pub async fn open_exec(&self) -> Result<ExecChannel, SshError> {
        if !self.is_alive() {
            return Err(SshError::Disconnected(
                "Session is disconnected".to_string(),
            ));
        }

        let channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(SshError::from)?;

        Ok(ExecChannel::new(channel))
    }

    /// 打开 SFTP 通道
    pub async fn open_sftp(&self) -> Result<SftpChannel, SshError> {
        if !self.is_alive() {
            return Err(SshError::Disconnected(
                "Session is disconnected".to_string(),
            ));
        }

        let channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(SshError::from)?;

        // 请求 SFTP 子系统
        channel
            .request_subsystem(false, "sftp")
            .await
            .map_err(SshError::from)?;

        Ok(SftpChannel::new(channel))
    }

    /// 关闭会话
    pub async fn close(&self) -> Result<(), SshError> {
        self.mark_disconnected();
        // Handle 会在 drop 时自动关闭连接
        Ok(())
    }
}

// 使用 russh::client::Msg 作为消息类型
type RusshChannel = russh::Channel<Msg>;

/// 终端通道
/// 分离读写路径以避免死锁：
/// - 读：需要 channel.wait()，会持有 channel 内部状态
/// - 写：直接使用 handle.data()，不需要持有 channel 锁
pub struct TerminalChannel {
    id: russh::ChannelId,
    handle: Arc<Handle<SshClientHandler>>,
    channel: Mutex<RusshChannel>,
}

impl TerminalChannel {
    fn new(channel: RusshChannel, handle: Arc<Handle<SshClientHandler>>) -> Self {
        Self {
            id: channel.id(),
            channel: Mutex::new(channel),
            handle,
        }
    }

    /// 写入数据到终端
    /// 直接通过 handle 发送，不阻塞读取循环
    pub async fn write(&self, data: &[u8]) -> Result<(), SshError> {
        self.handle
            .data(self.id, data.to_vec().into())
            .await
            .map_err(|_| SshError::Channel("Failed to send data to channel".to_string()))
    }

    /// 读取终端输出
    /// 返回 None 表示通道已关闭
    pub async fn read(&self) -> Result<Option<Vec<u8>>, SshError> {
        let mut channel = self.channel.lock().await;

        // wait() on Channel<Msg> returns Option<ChannelMsg> directly
        match channel.wait().await {
            Some(channel_msg) => match channel_msg {
                ChannelMsg::Data { data } => Ok(Some(data.to_vec())),
                ChannelMsg::ExtendedData { data, .. } => Ok(Some(data.to_vec())),
                ChannelMsg::Eof | ChannelMsg::Close => Ok(None),
                _ => Ok(Some(vec![])),
            },
            None => Ok(None),
        }
    }

    /// 调整终端大小
    pub async fn resize(&self, cols: u32, rows: u32) -> Result<(), SshError> {
        let channel = self.channel.lock().await;
        channel
            .window_change(cols, rows, 0, 0)
            .await
            .map_err(|e| SshError::Channel(e.to_string()))
    }

    /// 关闭通道
    pub async fn close(&self) -> Result<(), SshError> {
        let channel = self.channel.lock().await;
        channel
            .eof()
            .await
            .map_err(|e| SshError::Channel(e.to_string()))
    }
}

/// 执行通道（用于 Monitor 等需要执行单个命令的场景）
pub struct ExecChannel {
    channel: Mutex<RusshChannel>,
}

impl ExecChannel {
    fn new(channel: RusshChannel) -> Self {
        Self {
            channel: Mutex::new(channel),
        }
    }

    /// 执行命令并获取输出
    pub async fn exec(&self, command: &str) -> Result<CommandOutput, SshError> {
        let mut channel = self.channel.lock().await;

        channel
            .exec(true, command)
            .await
            .map_err(|e| SshError::Channel(e.to_string()))?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = None;

        loop {
            match channel.wait().await {
                Some(channel_msg) => match channel_msg {
                    ChannelMsg::Data { data } => {
                        stdout.extend_from_slice(&data);
                    }
                    ChannelMsg::ExtendedData { data, ext } => {
                        if ext == 1 {
                            // stderr
                            stderr.extend_from_slice(&data);
                        }
                    }
                    ChannelMsg::ExitStatus { exit_status } => {
                        exit_code = Some(exit_status);
                    }
                    ChannelMsg::Eof | ChannelMsg::Close => {
                        break;
                    }
                    _ => {}
                },
                None => break,
            }
        }

        Ok(CommandOutput {
            stdout,
            stderr,
            exit_code: exit_code.unwrap_or(0),
        })
    }
}

/// 命令输出
#[derive(Debug)]
pub struct CommandOutput {
    /// 标准输出
    pub stdout: Vec<u8>,
    /// 标准错误
    pub stderr: Vec<u8>,
    /// 退出码
    pub exit_code: u32,
}

impl CommandOutput {
    /// 获取标准输出字符串
    pub fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }

    /// 获取标准错误字符串
    pub fn stderr_string(&self) -> String {
        String::from_utf8_lossy(&self.stderr).to_string()
    }

    /// 检查命令是否成功
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

/// SFTP 通道
pub struct SftpChannel {
    channel: Mutex<RusshChannel>,
}

impl SftpChannel {
    fn new(channel: RusshChannel) -> Self {
        Self {
            channel: Mutex::new(channel),
        }
    }

    /// 获取底层通道用于 SFTP 操作
    /// 后续可以集成 russh-sftp 或自行实现 SFTP 协议
    pub async fn inner(&self) -> tokio::sync::MutexGuard<'_, RusshChannel> {
        self.channel.lock().await
    }

    /// 写入数据
    pub async fn write(&self, data: &[u8]) -> Result<(), SshError> {
        let channel = self.channel.lock().await;
        channel
            .data(data)
            .await
            .map_err(|e| SshError::Channel(e.to_string()))
    }

    /// 读取数据
    pub async fn read(&self) -> Result<Option<Vec<u8>>, SshError> {
        let mut channel = self.channel.lock().await;

        match channel.wait().await {
            Some(channel_msg) => match channel_msg {
                ChannelMsg::Data { data } => Ok(Some(data.to_vec())),
                ChannelMsg::Eof | ChannelMsg::Close => Ok(None),
                _ => Ok(Some(vec![])),
            },
            None => Ok(None),
        }
    }

    /// 关闭通道
    pub async fn close(&self) -> Result<(), SshError> {
        let channel = self.channel.lock().await;
        channel
            .eof()
            .await
            .map_err(|e| SshError::Channel(e.to_string()))
    }
}
