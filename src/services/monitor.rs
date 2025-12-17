// Monitor 后端服务
// 使用 SSH ExecChannel 轮询执行 Shell 脚本收集系统监控信息

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{debug, info, trace, warn};

use crate::models::monitor::{
    CpuInfo, CpuLoadInfo, DiskDeviceInfo, DiskInfo, HostInfo, LoadInfo, MemoryLoadInfo,
    MemoryTotalInfo, NetworkGlobalInfo, NetworkInfo, NetworkInterfaceInfo, ProcessInfo, SystemInfo,
};
use crate::ssh::session::SshSession;

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
    async fn run_polling_loop(
        session_id: String,
        session: Arc<SshSession>,
        settings: MonitorSettings,
        data_tx: mpsc::UnboundedSender<MonitorEvent>,
        mut stop_rx: watch::Receiver<bool>,
    ) {
        info!("[Monitor] Starting polling loop for session {}", session_id);

        // ========================================================================
        // 初次启动：并行获取所有数据
        // ========================================================================
        {
            let (system_result, load_result, network_result, disk_result) = tokio::join!(
                Self::fetch_system_info(&session, &session_id),
                Self::fetch_load_info(&session),
                Self::fetch_network_info(&session),
                Self::fetch_disk_info(&session)
            );

            // 发送系统信息
            match system_result {
                Ok(info) => {
                    let _ = data_tx.send(MonitorEvent::SystemInfo(info));
                }
                Err(e) => {
                    warn!("[Monitor] Failed to fetch system info: {}", e);
                    let _ = data_tx.send(MonitorEvent::Error(format!(
                        "Failed to fetch system info: {}",
                        e
                    )));
                }
            }

            // 发送负载信息
            match load_result {
                Ok(info) => {
                    let _ = data_tx.send(MonitorEvent::LoadInfo(info));
                }
                Err(e) => {
                    debug!("[Monitor] Failed to fetch initial load info: {}", e);
                }
            }

            // 发送网络信息
            match network_result {
                Ok(info) => {
                    let _ = data_tx.send(MonitorEvent::NetworkInfo(info));
                }
                Err(e) => {
                    debug!("[Monitor] Failed to fetch initial network info: {}", e);
                }
            }

            // 发送磁盘信息
            match disk_result {
                Ok(info) => {
                    let _ = data_tx.send(MonitorEvent::DiskInfo(info));
                }
                Err(e) => {
                    debug!("[Monitor] Failed to fetch initial disk info: {}", e);
                }
            }

            info!(
                "[Monitor] Initial data fetched in parallel for session {}",
                session_id
            );
        }

        // ========================================================================
        // 轮询循环
        // ========================================================================
        let load_interval = Duration::from_millis(settings.load_interval_ms);
        let disk_interval = Duration::from_millis(settings.disk_interval_ms);

        let mut load_ticker = tokio::time::interval(load_interval);
        let mut disk_ticker = tokio::time::interval(disk_interval);

        // 跳过第一个即时触发（因为初始数据已经获取过了）
        load_ticker.tick().await;
        disk_ticker.tick().await;

        loop {
            tokio::select! {
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() {
                        info!("[Monitor] Received stop signal for session {}", session_id);
                        break;
                    }
                }
                _ = load_ticker.tick() => {
                    if !session.is_alive() {
                        info!("[Monitor] Session {} disconnected, stopping", session_id);
                        break;
                    }

                    // 并行获取负载和网络信息
                    let (load_result, network_result) = tokio::join!(
                        Self::fetch_load_info(&session),
                        Self::fetch_network_info(&session)
                    );

                    // 发送负载信息
                    match load_result {
                        Ok(info) => {
                            let _ = data_tx.send(MonitorEvent::LoadInfo(info));
                        }
                        Err(e) => {
                            debug!("[Monitor] Failed to fetch load info: {}", e);
                        }
                    }

                    // 发送网络信息
                    match network_result {
                        Ok(info) => {
                            let _ = data_tx.send(MonitorEvent::NetworkInfo(info));
                        }
                        Err(e) => {
                            debug!("[Monitor] Failed to fetch network info: {}", e);
                        }
                    }
                }
                _ = disk_ticker.tick() => {
                    if !session.is_alive() {
                        break;
                    }

                    // 获取磁盘信息
                    match Self::fetch_disk_info(&session).await {
                        Ok(info) => {
                            let _ = data_tx.send(MonitorEvent::DiskInfo(info));
                        }
                        Err(e) => {
                            debug!("[Monitor] Failed to fetch disk info: {}", e);
                        }
                    }
                }
            }
        }

        info!("[Monitor] Polling loop ended for session {}", session_id);
    }

    /// 获取系统信息
    async fn fetch_system_info(
        session: &Arc<SshSession>,
        session_id: &str,
    ) -> Result<SystemInfo, String> {
        let script = SYSTEM_INFO_SCRIPT;

        let exec = session.open_exec().await.map_err(|e| e.to_string())?;
        let output = exec.exec(script).await.map_err(|e| e.to_string())?;

        if !output.is_success() {
            return Err(format!(
                "Command failed with exit code {}: {}",
                output.exit_code,
                output.stderr_string()
            ));
        }

        let json_str = output.stdout_string();
        trace!("[Monitor] System info raw JSON: {}", json_str);

        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| format!("JSON parse error: {}", e))?;

        // 从 SSH 会话获取主机地址
        let host_address = session.host().to_string();

        Ok(SystemInfo {
            host: HostInfo {
                address: host_address,
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
                uptime_seconds: parsed["host"]["uptime_seconds"].as_u64().unwrap_or(0),
            },
            cpu: CpuInfo {
                model: parsed["cpu"]["model"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
                cores_physical: parsed["cpu"]["cores_physical"].as_u64().unwrap_or(1) as u32,
                cores_logical: parsed["cpu"]["cores_logical"].as_u64().unwrap_or(1) as u32,
                architecture: parsed["cpu"]["architecture"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
            },
            memory: MemoryTotalInfo {
                total_bytes: parsed["memory"]["total_bytes"].as_u64().unwrap_or(0),
                swap_total_bytes: parsed["memory"]["swap_total_bytes"].as_u64().unwrap_or(0),
            },
        })
    }

    /// 获取负载信息
    async fn fetch_load_info(session: &Arc<SshSession>) -> Result<LoadInfo, String> {
        let script = LOAD_INFO_SCRIPT;

        let exec = session.open_exec().await.map_err(|e| e.to_string())?;
        let output = exec.exec(script).await.map_err(|e| e.to_string())?;

        if !output.is_success() {
            return Err(format!(
                "Command failed with exit code {}: {}",
                output.exit_code,
                output.stderr_string()
            ));
        }

        let json_str = output.stdout_string();
        trace!("[Monitor] Load info raw JSON: {}", json_str);

        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| format!("JSON parse error: {}", e))?;

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
                            pid: p["pid"].as_u64().unwrap_or(0) as u32,
                            name: p["name"].as_str().unwrap_or("").to_string(),
                            cpu_percent: p["cpu_percent"].as_f64().unwrap_or(0.0) as f32,
                            memory_percent: p["memory_percent"].as_f64().unwrap_or(0.0) as f32,
                            user: p["user"].as_str().unwrap_or("").to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

        Ok(LoadInfo {
            timestamp: parsed["timestamp"].as_u64().unwrap_or(0),
            cpu: CpuLoadInfo {
                usage_percent: parsed["cpu"]["usage_percent"].as_f64().unwrap_or(0.0) as f32,
                load_average: load_avg,
            },
            memory: MemoryLoadInfo {
                used_bytes: parsed["memory"]["used_bytes"].as_u64().unwrap_or(0),
                available_bytes: parsed["memory"]["available_bytes"].as_u64().unwrap_or(0),
                buffers_bytes: parsed["memory"]["buffers_bytes"].as_u64().unwrap_or(0),
                cached_bytes: parsed["memory"]["cached_bytes"].as_u64().unwrap_or(0),
                swap_used_bytes: parsed["memory"]["swap_used_bytes"].as_u64().unwrap_or(0),
            },
            top_cpu_processes: parse_processes("top_cpu_processes"),
            top_memory_processes: parse_processes("top_memory_processes"),
        })
    }

    /// 获取网络信息
    async fn fetch_network_info(session: &Arc<SshSession>) -> Result<NetworkInfo, String> {
        let script = NETWORK_INFO_SCRIPT;

        let exec = session.open_exec().await.map_err(|e| e.to_string())?;
        let output = exec.exec(script).await.map_err(|e| e.to_string())?;

        if !output.is_success() {
            return Err(format!(
                "Command failed with exit code {}: {}",
                output.exit_code,
                output.stderr_string()
            ));
        }

        let json_str = output.stdout_string();
        trace!("[Monitor] Network info raw JSON: {}", json_str);

        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| format!("JSON parse error: {}", e))?;

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
                        rx_bytes: iface["rx_bytes"].as_u64().unwrap_or(0),
                        tx_bytes: iface["tx_bytes"].as_u64().unwrap_or(0),
                        rx_packets: iface["rx_packets"].as_u64().unwrap_or(0),
                        tx_packets: iface["tx_packets"].as_u64().unwrap_or(0),
                        rx_errors: iface["rx_errors"].as_u64().unwrap_or(0),
                        tx_errors: iface["tx_errors"].as_u64().unwrap_or(0),
                        is_up: iface["is_up"].as_bool().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(NetworkInfo {
            timestamp: parsed["timestamp"].as_u64().unwrap_or(0),
            global: NetworkGlobalInfo {
                tcp_connections: parsed["global"]["tcp_connections"].as_u64().unwrap_or(0) as u32,
                tcp_established: parsed["global"]["tcp_established"].as_u64().unwrap_or(0) as u32,
                tcp_listen: parsed["global"]["tcp_listen"].as_u64().unwrap_or(0) as u32,
                tcp_time_wait: parsed["global"]["tcp_time_wait"].as_u64().unwrap_or(0) as u32,
            },
            interfaces,
        })
    }

    /// 获取磁盘信息
    async fn fetch_disk_info(session: &Arc<SshSession>) -> Result<DiskInfo, String> {
        let script = DISK_INFO_SCRIPT;

        let exec = session.open_exec().await.map_err(|e| e.to_string())?;
        let output = exec.exec(script).await.map_err(|e| e.to_string())?;

        if !output.is_success() {
            return Err(format!(
                "Command failed with exit code {}: {}",
                output.exit_code,
                output.stderr_string()
            ));
        }

        let json_str = output.stdout_string();
        trace!("[Monitor] Disk info raw JSON: {}", json_str);

        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| format!("JSON parse error: {}", e))?;

        let disks = parsed["disks"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|disk| DiskDeviceInfo {
                        device: disk["device"].as_str().unwrap_or("").to_string(),
                        mount_point: disk["mount_point"].as_str().unwrap_or("").to_string(),
                        fs_type: disk["fs_type"].as_str().unwrap_or("").to_string(),
                        total_bytes: disk["total_bytes"].as_u64().unwrap_or(0),
                        used_bytes: disk["used_bytes"].as_u64().unwrap_or(0),
                        available_bytes: disk["available_bytes"].as_u64().unwrap_or(0),
                        usage_percent: disk["usage_percent"].as_f64().unwrap_or(0.0) as f32,
                        inodes_total: disk["inodes_total"].as_u64().unwrap_or(0),
                        inodes_used: disk["inodes_used"].as_u64().unwrap_or(0),
                        inodes_available: disk["inodes_available"].as_u64().unwrap_or(0),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(DiskInfo {
            timestamp: parsed["timestamp"].as_u64().unwrap_or(0),
            disks,
        })
    }
}

