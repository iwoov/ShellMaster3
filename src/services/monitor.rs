// Monitor 后端服务
// 用一条常驻 SSH 通道运行远端采集脚本（collector.sh），持续读取按行输出的原始计数器，
// 由 `Collector` 和上一轮比较，换算成使用率、速率等监控数据。

mod collector;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{debug, trace, warn};

use crate::models::monitor::{DiskInfo, DiskIoInfo, LoadInfo, NetworkInfo, SystemInfo};
use crate::ssh::session::{ExecStreamChannel, SshSession};
use collector::Collector;

/// Monitor 事件类型
#[derive(Debug, Clone)]
pub enum MonitorEvent {
    /// 系统信息（静态，连接时获取一次）
    SystemInfo(SystemInfo),
    /// 系统负载信息（每 2 秒；连接后第一次约 1 秒后到达）
    LoadInfo(LoadInfo),
    /// 网络状态信息（每 2 秒）
    NetworkInfo(NetworkInfo),
    /// 磁盘读写速率（每 2 秒；与负载信息同时开始）
    DiskIo(DiskIoInfo),
    /// 磁盘容量信息（每 10 秒）
    DiskInfo(DiskInfo),
    /// 错误信息
    Error(String),
}

/// Monitor 服务配置
#[derive(Debug, Clone)]
pub struct MonitorSettings {
    /// 负载与网络采样刷新间隔（毫秒）
    pub load_interval_ms: u64,
    /// 磁盘容量等低频信息刷新间隔（毫秒）
    pub disk_interval_ms: u64,
}

impl Default for MonitorSettings {
    fn default() -> Self {
        Self {
            load_interval_ms: 2000,  // 2 秒
            disk_interval_ms: 10000, // 10 秒
        }
    }
}

/// 流式采集循环的结束原因
enum StreamOutcome {
    /// 收到停止信号
    Stopped,
    /// 通道结束（远端脚本退出 / 连接关闭）
    Ended {
        /// 本次连接是否至少收到过数据（用于决定是否重置退避）
        got_data: bool,
    },
}

/// Monitor 后端服务
pub struct MonitorService {
    session_id: String,
    stop_tx: Option<watch::Sender<bool>>,
    task_handle: Option<JoinHandle<()>>,
}

impl MonitorService {
    /// 创建 Monitor 服务
    /// 需要在 tokio 运行时上下文中调用，或者传入运行时句柄
    pub fn new(
        session_id: String,
        session: Arc<SshSession>,
        settings: MonitorSettings,
        runtime: &tokio::runtime::Runtime,
    ) -> (Self, mpsc::UnboundedReceiver<MonitorEvent>) {
        let (data_tx, data_rx) = mpsc::unbounded_channel();
        let (stop_tx, stop_rx) = watch::channel(false);

        // 使用传入的运行时来启动轮询任务
        let task = runtime.spawn(Self::run_polling_loop(
            session_id.clone(),
            session,
            settings,
            data_tx,
            stop_rx,
        ));

        let service = Self {
            session_id,
            stop_tx: Some(stop_tx),
            task_handle: Some(task),
        };

        (service, data_rx)
    }

