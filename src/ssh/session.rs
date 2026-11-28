// SSH 会话管理
// 连接成功后的会话对象，提供多通道支持

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use russh::client::Handle;
use russh::client::Msg;
use russh::{ChannelMsg, ChannelReadHalf};
use tokio::sync::{oneshot, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use super::error::SshError;
use super::forwarding::{
    start_port_forward as run_port_forward, ActivePortForward, PortForwardRule,
    RemoteForwardRegistry,
};
use super::handler::SshClientHandler;

/// 终端读取时单批合并的最大字节数
const TERMINAL_READ_BATCH_BYTES: usize = 256 * 1024;

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
    /// 可选的远端启动命令；为空时请求服务器默认 shell。
    pub command: Option<String>,
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
            command: None,
        }
    }
}

/// 连接关闭信号
///
/// 目标连接的 russh 会话结束时（服务器断开、网络中断、心跳超时或主动关闭）触发，
/// 让上层不依赖终端是否打开就能感知断线。
#[derive(Clone, Default)]
pub struct ConnectionClosed {
    token: CancellationToken,
    reason: Arc<std::sync::Mutex<Option<String>>>,
}

impl ConnectionClosed {
    pub fn new() -> Self {
        Self::default()
    }

    /// 标记连接已关闭；只记录第一次的原因
    pub fn close(&self, reason: impl Into<String>) {
        if let Ok(mut slot) = self.reason.lock() {
            if slot.is_none() {
                *slot = Some(reason.into());
            }
        }
        self.token.cancel();
    }

    pub fn is_closed(&self) -> bool {
        self.token.is_cancelled()
    }

    /// 等待连接关闭（已关闭时立即返回；不依赖 tokio 运行时，可在 UI 任务中等待）
    pub async fn wait(&self) {
        self.token.cancelled().await
    }

    pub fn reason(&self) -> Option<String> {
        self.reason.lock().ok().and_then(|reason| reason.clone())
    }
}

/// SSH 会话（连接成功后）
/// 内部持有 Handle，支持并发打开多个通道
pub struct SshSession {
    /// 会话 ID
    id: String,
    /// 共享的 russh Handle（Arc<RwLock> 包装：绝大多数操作读锁并发，仅 tcpip_forward 短暂写锁）
    handle: Arc<RwLock<Handle<SshClientHandler>>>,
    /// 跳板机连接（由外到内）。目标连接经它们转发，必须与会话同生命周期
    jump_handles: Vec<Arc<Handle<SshClientHandler>>>,
    /// 本次连接各跳已确认的主机密钥（"host:port" -> 指纹），自动重连时据此免确认
    verified_host_keys: HashMap<String, String>,
    /// 服务器主机名
    host: String,
    /// 登录用户名（预留会话身份元数据，与 `host` 对称）
    #[allow(dead_code)]
    username: String,
    /// 连接状态
    is_connected: AtomicBool,
    /// 连接关闭信号（由目标连接的 Handler 在会话结束时触发）
    closed: ConnectionClosed,
    /// 远程端口转发注册表（与目标连接 Handler 共享）
    remote_forwards: RemoteForwardRegistry,
    /// 本会话运行中的端口转发实例（按规则 ID 存储）
    active_forwards: std::sync::Mutex<HashMap<String, ActivePortForward>>,
}