impl Drop for MonitorService {
    fn drop(&mut self) {
        self.stop();
    }
}

// ============================================================================
// Shell 脚本
// ============================================================================

/// 系统信息脚本（执行一次）
const SYSTEM_INFO_SCRIPT: &str = r#"
hostname=$(hostname 2>/dev/null || echo "unknown")
# 获取发行版名称和版本号（优先使用 /etc/os-release）
if [ -f /etc/os-release ]; then
    os=$(. /etc/os-release && echo "${PRETTY_NAME:-${NAME} ${VERSION_ID}}" 2>/dev/null || echo "Linux")
else
    os=$(uname -s 2>/dev/null || echo "unknown")
fi
kernel=$(uname -r 2>/dev/null || echo "")
if [ -f /proc/uptime ]; then
    uptime_seconds=$(awk '{print int($1)}' /proc/uptime 2>/dev/null || echo "0")
else
    uptime_seconds=$(sysctl -n kern.boottime 2>/dev/null | awk '{print systime() - $4}' | tr -d ',' || echo "0")
fi

# CPU信息
if command -v lscpu >/dev/null 2>&1; then
    cpu_model=$(lscpu 2>/dev/null | grep "Model name" | cut -d: -f2 | xargs || echo "Unknown")
    cores_physical=$(lscpu 2>/dev/null | grep "Core(s) per socket" | awk '{print $4}' || echo "1")
    sockets=$(lscpu 2>/dev/null | grep "Socket(s)" | awk '{print $2}' || echo "1")
    cores_physical=$((cores_physical * sockets))
    cores_logical=$(nproc 2>/dev/null || echo "1")