    /// 停止监控
    pub fn stop(&mut self) {
        let mut stopped = false;
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(true);
            stopped = true;
        }
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
            stopped = true;
        }
        if stopped {
            debug!("[Monitor] Service stopped for session {}", self.session_id);
        }
    }

    /// 轮询主循环
    ///
    /// 打开一条常驻流式通道运行采集脚本，持续读取按行输出的数据并转换为 `MonitorEvent`。
    /// 通道意外结束且会话仍存活时，按指数退避自动重连；超过最大重试次数才上报错误并退出。
    async fn run_polling_loop(
        session_id: String,
        session: Arc<SshSession>,
        settings: MonitorSettings,
        data_tx: mpsc::UnboundedSender<MonitorEvent>,
        mut stop_rx: watch::Receiver<bool>,
    ) {
        debug!("[Monitor] Starting polling loop for session {}", session_id);

        // 采样间隔（秒，至少 1 秒）；磁盘容量和 IP 地址每 N 轮采一次
        let interval_secs = (settings.load_interval_ms / 1000).max(1);
        let slow_every = (settings.disk_interval_ms / settings.load_interval_ms.max(1)).max(1);
        let script = collector::build_script(interval_secs, slow_every);

        const MAX_RETRIES: u32 = 10;
        const INITIAL_BACKOFF_MS: u64 = 1000;
        const MAX_BACKOFF_MS: u64 = 30_000;

        let mut retries: u32 = 0;
        let mut backoff_ms = INITIAL_BACKOFF_MS;

        loop {
            if *stop_rx.borrow() {
                debug!("[Monitor] Stop requested for session {}", session_id);
                break;
            }
            if !session.is_alive() {
                debug!("[Monitor] Session {} disconnected, stopping", session_id);
                break;
            }

            match session.open_exec_stream(&script).await {
                Ok(channel) => {
                    debug!("[Monitor] Stream channel opened for session {}", session_id);
                    match Self::stream_loop(&channel, &data_tx, &session, &mut stop_rx).await {
                        StreamOutcome::Stopped => {
                            debug!("[Monitor] Stream stopped for session {}", session_id);
                            break;
                        }
                        StreamOutcome::Ended { got_data } => {
                            // 成功收到过数据则视为一次健康连接，重置退避
                            if got_data {
                                retries = 0;
                                backoff_ms = INITIAL_BACKOFF_MS;
                            }
                            if !session.is_alive() || *stop_rx.borrow() {
                                break;
                            }
                            warn!(
                                "[Monitor] Stream ended unexpectedly for session {}, reconnecting",
                                session_id
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!("[Monitor] Failed to open stream channel: {}", e);
                }
            }

            retries += 1;
            if retries > MAX_RETRIES {
                let _ = data_tx.send(MonitorEvent::Error(format!(
                    "Monitor stream stopped after {} retries",
                    MAX_RETRIES
                )));
                break;
            }

            // 退避等待，可被停止信号打断
            tokio::select! {
                _ = stop_rx.changed() => {}
                _ = tokio::time::sleep(Duration::from_millis(backoff_ms)) => {}
            }
            if *stop_rx.borrow() {
                break;
            }
            backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
        }

        debug!("[Monitor] Polling loop ended for session {}", session_id);
    }

    /// 单次连接的流式读取循环：按行交给 `Collector`，把换算出的事件派发出去。
    async fn stream_loop(
        channel: &ExecStreamChannel,
        data_tx: &mpsc::UnboundedSender<MonitorEvent>,
        session: &Arc<SshSession>,
        stop_rx: &mut watch::Receiver<bool>,
    ) -> StreamOutcome {
        // 按字节缓存，切出整行后再转成文本：一个汉字可能被拆在两次读取里
        let mut buf: Vec<u8> = Vec::new();
        let mut got_data = false;
        // 每次连接重新开始：远端脚本是新启动的，上一轮的计数器不能沿用
        let mut collector = Collector::new(session.host());

        loop {
            tokio::select! {
                changed = stop_rx.changed() => {
                    if changed.is_err() || *stop_rx.borrow() {
                        return StreamOutcome::Stopped;
                    }
                }
                read = channel.read() => {
                    match read {
                        Ok(Some(bytes)) => {
                            got_data = true;
                            buf.extend_from_slice(&bytes);

                            // 按 \n 切分出完整行，逐行处理；不完整尾段留待下次
                            while let Some(pos) = buf.iter().position(|byte| *byte == b'\n') {
                                let line: Vec<u8> = buf.drain(..=pos).collect();
                                let line = String::from_utf8_lossy(&line);
                                let line = line.trim();
                                if line.is_empty() {
                                    continue;
                                }
                                for event in collector.feed(line) {
                                    log_event(&event);
                                    let _ = data_tx.send(event);
                                }
                            }

                            // 防御：异常情况下行缓冲无限增长则丢弃
                            if buf.len() > 1_000_000 {
                                warn!("[Monitor] Line buffer too large, clearing");
                                buf.clear();
                            }
                        }
                        Ok(None) => return StreamOutcome::Ended { got_data },
                        Err(e) => {
                            debug!("[Monitor] Stream read error: {}", e);
                            return StreamOutcome::Ended { got_data };
                        }
                    }
                }
            }
        }
    }
}

/// 记录监控事件：一次性系统摘要为 DEBUG，每 2s/10s 的周期采样为 TRACE
/// （可通过 `RUST_LOG=shellmaster3::services::monitor=trace` 开启逐轮排查）
fn log_event(event: &MonitorEvent) {
    match event {
        MonitorEvent::SystemInfo(info) => debug!(
            "[Monitor] SYS: hostname={:?} os={:?} cores={}/{} mem_total={}B swap_total={}B",
            info.host.hostname,
            info.host.os,
            info.cpu.cores_physical,
            info.cpu.cores_logical,
            info.memory.total_bytes,
            info.memory.swap_total_bytes,
        ),
        MonitorEvent::LoadInfo(info) => trace!(
            "[Monitor] LOAD: cpu={:?}% breakdown={:?} cores={} load={:?} memory={:?} top_cpu={:?}",
            info.cpu.usage_percent,
            info.cpu.breakdown,
            info.cpu.per_core.len(),
            info.cpu.load_average,
            info.memory,
            info.top_cpu_processes
                .iter()
                .map(|p| format!("{}({})={:.1}%", p.name, p.pid, p.cpu_percent))
                .collect::<Vec<_>>(),
        ),
        MonitorEvent::NetworkInfo(info) => trace!(
            "[Monitor] NET: {} interfaces, tcp established={} total={}",
            info.interfaces.len(),
            info.global.tcp_established,
            info.global.tcp_connections,
        ),
        MonitorEvent::DiskIo(info) => trace!(
            "[Monitor] DIO: {:?}",
            info.devices
                .iter()
                .map(|d| format!(
                    "{} r={:.0}B/s w={:.0}B/s busy={:.0}%",
                    d.name, d.read_bytes_per_sec, d.write_bytes_per_sec, d.busy_percent
                ))
                .collect::<Vec<_>>(),
        ),
        MonitorEvent::DiskInfo(info) => {
            trace!("[Monitor] DISK: {} filesystems", info.disks.len())
        }
        MonitorEvent::Error(_) => {}
    }
}

impl Drop for MonitorService {
    fn drop(&mut self) {
        self.stop();
    }
}