impl SshSession {
    /// 创建新的会话
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        handle: Arc<RwLock<Handle<SshClientHandler>>>,
        jump_handles: Vec<Arc<Handle<SshClientHandler>>>,
        verified_host_keys: HashMap<String, String>,
        host: String,
        username: String,
        closed: ConnectionClosed,
        remote_forwards: RemoteForwardRegistry,
    ) -> Self {
        Self {
            id,
            handle,
            jump_handles,
            verified_host_keys,
            host,
            username,
            is_connected: AtomicBool::new(true),
            closed,
            remote_forwards,
            active_forwards: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// 启动一条端口转发（-L / -R / -D）并返回实际生效的规则（若 bind_port 为 0 会填充系统分配端口）
    pub async fn start_port_forward(
        &self,
        rule: PortForwardRule,
    ) -> Result<PortForwardRule, String> {
        let active = run_port_forward(rule, self.handle.clone(), self.remote_forwards.clone()).await?;
        let effective_rule = active.rule.clone();
        if let Ok(mut map) = self.active_forwards.lock() {
            map.insert(effective_rule.id.clone(), active);
        }
        Ok(effective_rule)
    }

    /// 停止一条运行中的端口转发
    pub async fn stop_port_forward(&self, rule_id: &str) {
        let removed = self
            .active_forwards
            .lock()
            .ok()
            .and_then(|mut map| map.remove(rule_id));
        if let Some(active) = removed {
            active.stop().await;
        }
    }

    /// 取消本会话的所有端口转发（关闭会话或 Drop 时调用）
    pub fn cancel_all_port_forwards(&self) {
        if let Ok(mut map) = self.active_forwards.lock() {
            for (_, active) in map.drain() {
                active.cancel_now();
            }
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

    /// 获取登录用户名（预留会话身份查询接口）
    #[allow(dead_code)]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// 本次连接各跳已确认的主机密钥
    pub fn verified_host_keys(&self) -> &HashMap<String, String> {
        &self.verified_host_keys
    }

    /// 检查会话是否活跃（任何一跳断开都视为断开）
    pub fn is_alive(&self) -> bool {
        let handle_open = self
            .handle
            .try_read()
            .map(|h| !h.is_closed())
            .unwrap_or(true);
        self.is_connected.load(Ordering::Relaxed)
            && !self.closed.is_closed()
            && handle_open
            && self.jump_handles.iter().all(|jump| !jump.is_closed())
    }

    /// 等待连接关闭（已关闭时立即返回）
    pub async fn closed(&self) {
        self.closed.wait().await
    }

    /// 连接关闭的原因（服务器的断开说明或错误信息）
    pub fn close_reason(&self) -> Option<String> {
        self.closed.reason()
    }

    /// 标记会话断开
    pub fn mark_disconnected(&self) {
        self.is_connected.store(false, Ordering::Relaxed);
        self.closed.close("session closed");
    }

    /// 获取 Handle 引用（供高级用途）
    pub fn handle(&self) -> Arc<RwLock<Handle<SshClientHandler>>> {
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
            .read()
            .await
            .channel_open_session()
            .await
            .map_err(SshError::from)?;

        // 请求 PTY
        channel
            .request_pty(
                true, // want_reply
                &pty.term,
                pty.col_width,
                pty.row_height,
                pty.pix_width,
                pty.pix_height,
                &pty.modes,
            )
            .await
            .map_err(SshError::from)?;

        if let Some(command) = pty.command.filter(|command| !command.trim().is_empty()) {
            channel.exec(true, command).await.map_err(SshError::from)?;
        } else {
            channel.request_shell(true).await.map_err(SshError::from)?;
        }

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
            .read()
            .await
            .channel_open_session()
            .await
            .map_err(SshError::from)?;

        Ok(ExecChannel::new(channel))
    }

    /// 打开流式执行通道（用于 Monitor 常驻采集脚本）
    /// 与 `open_exec` 不同：启动一个长期运行的远端脚本，持续读取其输出而不等待退出。
    pub async fn open_exec_stream(&self, script: &str) -> Result<ExecStreamChannel, SshError> {
        if !self.is_alive() {
            return Err(SshError::Disconnected(
                "Session is disconnected".to_string(),
            ));
        }

        let channel = self
            .handle
            .read()
            .await
            .channel_open_session()
            .await
            .map_err(SshError::from)?;

        // 用 `sh -s` 从 stdin 读取脚本执行，而不是把脚本作为命令字符串传给远端登录 shell。
        // 否则当用户默认 shell 不是 POSIX sh（如 csh/tcsh/fish）时，多行脚本会被错误解析而立即失败。
        channel
            .exec(false, "sh -s")
            .await
            .map_err(|e| SshError::Channel(e.to_string()))?;

        // 把脚本写入 stdin，然后发送 EOF（仅半关闭写方向，服务端仍可持续输出 stdout）
        channel
            .data(script.as_bytes())
            .await
            .map_err(|e| SshError::Channel(e.to_string()))?;
        channel
            .eof()
            .await
            .map_err(|e| SshError::Channel(e.to_string()))?;

        Ok(ExecStreamChannel::new(channel))
    }

    /// 关闭会话：先断开目标连接，再由内向外断开跳板机连接（已断开的跳过）
    pub async fn close(&self) -> Result<(), SshError> {
        self.cancel_all_port_forwards();
        self.mark_disconnected();
        let handle_guard = self.handle.read().await;
        let result = if handle_guard.is_closed() {
            Ok(())
        } else {
            handle_guard
                .disconnect(
                    russh::Disconnect::ByApplication,
                    "ShellMaster session closed",
                    "en",
                )
                .await
                .map_err(SshError::from)
        };
        drop(handle_guard);
        for jump in self.jump_handles.iter().rev() {
            if !jump.is_closed() {
                let _ = jump
                    .disconnect(
                        russh::Disconnect::ByApplication,
                        "ShellMaster session closed",
                        "en",
                    )
                    .await;
            }
        }
        result
    }
}

// 使用 russh::client::Msg 作为消息类型
type RusshChannel = russh::Channel<Msg>;

/// 终端通道
/// 分离读写路径以避免死锁：
/// - 读：使用 read_half.wait()，独立的读取路径
/// - 写：使用 handle.data()，直接通过 handle 发送数据
/// - 控制：使用 write_half.window_change()/eof()，独立于读取循环
pub struct TerminalChannel {
    reader: Mutex<TerminalReader>,
    command_tx: tokio::sync::mpsc::UnboundedSender<TerminalCommand>,
}

struct TerminalReader {
    read_half: ChannelReadHalf,
    /// 已读到通道结束（合并数据时遇到 EOF/Close，需在下一次读取时报告）
    closed: bool,
}

enum TerminalCommand {
    Write(Vec<u8>),
    Resize {
        cols: u32,
        rows: u32,
        pix_width: u32,
        pix_height: u32,
    },
    Close(oneshot::Sender<Result<(), String>>),
}

impl TerminalChannel {
    fn new(channel: RusshChannel, handle: Arc<RwLock<Handle<SshClientHandler>>>) -> Self {
        let id = channel.id();
        let (read_half, write_half) = channel.split();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let writer_handle = handle.clone();
        crate::ssh::manager::SshManager::global()
            .runtime()
            .spawn(async move {
                while let Some(command) = command_rx.recv().await {
                    match command {
                        TerminalCommand::Write(data) => {
                            if writer_handle.read().await.data(id, data.into()).await.is_err() {
                                tracing::error!("[Terminal] Ordered PTY writer stopped");
                                break;
                            }
                        }
                        TerminalCommand::Resize {
                            cols,
                            rows,
                            pix_width,
                            pix_height,
                        } => {
                            if let Err(error) = write_half
                                .window_change(cols, rows, pix_width, pix_height)
                                .await
                            {
                                tracing::error!("[Terminal] PTY resize failed: {:?}", error);
                            }
                        }
                        TerminalCommand::Close(reply) => {
                            let result = async {
                                write_half.eof().await?;
                                write_half.close().await
                            }
                            .await
                            .map_err(|error| error.to_string());
                            let _ = reply.send(result);
                            break;
                        }
                    }
                }
            });
        Self {
            reader: Mutex::new(TerminalReader {
                read_half,
                closed: false,
            }),
            command_tx,
        }
    }

    /// 将数据按调用顺序送入单一 PTY writer，避免按键/鼠标任务相互乱序。
    pub fn queue_write(&self, data: impl Into<Vec<u8>>) -> Result<(), SshError> {
        self.command_tx
            .send(TerminalCommand::Write(data.into()))
            .map_err(|_| SshError::Channel("PTY writer is closed".to_string()))
    }

    /// 读取终端输出
    ///
    /// 等到第一块数据后，把已经到达的后续数据一并取出合并返回（最多约 256KB），
    /// 大量输出时显著减少切回 UI 线程的次数，也让 russh 的通道队列尽快腾空，
    /// 避免整条连接（含同连接上的 SFTP、监控）被一个处理不过来的终端拖住。
    ///
    /// 返回 None 表示通道已关闭
    pub async fn read(&self) -> Result<Option<Vec<u8>>, SshError> {
        let mut reader = self.reader.lock().await;
        if reader.closed {
            return Ok(None);
        }

        let mut output = loop {
            match reader.read_half.wait().await {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    break data.to_vec();
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                    reader.closed = true;
                    return Ok(None);
                }
                // 窗口调整、退出码等非数据消息直接跳过
                Some(_) => {}
            }
        };

        while output.len() < TERMINAL_READ_BATCH_BYTES {
            // now_or_never 只轮询一次：没有已到达的消息就立即返回（tokio mpsc 的 recv 可安全取消）
            match reader.read_half.wait().now_or_never() {
                Some(Some(ChannelMsg::Data { data }))
                | Some(Some(ChannelMsg::ExtendedData { data, .. })) => {
                    output.extend_from_slice(&data);
                }
                Some(Some(ChannelMsg::Eof)) | Some(Some(ChannelMsg::Close)) | Some(None) => {
                    reader.closed = true;
                    break;
                }
                Some(Some(_)) => {}
                None => break,
            }
        }

        Ok(Some(output))
    }

    /// 调整终端大小
    /// 使用 write_half 独立于读取循环，不会被阻塞
    pub fn resize(
        &self,
        cols: u32,
        rows: u32,
        pix_width: u32,
        pix_height: u32,
    ) -> Result<(), SshError> {
        self.command_tx
            .send(TerminalCommand::Resize {
                cols,
                rows,
                pix_width,
                pix_height,
            })
            .map_err(|_| SshError::Channel("PTY writer is closed".to_string()))
    }

    /// 关闭通道
    pub async fn close(&self) -> Result<(), SshError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(TerminalCommand::Close(reply_tx))
            .map_err(|_| SshError::Channel("PTY writer is closed".to_string()))?;
        reply_rx
            .await
            .map_err(|_| SshError::Channel("PTY close was cancelled".to_string()))?
            .map_err(SshError::Channel)
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

    /// 执行命令并获取输出（需在 SSH 运行时中调用）
    pub async fn exec(&self, command: &str) -> Result<CommandOutput, SshError> {
        let mut channel = self.channel.lock().await;

        channel
            .exec(true, command)
            .await
            .map_err(|e| SshError::Channel(e.to_string()))?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = None;
        let mut eof_received = false;

        // OpenSSH 通常在 EOF 之后才发送 exit-status，所以要一直读到通道关闭；
        // 收到 EOF 后最多再等 5 秒，防止个别服务器迟迟不关闭通道。
        loop {
            let message = if eof_received {
                match tokio::time::timeout(Duration::from_secs(5), channel.wait()).await {
                    Ok(message) => message,
                    Err(_) => break,
                }
            } else {
                channel.wait().await
            };

            match message {
                Some(ChannelMsg::Data { data }) => stdout.extend_from_slice(&data),
                // ext == 1 为 stderr
                Some(ChannelMsg::ExtendedData { data, ext: 1 }) => stderr.extend_from_slice(&data),
                Some(ChannelMsg::ExitStatus { exit_status }) => exit_code = Some(exit_status),
                Some(ChannelMsg::Eof) => eof_received = true,
                Some(ChannelMsg::Close) | None => break,
                Some(_) => {}
            }
        }

        Ok(CommandOutput {
            stdout,
            stderr,
            exit_code,
        })
    }
}

/// 流式执行通道（用于 Monitor 常驻采集脚本）
/// 仅做流式读取：脚本在远端长期运行并持续输出，逐块读取即可。
pub struct ExecStreamChannel {
    read_half: Mutex<ChannelReadHalf>,
}

impl ExecStreamChannel {
    fn new(channel: RusshChannel) -> Self {
        let (read_half, _write_half) = channel.split();
        Self {
            read_half: Mutex::new(read_half),
        }
    }

    /// 读取下一块输出数据
    /// - `Some(bytes)`：收到 stdout 数据
    /// - `None`：通道已结束（Eof/Close 或底层关闭）
    pub async fn read(&self) -> Result<Option<Vec<u8>>, SshError> {
        let mut read_half = self.read_half.lock().await;

        loop {
            match read_half.wait().await {
                Some(channel_msg) => match channel_msg {
                    ChannelMsg::Data { data } => return Ok(Some(data.to_vec())),
                    ChannelMsg::ExtendedData { data, ext } => {
                        // ext==0 为标准输出；stderr（ext==1）忽略
                        if ext == 0 {
                            return Ok(Some(data.to_vec()));
                        }
                    }
                    ChannelMsg::Eof | ChannelMsg::Close => return Ok(None),
                    _ => {}
                },
                None => return Ok(None),
            }
        }
    }
}

/// 命令输出
#[derive(Debug)]
pub struct CommandOutput {
    /// 标准输出
    pub stdout: Vec<u8>,
    /// 标准错误
    pub stderr: Vec<u8>,
    /// 退出码；被信号终止或服务器未返回退出码时为 None
    pub exit_code: Option<u32>,
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

    /// 检查命令是否成功（只有明确收到退出码 0 才算成功）
    pub fn is_success(&self) -> bool {
        self.exit_code == Some(0)
    }
}