else
    cpu_model=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "Unknown")
    cores_physical=$(sysctl -n hw.physicalcpu 2>/dev/null || echo "1")
    cores_logical=$(sysctl -n hw.logicalcpu 2>/dev/null || echo "1")
fi
architecture=$(uname -m 2>/dev/null || echo "unknown")

# 内存信息
if [ -f /proc/meminfo ]; then
    mem_total=$(awk '/MemTotal/{print $2 * 1024}' /proc/meminfo 2>/dev/null || echo "0")
    swap_total=$(awk '/SwapTotal/{print $2 * 1024}' /proc/meminfo 2>/dev/null || echo "0")
else
    mem_total=$(sysctl -n hw.memsize 2>/dev/null || echo "0")
    swap_total=0
fi

cat <<EOF
{
  "host": {
    "hostname": "$hostname",
    "os": "$os",
    "kernel": "$kernel",
    "uptime_seconds": $uptime_seconds
  },
  "cpu": {
    "model": "$cpu_model",
    "cores_physical": $cores_physical,
    "cores_logical": $cores_logical,
    "architecture": "$architecture"
  },
  "memory": {
    "total_bytes": $mem_total,
    "swap_total_bytes": $swap_total
  }
}
EOF
"#;

/// 系统负载脚本（每 2 秒）
const LOAD_INFO_SCRIPT: &str = r#"
timestamp=$(date +%s)

