// Monitor 后端服务
// 使用一条常驻 SSH ExecChannel 运行远端循环脚本，持续读取按行输出的 JSON 监控数据。
// 相比「每个指标每个 tick 新开一条通道」的旧方案，彻底消除通道频繁开关与并发竞争，
// 并通过 /proc/stat 增量采样修正 CPU 使用率计算。

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::models::monitor::{
    CpuInfo, CpuLoadInfo, DiskDeviceInfo, DiskInfo, HostInfo, LoadInfo, MemoryLoadInfo,
    MemoryTotalInfo, NetworkGlobalInfo, NetworkInfo, NetworkInterfaceInfo, ProcessInfo, SystemInfo,
};
use crate::ssh::session::{ExecStreamChannel, SshSession};

/// Monitor 事件类型
#[derive(Debug, Clone)]
pub enum MonitorEvent {
    /// 系统信息（静态，连接时获取一次）
    SystemInfo(SystemInfo),
    /// 系统负载信息（每 2 秒）
    LoadInfo(LoadInfo),
    /// 网络状态信息（每 2 秒）
    NetworkInfo(NetworkInfo),
    /// 磁盘状态信息（每 10 秒）
    DiskInfo(DiskInfo),
    /// 错误信息
    Error(String),
}

/// Monitor 服务配置
#[derive(Debug, Clone)]
pub struct MonitorSettings {
    /// 负载信息刷新间隔（毫秒）
    pub load_interval_ms: u64,
    /// 网络信息刷新间隔（毫秒）
    pub network_interval_ms: u64,
    /// 磁盘信息刷新间隔（毫秒）
    pub disk_interval_ms: u64,
}

impl Default for MonitorSettings {
    fn default() -> Self {
        Self {
            load_interval_ms: 2000,    // 2 秒
            network_interval_ms: 2000, // 2 秒
            disk_interval_ms: 10000,   // 10 秒
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
    session: Arc<SshSession>,
    settings: MonitorSettings,
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
            session.clone(),
            settings.clone(),
            data_tx,
            stop_rx,
        ));

        let service = Self {
            session_id,
            session,
            settings,
            stop_tx: Some(stop_tx),
            task_handle: Some(task),
        };