# CPU使用率
if [ -f /proc/stat ]; then
    cpu_line=$(head -1 /proc/stat)
    cpu_user=$(echo $cpu_line | awk '{print $2}')
    cpu_nice=$(echo $cpu_line | awk '{print $3}')
    cpu_system=$(echo $cpu_line | awk '{print $4}')
    cpu_idle=$(echo $cpu_line | awk '{print $5}')
    cpu_total=$((cpu_user + cpu_nice + cpu_system + cpu_idle))
    cpu_usage=$(awk "BEGIN {printf \"%.1f\", ($cpu_total - $cpu_idle) / $cpu_total * 100}")
else
    cpu_usage=$(top -l 1 2>/dev/null | grep "CPU usage" | awk '{print $3}' | tr -d '%' || echo "0")
fi

# 负载
if [ -f /proc/loadavg ]; then
    load_avg=$(awk '{print $1","$2","$3}' /proc/loadavg)
else
    load_avg=$(sysctl -n vm.loadavg 2>/dev/null | awk '{print $2","$3","$4}' || echo "0,0,0")
fi

# 内存详情
if command -v free >/dev/null 2>&1; then
    mem_info=$(free -b 2>/dev/null | grep Mem)
    mem_used=$(echo $mem_info | awk '{print $3}')
    mem_available=$(echo $mem_info | awk '{print $7}')
    if [ -z "$mem_available" ] || [ "$mem_available" = "" ]; then
        mem_available=$(echo $mem_info | awk '{print $4}')
    fi
    mem_buffers=$(awk '/Buffers/{print $2 * 1024}' /proc/meminfo 2>/dev/null || echo "0")
    mem_cached=$(awk '/^Cached:/{print $2 * 1024}' /proc/meminfo 2>/dev/null || echo "0")
    swap_used=$(free -b 2>/dev/null | grep Swap | awk '{print $3}' || echo "0")
else
    mem_used=0
    mem_available=0
    mem_buffers=0
    mem_cached=0
    swap_used=0
fi

# Top 5 CPU进程
top_cpu=$(ps aux --sort=-%cpu 2>/dev/null | head -6 | tail -5 | awk '{gsub(/"/, "\\\"", $11); printf "{\"pid\":%s,\"name\":\"%s\",\"cpu_percent\":%s,\"memory_percent\":%s,\"user\":\"%s\"},", $2, $11, $3, $4, $1}' | sed 's/,$//' || echo "")

# Top 5 内存进程
top_mem=$(ps aux --sort=-%mem 2>/dev/null | head -6 | tail -5 | awk '{gsub(/"/, "\\\"", $11); printf "{\"pid\":%s,\"name\":\"%s\",\"cpu_percent\":%s,\"memory_percent\":%s,\"user\":\"%s\"},", $2, $11, $3, $4, $1}' | sed 's/,$//' || echo "")

cat <<EOF
{
  "timestamp": $timestamp,
  "cpu": {
    "usage_percent": $cpu_usage,
    "load_average": [$load_avg]
  },
  "memory": {
    "used_bytes": $mem_used,
    "available_bytes": $mem_available,
    "buffers_bytes": $mem_buffers,
    "cached_bytes": $mem_cached,
    "swap_used_bytes": $swap_used
  },
  "top_cpu_processes": [$top_cpu],
  "top_memory_processes": [$top_mem]
}
EOF
"#;

/// 网络状态脚本（每 2 秒）
const NETWORK_INFO_SCRIPT: &str = r#"
timestamp=$(date +%s)

# TCP连接统计
if command -v ss >/dev/null 2>&1; then
    tcp_stats=$(ss -t -a 2>/dev/null)
else
    tcp_stats=$(netstat -ant 2>/dev/null)
fi
tcp_total=$(echo "$tcp_stats" | grep -c "tcp" 2>/dev/null | tr -d '\n' || echo "0")
tcp_established=$(echo "$tcp_stats" | grep -c "ESTAB" 2>/dev/null | tr -d '\n' || echo "0")
tcp_listen=$(echo "$tcp_stats" | grep -c "LISTEN" 2>/dev/null | tr -d '\n' || echo "0")
tcp_time_wait=$(echo "$tcp_stats" | grep -c "TIME-WAIT\|TIME_WAIT" 2>/dev/null | tr -d '\n' || echo "0")

# 网卡信息
interfaces=""
if [ -d /sys/class/net ]; then
    for iface in $(ls /sys/class/net 2>/dev/null); do
        # 跳过虚拟接口（但保留 eth*, ens*, enp* 等）
        case "$iface" in
            veth*|docker*|br-*|virbr*) continue ;;
        esac
        
        mac=$(cat /sys/class/net/$iface/address 2>/dev/null || echo "00:00:00:00:00:00")
        rx_bytes=$(cat /sys/class/net/$iface/statistics/rx_bytes 2>/dev/null || echo "0")
        tx_bytes=$(cat /sys/class/net/$iface/statistics/tx_bytes 2>/dev/null || echo "0")
        rx_packets=$(cat /sys/class/net/$iface/statistics/rx_packets 2>/dev/null || echo "0")
        tx_packets=$(cat /sys/class/net/$iface/statistics/tx_packets 2>/dev/null || echo "0")
        rx_errors=$(cat /sys/class/net/$iface/statistics/rx_errors 2>/dev/null || echo "0")
        tx_errors=$(cat /sys/class/net/$iface/statistics/tx_errors 2>/dev/null || echo "0")
        is_up=$(cat /sys/class/net/$iface/operstate 2>/dev/null)
        [ "$is_up" = "up" ] && is_up="true" || is_up="false"
        
        # 获取IP地址
        if command -v ip >/dev/null 2>&1; then
            ips=$(ip -o addr show $iface 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | tr '\n' ',' | sed 's/,$//')
        else
            ips=$(ifconfig $iface 2>/dev/null | grep "inet " | awk '{print $2}' | tr '\n' ',' | sed 's/,$//')
        fi
        ips_json=$(echo "$ips" | awk -F, '{for(i=1;i<=NF;i++) if($i!="") printf "\"%s\"%s", $i, (i<NF?",":"")}')
        
        interfaces="$interfaces{\"name\":\"$iface\",\"mac_address\":\"$mac\",\"ip_addresses\":[$ips_json],\"rx_bytes\":$rx_bytes,\"tx_bytes\":$tx_bytes,\"rx_packets\":$rx_packets,\"tx_packets\":$tx_packets,\"rx_errors\":$rx_errors,\"tx_errors\":$tx_errors,\"is_up\":$is_up},"
    done