        (service, data_rx)
    }

    /// 停止监控
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(true);
        }
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
        info!("[Monitor] Service stopped for session {}", self.session_id);
    }

    /// 是否正在运行
    pub fn is_running(&self) -> bool {
        self.task_handle
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }

    /// 轮询主循环
    ///
    /// 打开一条常驻流式通道运行采集脚本，持续读取按行输出的 JSON 并转换为 `MonitorEvent`。
    /// 通道意外结束且会话仍存活时，按指数退避自动重连；超过最大重试次数才上报错误并退出。
    async fn run_polling_loop(
        session_id: String,
        session: Arc<SshSession>,
        settings: MonitorSettings,
        data_tx: mpsc::UnboundedSender<MonitorEvent>,
        mut stop_rx: watch::Receiver<bool>,
    ) {
        info!("[Monitor] Starting polling loop for session {}", session_id);

        // 采集间隔（秒，至少 1 秒）与磁盘采集频率（每 N 轮一次）
        let interval_secs = (settings.load_interval_ms / 1000).max(1);
        let disk_every = (settings.disk_interval_ms / settings.load_interval_ms.max(1)).max(1);
        let script = build_stream_script(interval_secs, disk_every);

        const MAX_RETRIES: u32 = 10;
        const INITIAL_BACKOFF_MS: u64 = 1000;
        const MAX_BACKOFF_MS: u64 = 30_000;

        let mut retries: u32 = 0;
        let mut backoff_ms = INITIAL_BACKOFF_MS;

        loop {
            if *stop_rx.borrow() {
                info!("[Monitor] Stop requested for session {}", session_id);
                break;
            }
            if !session.is_alive() {
                info!("[Monitor] Session {} disconnected, stopping", session_id);
                break;
            }

            match session.open_exec_stream(&script).await {
                Ok(channel) => {
                    info!("[Monitor] Stream channel opened for session {}", session_id);
                    match Self::stream_loop(&channel, &data_tx, &session, &mut stop_rx).await {
                        StreamOutcome::Stopped => {
                            info!("[Monitor] Stream stopped for session {}", session_id);
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

        info!("[Monitor] Polling loop ended for session {}", session_id);
    }

    /// 单次连接的流式读取循环：按行解析输出并派发事件。
    async fn stream_loop(
        channel: &ExecStreamChannel,
        data_tx: &mpsc::UnboundedSender<MonitorEvent>,
        session: &Arc<SshSession>,
        stop_rx: &mut watch::Receiver<bool>,
    ) -> StreamOutcome {
        let mut buf = String::new();
        let mut got_data = false;

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
                            buf.push_str(&String::from_utf8_lossy(&bytes));

                            // 按 \n 切分出完整行，逐行派发；不完整尾段留待下次
                            while let Some(pos) = buf.find('\n') {
                                let line: String = buf.drain(..=pos).collect();
                                let line = line.trim();
                                if line.is_empty() {
                                    continue;
                                }
                                Self::dispatch_line(line, data_tx, session);
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

    /// 解析单行输出（`TAG {json}`）并派发为对应事件。
    fn dispatch_line(
        line: &str,
        data_tx: &mpsc::UnboundedSender<MonitorEvent>,
        session: &Arc<SshSession>,
    ) {
        let Some((tag, json)) = line.split_once(' ') else {
            debug!("[Monitor] Skipping line without tag: {:?}", line);
            return;
        };

        let value: serde_json::Value = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => {
                debug!(
                    "[Monitor] Skipping malformed line ({}): {} | raw={:?}",
                    tag, e, json
                );
                return;
            }
        };

        match tag {
            "SYS" => {
                // 系统信息很少出现（每次连接一次），完整打印原始行 + 解析结果，便于排查
                debug!("[Monitor] SYS raw line: {}", line);
                let info = parse_system(&value, session.host());
                debug!(
                    "[Monitor] SYS parsed: hostname={:?} os={:?} cores={}/{} mem_total={}B swap_total={}B (raw memory={})",
                    info.host.hostname,
                    info.host.os,
                    info.cpu.cores_physical,
                    info.cpu.cores_logical,
                    info.memory.total_bytes,
                    info.memory.swap_total_bytes,
                    value["memory"],
                );
                let _ = data_tx.send(MonitorEvent::SystemInfo(info));
            }
            "LOAD" => {
                let info = parse_load(&value);
                debug!(
                    "[Monitor] LOAD parsed: cpu={:.1}% used={}B avail={}B buffers={}B cached={}B swap_used={}B (raw memory={})",
                    info.cpu.usage_percent,
                    info.memory.used_bytes,
                    info.memory.available_bytes,
                    info.memory.buffers_bytes,
                    info.memory.cached_bytes,
                    info.memory.swap_used_bytes,
                    value["memory"],
                );
                let _ = data_tx.send(MonitorEvent::LoadInfo(info));
            }
            "NET" => {
                let info = parse_network(&value);
                debug!(
                    "[Monitor] NET parsed: {} interfaces, tcp_total={}",
                    info.interfaces.len(),
                    info.global.tcp_connections
                );
                let _ = data_tx.send(MonitorEvent::NetworkInfo(info));
            }
            "DISK" => {
                let info = parse_disk(&value);
                debug!("[Monitor] DISK parsed: {} disks", info.disks.len());
                let _ = data_tx.send(MonitorEvent::DiskInfo(info));
            }
            other => {
                debug!("[Monitor] Unknown tag {:?}, raw={:?}", other, line);
            }
        }
    }
}

impl Drop for MonitorService {
    fn drop(&mut self) {
        self.stop();
    }
}

// ============================================================================
// JSON -> 结构体 解析（从原 fetch_* 抽出，供流式行派发复用）
// ============================================================================

/// 把 JSON 数字解析为 u64，并兼容某些 awk（如 Debian/Ubuntu 默认的 mawk）
/// 把大整数输出成浮点/科学计数法（例如 16723714048 输出为 `1.67237e+10`）的情况。
/// `as_u64()` 对带小数点/指数的数字返回 None，这里回退到 `as_f64` 再取整。
fn ju64(v: &serde_json::Value) -> u64 {
    v.as_u64()
        .or_else(|| {
            v.as_f64()
                .filter(|f| f.is_finite() && *f >= 0.0)
                .map(|f| f as u64)
        })
        .unwrap_or(0)
}

/// 解析系统信息（host 地址由 SSH 会话提供）
fn parse_system(parsed: &serde_json::Value, host_address: &str) -> SystemInfo {
    SystemInfo {
        host: HostInfo {
            address: host_address.to_string(),
            hostname: parsed["host"]["hostname"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            os: parsed["host"]["os"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            kernel: parsed["host"]["kernel"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            uptime_seconds: ju64(&parsed["host"]["uptime_seconds"]),
        },
        cpu: CpuInfo {
            model: parsed["cpu"]["model"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            cores_physical: (ju64(&parsed["cpu"]["cores_physical"]).max(1)) as u32,
            cores_logical: (ju64(&parsed["cpu"]["cores_logical"]).max(1)) as u32,
            architecture: parsed["cpu"]["architecture"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
        },
        memory: MemoryTotalInfo {
            total_bytes: ju64(&parsed["memory"]["total_bytes"]),
            swap_total_bytes: ju64(&parsed["memory"]["swap_total_bytes"]),
        },
    }
}

/// 解析负载信息
fn parse_load(parsed: &serde_json::Value) -> LoadInfo {
    let load_avg = parsed["cpu"]["load_average"]
        .as_array()
        .map(|arr| {
            let mut result = [0.0f32; 3];
            for (i, v) in arr.iter().take(3).enumerate() {
                result[i] = v.as_f64().unwrap_or(0.0) as f32;
            }
            result
        })
        .unwrap_or([0.0, 0.0, 0.0]);

    let parse_processes = |key: &str| -> Vec<ProcessInfo> {
        parsed[key]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|p| ProcessInfo {
                        pid: ju64(&p["pid"]) as u32,
                        name: p["name"].as_str().unwrap_or("").to_string(),
                        cpu_percent: p["cpu_percent"].as_f64().unwrap_or(0.0) as f32,
                        memory_percent: p["memory_percent"].as_f64().unwrap_or(0.0) as f32,
                        user: p["user"].as_str().unwrap_or("").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    LoadInfo {
        timestamp: ju64(&parsed["timestamp"]),
        cpu: CpuLoadInfo {
            usage_percent: parsed["cpu"]["usage_percent"].as_f64().unwrap_or(0.0) as f32,
            load_average: load_avg,
        },
        memory: MemoryLoadInfo {
            used_bytes: ju64(&parsed["memory"]["used_bytes"]),
            available_bytes: ju64(&parsed["memory"]["available_bytes"]),
            buffers_bytes: ju64(&parsed["memory"]["buffers_bytes"]),
            cached_bytes: ju64(&parsed["memory"]["cached_bytes"]),
            swap_used_bytes: ju64(&parsed["memory"]["swap_used_bytes"]),
        },
        top_cpu_processes: parse_processes("top_cpu_processes"),
        top_memory_processes: parse_processes("top_memory_processes"),
    }
}

/// 解析网络信息
fn parse_network(parsed: &serde_json::Value) -> NetworkInfo {
    let interfaces = parsed["interfaces"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|iface| NetworkInterfaceInfo {
                    name: iface["name"].as_str().unwrap_or("").to_string(),
                    mac_address: iface["mac_address"].as_str().unwrap_or("").to_string(),
                    ip_addresses: iface["ip_addresses"]
                        .as_array()
                        .map(|ips| {
                            ips.iter()
                                .filter_map(|ip| ip.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default(),
                    rx_bytes: ju64(&iface["rx_bytes"]),
                    tx_bytes: ju64(&iface["tx_bytes"]),
                    rx_packets: ju64(&iface["rx_packets"]),
                    tx_packets: ju64(&iface["tx_packets"]),
                    rx_errors: ju64(&iface["rx_errors"]),
                    tx_errors: ju64(&iface["tx_errors"]),
                    is_up: iface["is_up"].as_bool().unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();

    NetworkInfo {
        timestamp: ju64(&parsed["timestamp"]),
        global: NetworkGlobalInfo {
            tcp_connections: ju64(&parsed["global"]["tcp_connections"]) as u32,
            tcp_established: ju64(&parsed["global"]["tcp_established"]) as u32,
            tcp_listen: ju64(&parsed["global"]["tcp_listen"]) as u32,
            tcp_time_wait: ju64(&parsed["global"]["tcp_time_wait"]) as u32,
        },
        interfaces,
    }
}

/// 解析磁盘信息
fn parse_disk(parsed: &serde_json::Value) -> DiskInfo {
    let disks = parsed["disks"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|disk| DiskDeviceInfo {
                    device: disk["device"].as_str().unwrap_or("").to_string(),
                    mount_point: disk["mount_point"].as_str().unwrap_or("").to_string(),
                    fs_type: disk["fs_type"].as_str().unwrap_or("").to_string(),
                    total_bytes: ju64(&disk["total_bytes"]),
                    used_bytes: ju64(&disk["used_bytes"]),
                    available_bytes: ju64(&disk["available_bytes"]),
                    usage_percent: disk["usage_percent"].as_f64().unwrap_or(0.0) as f32,
                    inodes_total: ju64(&disk["inodes_total"]),
                    inodes_used: ju64(&disk["inodes_used"]),
                    inodes_available: ju64(&disk["inodes_available"]),
                })
                .collect()
        })
        .unwrap_or_default();

    DiskInfo {
        timestamp: ju64(&parsed["timestamp"]),
        disks,
    }
}

// ============================================================================
// 远端采集脚本
// ============================================================================

/// 用采集间隔与磁盘采集频率填充脚本模板
fn build_stream_script(interval_secs: u64, disk_every: u64) -> String {
    STREAM_SCRIPT_TEMPLATE
        .replace("__INTERVAL__", &interval_secs.to_string())
        .replace("__DISK_EVERY__", &disk_every.to_string())
}

/// 常驻采集脚本（单行 JSON，行首带 SYS/LOAD/NET/DISK 标签）
///
/// 设计要点：
/// - 只在启动时探测一次环境，循环内不重复探测；
/// - 每条记录输出为**单行** compact JSON，行首加标签作为帧分隔；
/// - CPU 使用率用 /proc/stat 的**前后两次采样增量**计算（Linux），而非自开机平均；
/// - 通道关闭时远端循环会在下一次 printf 触发 SIGPIPE 自行退出，无残留进程；
/// - Linux 优先精确，macOS/BSD 尽力而为（缺失数据降级为 0）。
const STREAM_SCRIPT_TEMPLATE: &str = r#"
INTERVAL=__INTERVAL__
DISK_EVERY=__DISK_EVERY__

have() { command -v "$1" >/dev/null 2>&1; }

emit_system() {
    hostname=$(hostname 2>/dev/null || echo unknown)
    if [ -f /etc/os-release ]; then
        os=$(. /etc/os-release 2>/dev/null && printf '%s' "${PRETTY_NAME:-${NAME} ${VERSION_ID}}")
        [ -z "$os" ] && os=Linux
    else
        os=$(uname -s 2>/dev/null || echo unknown)
    fi
    kernel=$(uname -r 2>/dev/null || echo "")
    if [ -f /proc/uptime ]; then
        uptime_seconds=$(awk '{print int($1)}' /proc/uptime 2>/dev/null || echo 0)
    else
        uptime_seconds=0
    fi

    if have lscpu; then
        lscpu_out=$(lscpu 2>/dev/null)
        # 只取第一条 "Model name:"（避免同时匹配到 "BIOS Model name:" 等，导致值含换行）
        cpu_model=$(printf '%s\n' "$lscpu_out" | awk -F: '/^Model name:/{sub(/^[ \t]*/,"",$2); print $2; exit}')
        cpsk=$(printf '%s\n' "$lscpu_out" | awk -F: '/^Core\(s\) per socket:/{gsub(/[ \t]/,"",$2); print $2; exit}')
        sockets=$(printf '%s\n' "$lscpu_out" | awk -F: '/^Socket\(s\):/{gsub(/[ \t]/,"",$2); print $2; exit}')
        case "$cpsk" in ''|*[!0-9]*) cpsk=1 ;; esac
        case "$sockets" in ''|*[!0-9]*) sockets=1 ;; esac
        cores_physical=$((cpsk * sockets))
        cores_logical=$(nproc 2>/dev/null || echo 1)
    else
        cpu_model=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo Unknown)
        cores_physical=$(sysctl -n hw.physicalcpu 2>/dev/null || echo 1)
        cores_logical=$(sysctl -n hw.logicalcpu 2>/dev/null || echo 1)
    fi
    architecture=$(uname -m 2>/dev/null || echo unknown)

    if [ -f /proc/meminfo ]; then
        mem_kb=$(awk '/^MemTotal:/{print $2}' /proc/meminfo 2>/dev/null)
        swap_kb=$(awk '/^SwapTotal:/{print $2}' /proc/meminfo 2>/dev/null)
        case "$mem_kb" in ''|*[!0-9]*) mem_kb=0 ;; esac
        case "$swap_kb" in ''|*[!0-9]*) swap_kb=0 ;; esac
        mem_total=$((mem_kb * 1024))
        swap_total=$((swap_kb * 1024))
    else
        mem_total=$(sysctl -n hw.memsize 2>/dev/null || echo 0)
        swap_total=0
    fi

    # 去掉所有自由文本字段中的引号/反斜杠/换行，避免破坏单行 JSON 帧
    hostname=$(printf '%s' "$hostname" | tr -d '"\\\n\r')
    os=$(printf '%s' "$os" | tr -d '"\\\n\r')
    kernel=$(printf '%s' "$kernel" | tr -d '"\\\n\r')
    cpu_model=$(printf '%s' "$cpu_model" | tr -d '"\\\n\r')
    architecture=$(printf '%s' "$architecture" | tr -d '"\\\n\r')
    [ -z "$cpu_model" ] && cpu_model=Unknown
    [ -z "$cores_physical" ] && cores_physical=1
    [ -z "$cores_logical" ] && cores_logical=1
    [ -z "$mem_total" ] && mem_total=0
    [ -z "$swap_total" ] && swap_total=0
    [ -z "$uptime_seconds" ] && uptime_seconds=0

    printf 'SYS {"host":{"hostname":"%s","os":"%s","kernel":"%s","uptime_seconds":%s},"cpu":{"model":"%s","cores_physical":%s,"cores_logical":%s,"architecture":"%s"},"memory":{"total_bytes":%s,"swap_total_bytes":%s}}\n' \
        "$hostname" "$os" "$kernel" "$uptime_seconds" "$cpu_model" "$cores_physical" "$cores_logical" "$architecture" "$mem_total" "$swap_total"
}

prev_total=0
prev_idle=0
read_cpu() {
    if [ -r /proc/stat ]; then
        set -- $(head -n1 /proc/stat 2>/dev/null)
        u=${2:-0}; n=${3:-0}; s=${4:-0}; i=${5:-0}; io=${6:-0}; ir=${7:-0}; si=${8:-0}
        cur_idle=$((i + io))
        cur_total=$((u + n + s + i + io + ir + si))
    else
        cur_idle=0
        cur_total=0
    fi
}

emit_load() {
    timestamp=$(date +%s 2>/dev/null || echo 0)

    read_cpu
    d_total=$((cur_total - prev_total))
    d_idle=$((cur_idle - prev_idle))
    if [ "$d_total" -gt 0 ] 2>/dev/null; then
        cpu_usage=$(awk "BEGIN{u=($d_total-$d_idle)/$d_total*100; if(u<0)u=0; if(u>100)u=100; printf \"%.1f\", u}")
    else
        cpu_usage="0.0"
    fi
    prev_total=$cur_total
    prev_idle=$cur_idle
    [ -z "$cpu_usage" ] && cpu_usage="0.0"

    if [ -r /proc/loadavg ]; then
        load_avg=$(awk '{print $1","$2","$3}' /proc/loadavg 2>/dev/null)
    else
        load_avg=$(sysctl -n vm.loadavg 2>/dev/null | awk '{print $2","$3","$4}')
    fi
    [ -z "$load_avg" ] && load_avg="0,0,0"

    if have free; then
        mem_line=$(free -b 2>/dev/null | awk '/^Mem:/{print}')
        mem_used=$(echo "$mem_line" | awk '{print $3}')
        mem_available=$(echo "$mem_line" | awk '{print $7}')
        [ -z "$mem_available" ] && mem_available=$(echo "$mem_line" | awk '{print $4}')
        swap_used=$(free -b 2>/dev/null | awk '/^Swap:/{print $3}')
    else
        mem_used=0; mem_available=0; swap_used=0
    fi
    buf_kb=$(awk '/^Buffers:/{print $2}' /proc/meminfo 2>/dev/null)
    cache_kb=$(awk '/^Cached:/{print $2}' /proc/meminfo 2>/dev/null)
    case "$buf_kb" in ''|*[!0-9]*) buf_kb=0 ;; esac
    case "$cache_kb" in ''|*[!0-9]*) cache_kb=0 ;; esac
    mem_buffers=$((buf_kb * 1024))
    mem_cached=$((cache_kb * 1024))
    [ -z "$mem_used" ] && mem_used=0
    [ -z "$mem_available" ] && mem_available=0
    [ -z "$swap_used" ] && swap_used=0

    top_cpu=$(ps aux 2>/dev/null | tail -n +2 | sort -k3 -rn | head -n5 | awk '{gsub(/[\\"]/,"",$11); printf "{\"pid\":%s,\"name\":\"%s\",\"cpu_percent\":%s,\"memory_percent\":%s,\"user\":\"%s\"},", $2, $11, $3, $4, $1}' | sed 's/,$//')
    top_mem=$(ps aux 2>/dev/null | tail -n +2 | sort -k4 -rn | head -n5 | awk '{gsub(/[\\"]/,"",$11); printf "{\"pid\":%s,\"name\":\"%s\",\"cpu_percent\":%s,\"memory_percent\":%s,\"user\":\"%s\"},", $2, $11, $3, $4, $1}' | sed 's/,$//')

    printf 'LOAD {"timestamp":%s,"cpu":{"usage_percent":%s,"load_average":[%s]},"memory":{"used_bytes":%s,"available_bytes":%s,"buffers_bytes":%s,"cached_bytes":%s,"swap_used_bytes":%s},"top_cpu_processes":[%s],"top_memory_processes":[%s]}\n' \
        "$timestamp" "$cpu_usage" "$load_avg" "$mem_used" "$mem_available" "$mem_buffers" "$mem_cached" "$swap_used" "$top_cpu" "$top_mem"
}

emit_network() {
    timestamp=$(date +%s 2>/dev/null || echo 0)

    if have ss; then
        tcp_stats=$(ss -t -a 2>/dev/null)
    else
        tcp_stats=$(netstat -ant 2>/dev/null)
    fi
    tcp_total=$(printf '%s\n' "$tcp_stats" | grep -c 'tcp' 2>/dev/null | tr -d '\n')
    tcp_established=$(printf '%s\n' "$tcp_stats" | grep -c 'ESTAB' 2>/dev/null | tr -d '\n')
    tcp_listen=$(printf '%s\n' "$tcp_stats" | grep -c 'LISTEN' 2>/dev/null | tr -d '\n')
    tcp_time_wait=$(printf '%s\n' "$tcp_stats" | grep -c 'TIME-WAIT\|TIME_WAIT' 2>/dev/null | tr -d '\n')
    [ -z "$tcp_total" ] && tcp_total=0
    [ -z "$tcp_established" ] && tcp_established=0
    [ -z "$tcp_listen" ] && tcp_listen=0
    [ -z "$tcp_time_wait" ] && tcp_time_wait=0

    interfaces=""
    if [ -d /sys/class/net ]; then
        for iface in $(ls /sys/class/net 2>/dev/null); do
            case "$iface" in
                veth*|docker*|br-*|virbr*) continue ;;
            esac
            mac=$(cat /sys/class/net/$iface/address 2>/dev/null || echo "00:00:00:00:00:00")
            rx_bytes=$(cat /sys/class/net/$iface/statistics/rx_bytes 2>/dev/null || echo 0)
            tx_bytes=$(cat /sys/class/net/$iface/statistics/tx_bytes 2>/dev/null || echo 0)
            rx_packets=$(cat /sys/class/net/$iface/statistics/rx_packets 2>/dev/null || echo 0)
            tx_packets=$(cat /sys/class/net/$iface/statistics/tx_packets 2>/dev/null || echo 0)
            rx_errors=$(cat /sys/class/net/$iface/statistics/rx_errors 2>/dev/null || echo 0)
            tx_errors=$(cat /sys/class/net/$iface/statistics/tx_errors 2>/dev/null || echo 0)
            is_up=$(cat /sys/class/net/$iface/operstate 2>/dev/null)
            [ "$is_up" = "up" ] && is_up="true" || is_up="false"

            if have ip; then
                ips=$(ip -o addr show $iface 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | tr '\n' ',' | sed 's/,$//')
            else
                ips=$(ifconfig $iface 2>/dev/null | grep 'inet ' | awk '{print $2}' | tr '\n' ',' | sed 's/,$//')
            fi
            ips_json=$(echo "$ips" | awk -F, '{for(i=1;i<=NF;i++) if($i!="") printf "\"%s\"%s", $i, (i<NF?",":"")}')

            interfaces="$interfaces{\"name\":\"$iface\",\"mac_address\":\"$mac\",\"ip_addresses\":[$ips_json],\"rx_bytes\":$rx_bytes,\"tx_bytes\":$tx_bytes,\"rx_packets\":$rx_packets,\"tx_packets\":$tx_packets,\"rx_errors\":$rx_errors,\"tx_errors\":$tx_errors,\"is_up\":$is_up},"
        done
    fi
    interfaces=$(printf '%s' "$interfaces" | sed 's/,$//')

    printf 'NET {"timestamp":%s,"global":{"tcp_connections":%s,"tcp_established":%s,"tcp_listen":%s,"tcp_time_wait":%s},"interfaces":[%s]}\n' \
        "$timestamp" "$tcp_total" "$tcp_established" "$tcp_listen" "$tcp_time_wait" "$interfaces"
}

emit_disk() {
    timestamp=$(date +%s 2>/dev/null || echo 0)

    disks=""
    if have df; then
        disks=$(df -B1 -T 2>/dev/null | grep -vE '^Filesystem|tmpfs|devtmpfs|squashfs|overlay|none' | while read -r device fstype total used available pcent mount rest; do
            [ -z "$device" ] && continue
            [ -z "$total" ] && continue
            [ "$total" = "0" ] && continue
            if [ "$total" -gt 0 ] 2>/dev/null; then
                usage_percent=$(awk "BEGIN{printf \"%.1f\", $used / $total * 100}")
            else
                usage_percent="0.0"
            fi
            inode_info=$(df -i "$mount" 2>/dev/null | tail -1)
            inodes_total=$(echo "$inode_info" | awk '{print $2}')
            inodes_used=$(echo "$inode_info" | awk '{print $3}')
            inodes_available=$(echo "$inode_info" | awk '{print $4}')
            echo "$inodes_total" | grep -qE '^[0-9]+$' || inodes_total=0
            echo "$inodes_used" | grep -qE '^[0-9]+$' || inodes_used=0
            echo "$inodes_available" | grep -qE '^[0-9]+$' || inodes_available=0
            printf '{"device":"%s","mount_point":"%s","fs_type":"%s","total_bytes":%s,"used_bytes":%s,"available_bytes":%s,"usage_percent":%s,"inodes_total":%s,"inodes_used":%s,"inodes_available":%s},' \
                "$device" "$mount" "$fstype" "$total" "$used" "$available" "$usage_percent" "$inodes_total" "$inodes_used" "$inodes_available"
        done)
        disks=$(printf '%s' "$disks" | sed 's/,$//')
    fi

    printf 'DISK {"timestamp":%s,"disks":[%s]}\n' "$timestamp" "$disks"
}

emit_system
read_cpu
prev_total=$cur_total
prev_idle=$cur_idle

count=0
while :; do
    emit_load
    emit_network
    if [ $((count % DISK_EVERY)) -eq 0 ]; then
        emit_disk
    fi
    count=$((count + 1))
    sleep $INTERVAL
done
"#;