fi

# 移除末尾逗号
interfaces=$(echo "$interfaces" | sed 's/,$//')

cat <<EOF
{
  "timestamp": $timestamp,
  "global": {
    "tcp_connections": $tcp_total,
    "tcp_established": $tcp_established,
    "tcp_listen": $tcp_listen,
    "tcp_time_wait": $tcp_time_wait
  },
  "interfaces": [$interfaces]
}
EOF
"#;

/// 磁盘状态脚本（每 10 秒）
const DISK_INFO_SCRIPT: &str = r#"
timestamp=$(date +%s)

disks=""
if command -v df >/dev/null 2>&1; then
    # 使用 df 获取磁盘信息，排除临时文件系统
    df -B1 -T 2>/dev/null | grep -v "^Filesystem\|tmpfs\|devtmpfs\|squashfs\|overlay\|none" | while read line; do
        device=$(echo "$line" | awk '{print $1}')
        fs_type=$(echo "$line" | awk '{print $2}')
        total=$(echo "$line" | awk '{print $3}')
        used=$(echo "$line" | awk '{print $4}')
        available=$(echo "$line" | awk '{print $5}')
        mount_point=$(echo "$line" | awk '{print $7}')
        
        # 跳过无效行
        [ -z "$device" ] && continue
        [ -z "$total" ] && continue
        [ "$total" = "0" ] && continue
        
        # 计算使用率
        if [ "$total" -gt 0 ] 2>/dev/null; then
            usage_percent=$(awk "BEGIN {printf \"%.1f\", $used / $total * 100}")
        else
            usage_percent="0.0"
        fi
        
        # inode信息
        inode_info=$(df -i "$mount_point" 2>/dev/null | tail -1)
        inodes_total=$(echo "$inode_info" | awk '{print $2}' || echo "0")
        inodes_used=$(echo "$inode_info" | awk '{print $3}' || echo "0")
        inodes_available=$(echo "$inode_info" | awk '{print $4}' || echo "0")
        
        # 处理可能的非数字值
        [ -z "$inodes_total" ] && inodes_total=0
        [ -z "$inodes_used" ] && inodes_used=0
        [ -z "$inodes_available" ] && inodes_available=0
        
        echo "{\"device\":\"$device\",\"mount_point\":\"$mount_point\",\"fs_type\":\"$fs_type\",\"total_bytes\":$total,\"used_bytes\":$used,\"available_bytes\":$available,\"usage_percent\":$usage_percent,\"inodes_total\":$inodes_total,\"inodes_used\":$inodes_used,\"inodes_available\":$inodes_available}"
    done > /tmp/disk_info_$$
    
    disks=$(cat /tmp/disk_info_$$ 2>/dev/null | tr '\n' ',' | sed 's/,$//')
    rm -f /tmp/disk_info_$$ 2>/dev/null
fi

cat <<EOF
{
  "timestamp": $timestamp,
  "disks": [$disks]
}
EOF
"#;
