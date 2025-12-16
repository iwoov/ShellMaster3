// Monitor 监控数据模型

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

// ============================================================================
// 系统信息 (静态，连接时获取一次)
// ============================================================================

/// 完整的系统信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemInfo {
    pub host: HostInfo,
    pub cpu: CpuInfo,
    pub memory: MemoryTotalInfo,
}

/// 主机信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostInfo {
    /// 主机地址（来自连接配置）
    pub address: String,
    /// 主机名
    pub hostname: String,
    /// 操作系统信息
    pub os: String,
    /// 运行时间（秒）
    pub uptime_seconds: u64,
}

/// CPU 信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CpuInfo {
    /// CPU 型号
    pub model: String,
    /// 物理核心数
    pub cores_physical: u32,
    /// 逻辑核心数
    pub cores_logical: u32,
    /// 架构 (x86_64, aarch64等)
    pub architecture: String,
}

/// 内存总量信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryTotalInfo {
    /// 总内存（字节）
    pub total_bytes: u64,
    /// Swap 总计（字节）
    pub swap_total_bytes: u64,
}

// ============================================================================
// 系统负载 (动态，每2秒刷新)
// ============================================================================

/// 系统负载信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoadInfo {
    pub timestamp: u64,
    pub cpu: CpuLoadInfo,
    pub memory: MemoryLoadInfo,
    pub top_cpu_processes: Vec<ProcessInfo>,
    pub top_memory_processes: Vec<ProcessInfo>,
}

/// CPU 负载信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CpuLoadInfo {
    /// CPU 使用率百分比
    pub usage_percent: f32,
    /// 1/5/15 分钟负载
    pub load_average: [f32; 3],
}

/// 内存负载信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryLoadInfo {
    /// 已使用内存（字节）
    pub used_bytes: u64,
    /// 可用内存（字节）
    pub available_bytes: u64,
    /// Buffers（字节）
    pub buffers_bytes: u64,
    /// Cached（字节）
    pub cached_bytes: u64,
    /// Swap 已使用（字节）
    pub swap_used_bytes: u64,
}

/// 进程信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProcessInfo {
    /// 进程 ID
    pub pid: u32,
    /// 进程名
    pub name: String,
    /// CPU 占用百分比
    pub cpu_percent: f32,
    /// 内存占用百分比
    pub memory_percent: f32,
    /// 运行用户
    pub user: String,
}

// ============================================================================
// 网络状态 (动态，每2秒刷新)
// ============================================================================

/// 网络状态信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub timestamp: u64,
    pub global: NetworkGlobalInfo,
    pub interfaces: Vec<NetworkInterfaceInfo>,
}

/// 全局网络信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkGlobalInfo {
    /// TCP 总连接数
    pub tcp_connections: u32,
    /// ESTABLISHED 状态
    pub tcp_established: u32,
    /// LISTEN 状态
    pub tcp_listen: u32,
    /// TIME_WAIT 状态
    pub tcp_time_wait: u32,
}

/// 网络接口信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkInterfaceInfo {
    /// 网卡名称
    pub name: String,
    /// MAC 地址
    pub mac_address: String,
    /// IP 地址列表
    pub ip_addresses: Vec<String>,
    /// 接收字节数
    pub rx_bytes: u64,
    /// 发送字节数
    pub tx_bytes: u64,
    /// 接收包数
    pub rx_packets: u64,
    /// 发送包数
    pub tx_packets: u64,
    /// 接收错误数
    pub rx_errors: u64,
    /// 发送错误数
    pub tx_errors: u64,
    /// 是否启用
    pub is_up: bool,
}

// ============================================================================
// 磁盘状态 (低频，每10秒刷新)
// ============================================================================

/// 磁盘状态信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskInfo {
    pub timestamp: u64,
    pub disks: Vec<DiskDeviceInfo>,
}

/// 磁盘设备信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskDeviceInfo {
    /// 设备名
    pub device: String,
    /// 挂载点
    pub mount_point: String,
    /// 文件系统类型
    pub fs_type: String,
    /// 总容量（字节）
    pub total_bytes: u64,
    /// 已使用（字节）
    pub used_bytes: u64,
    /// 可用（字节）
    pub available_bytes: u64,
    /// 使用率百分比
    pub usage_percent: f32,
    /// inode 总数
    pub inodes_total: u64,
    /// inode 已使用
    pub inodes_used: u64,
    /// inode 可用
    pub inodes_available: u64,
}

// ============================================================================
// UI 状态
// ============================================================================

/// 网速快照（用于图表显示）
#[derive(Debug, Clone, Default)]
pub struct NetworkSpeedSnapshot {
    pub timestamp: u64,
    /// 接收速率 (bytes/s)
    pub rx_speed: f64,
    /// 发送速率 (bytes/s)
    pub tx_speed: f64,
}

/// Monitor UI 状态
#[derive(Debug, Clone, Default)]
pub struct MonitorState {
    /// 是否启用监控
    pub enabled: bool,
    /// 静态系统信息
    pub system_info: Option<SystemInfo>,
    /// 负载历史（最近5分钟）
    pub load_history: VecDeque<LoadInfo>,
    /// 网络历史（最近5分钟）
    pub network_history: VecDeque<NetworkInfo>,
    /// 最新磁盘信息
    pub disk_info: Option<DiskInfo>,
    /// 当前选中的网络接口索引
    pub selected_interface_index: usize,
    /// 网速历史（用于图表，最近30秒）
    pub speed_history: VecDeque<NetworkSpeedSnapshot>,
}

impl MonitorState {
    /// 创建带有 mock 数据的状态（用于 UI 开发）
    pub fn with_mock_data() -> Self {
        let mut load_history = VecDeque::new();
        load_history.push_back(LoadInfo {
            timestamp: 0,
            cpu: CpuLoadInfo {
                usage_percent: 45.5,
                load_average: [1.2, 0.8, 0.5],
            },
            memory: MemoryLoadInfo {
                used_bytes: 8 * 1024 * 1024 * 1024,      // 8GB
                available_bytes: 8 * 1024 * 1024 * 1024, // 8GB
                buffers_bytes: 512 * 1024 * 1024,        // 512MB
                cached_bytes: 2 * 1024 * 1024 * 1024,    // 2GB
                swap_used_bytes: 1024 * 1024 * 1024,     // 1GB
            },
            top_cpu_processes: vec![
                ProcessInfo {
                    pid: 1234,
                    name: "cargo".to_string(),
                    cpu_percent: 80.5,
                    memory_percent: 2.1,
                    user: "root".to_string(),
                },
                ProcessInfo {
                    pid: 5678,
                    name: "rust-analyzer".to_string(),
                    cpu_percent: 45.2,
                    memory_percent: 5.5,
                    user: "wuyun".to_string(),
                },
            ],
            top_memory_processes: vec![
                ProcessInfo {
                    pid: 9012,
                    name: "chrome".to_string(),
                    cpu_percent: 5.0,
                    memory_percent: 12.5,
                    user: "wuyun".to_string(),
                },
                ProcessInfo {
                    pid: 3456,
                    name: "vscode".to_string(),
                    cpu_percent: 2.0,
                    memory_percent: 8.5,
                    user: "wuyun".to_string(),
                },
            ],
        });

        let mut network_history = VecDeque::new();
        network_history.push_back(NetworkInfo {
            timestamp: 0,
            global: NetworkGlobalInfo {
                tcp_connections: 125,
                tcp_established: 45,
                tcp_listen: 12,
                tcp_time_wait: 30,
            },
            interfaces: vec![
                NetworkInterfaceInfo {
                    name: "eth0".to_string(),
                    mac_address: "00:11:22:33:44:55".to_string(),
                    ip_addresses: vec!["192.168.1.10".to_string(), "fe80::1".to_string()],
                    rx_bytes: 1024 * 1024 * 100, // 100MB
                    tx_bytes: 1024 * 1024 * 50,  // 50MB
                    rx_packets: 10000,
                    tx_packets: 5000,
                    rx_errors: 0,
                    tx_errors: 0,
                    is_up: true,
                },
                NetworkInterfaceInfo {
                    name: "lo".to_string(),
                    mac_address: "00:00:00:00:00:00".to_string(),
                    ip_addresses: vec!["127.0.0.1".to_string(), "::1".to_string()],
                    rx_bytes: 1024 * 1024,
                    tx_bytes: 1024 * 1024,
                    rx_packets: 100,
                    tx_packets: 100,
                    rx_errors: 0,
                    tx_errors: 0,
                    is_up: true,
                },
            ],
        });

        Self {
            enabled: true,
            system_info: Some(SystemInfo {
                host: HostInfo {
                    address: "110.42.98.184".to_string(),
                    hostname: "mock-server".to_string(),
                    os: "Ubuntu 22.04 LTS".to_string(),
                    uptime_seconds: 3600 * 24 * 5, // 5 days
                },
                cpu: CpuInfo {
                    model: "Intel(R) Xeon(R) Platinum 8269CY CPU @ 2.50GHz".to_string(),
                    cores_physical: 4,
                    cores_logical: 8,
                    architecture: "x86_64".to_string(),
                },
                memory: MemoryTotalInfo {
                    total_bytes: 16 * 1024 * 1024 * 1024,     // 16GB
                    swap_total_bytes: 4 * 1024 * 1024 * 1024, // 4GB
                },
            }),
            load_history,
            network_history,
            disk_info: Some(DiskInfo {
                timestamp: 0,
                disks: vec![DiskDeviceInfo {
                    device: "/dev/sda1".to_string(),
                    mount_point: "/".to_string(),
                    fs_type: "ext4".to_string(),
                    total_bytes: 100 * 1024 * 1024 * 1024, // 100GB
                    used_bytes: 45 * 1024 * 1024 * 1024,   // 45GB
                    available_bytes: 55 * 1024 * 1024 * 1024, // 55GB
                    usage_percent: 45.0,
                    inodes_total: 1000000,
                    inodes_used: 50000,
                    inodes_available: 950000,
                }],
            }),
            selected_interface_index: 0,
            speed_history: VecDeque::new(),
        }
    }

    /// 获取当前负载信息
    pub fn current_load(&self) -> Option<&LoadInfo> {
        self.load_history.back()
    }

    /// 获取当前网络信息
    pub fn current_network(&self) -> Option<&NetworkInfo> {
        self.network_history.back()
    }

    /// 获取当前选中的网络接口
    pub fn selected_interface(&self) -> Option<&NetworkInterfaceInfo> {
        self.current_network()
            .and_then(|n| n.interfaces.get(self.selected_interface_index))
    }

    // ========================================================================
    // 创建和更新方法
    // ========================================================================

    /// 历史记录最大条目数（5分钟，每2秒一条 = 150条）
    const MAX_HISTORY_SIZE: usize = 150;

    /// 创建初始化的空状态（用于真实数据，显示占位符）
    pub fn empty() -> Self {
        let mut state = Self::default();
        state.enabled = true;

        // 初始化空的负载信息，供 UI 显示占位符
        state.load_history.push_back(LoadInfo::default());

        // 初始化空的网络信息
        state.network_history.push_back(NetworkInfo::default());

        // 初始化速度历史，使用默认的0值数据点，让图表直接显示
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        for i in 0..30 {
            state.speed_history.push_back(NetworkSpeedSnapshot {
                timestamp: now - (30 - i) * 2, // 每2秒一个数据点
                rx_speed: 0.0,
                tx_speed: 0.0,
            });
        }

        state
    }

    /// 更新系统信息
    pub fn update_system_info(&mut self, info: SystemInfo) {
        self.system_info = Some(info);
    }

    /// 更新负载信息（并维护历史记录）
    pub fn update_load_info(&mut self, info: LoadInfo) {
        self.load_history.push_back(info);
        // 限制历史记录数量
        while self.load_history.len() > Self::MAX_HISTORY_SIZE {
            self.load_history.pop_front();
        }
    }

    /// 更新网络信息（并维护历史记录和网速计算）
    pub fn update_network_info(&mut self, info: NetworkInfo) {
        // 如果是首次接收真实网络信息（历史只有初始化的空数据）
        let is_first_real_data = self.network_history.len() <= 1
            && self
                .network_history
                .back()
                .map(|n| n.interfaces.is_empty())
                .unwrap_or(true);

        if is_first_real_data {
            // 清空初始化的占位数据
            self.network_history.clear();

            // 自动选择流量最大的 "up" 状态接口
            let best_interface = info
                .interfaces
                .iter()
                .enumerate()
                .filter(|(_, iface)| iface.is_up)
                .max_by_key(|(_, iface)| iface.rx_bytes + iface.tx_bytes);

            if let Some((idx, iface)) = best_interface {
                tracing::info!(
                    "[Monitor] Auto-selected interface: {} (index {})",
                    iface.name,
                    idx
                );
                self.selected_interface_index = idx;
            }
        }

        // 计算网速：与上一条记录比较
        if let Some(prev_info) = self.network_history.back() {
            let time_diff = info.timestamp.saturating_sub(prev_info.timestamp);
            if time_diff > 0 {
                // 获取当前选中接口的流量数据
                let curr_iface = info.interfaces.get(self.selected_interface_index);
                let prev_iface = prev_info.interfaces.get(self.selected_interface_index);

                if let (Some(curr), Some(prev)) = (curr_iface, prev_iface) {
                    let rx_diff = curr.rx_bytes.saturating_sub(prev.rx_bytes);
                    let tx_diff = curr.tx_bytes.saturating_sub(prev.tx_bytes);

                    let rx_speed = rx_diff as f64 / time_diff as f64;
                    let tx_speed = tx_diff as f64 / time_diff as f64;

                    self.speed_history.push_back(NetworkSpeedSnapshot {
                        timestamp: info.timestamp,
                        rx_speed,
                        tx_speed,
                    });

                    // 限制网速历史数量（60秒 / 2秒 = 30条）
                    const MAX_SPEED_HISTORY: usize = 30;
                    while self.speed_history.len() > MAX_SPEED_HISTORY {
                        self.speed_history.pop_front();
                    }
                }
            }
        }

        self.network_history.push_back(info);
        // 限制历史记录数量
        while self.network_history.len() > Self::MAX_HISTORY_SIZE {
            self.network_history.pop_front();
        }
    }

    /// 更新磁盘信息
    pub fn update_disk_info(&mut self, info: DiskInfo) {
        self.disk_info = Some(info);
    }

    /// 获取当前网速 (RX, TX) bytes/s
    pub fn current_speed(&self) -> (f64, f64) {
        self.speed_history
            .back()
            .map(|s| (s.rx_speed, s.tx_speed))
            .unwrap_or((0.0, 0.0))
    }

    /// 格式化网速显示
    pub fn format_speed(bytes_per_sec: f64) -> String {
        if bytes_per_sec >= 1_000_000_000.0 {
            format!("{:.1} GB/s", bytes_per_sec / 1_000_000_000.0)
        } else if bytes_per_sec >= 1_000_000.0 {
            format!("{:.1} MB/s", bytes_per_sec / 1_000_000.0)
        } else if bytes_per_sec >= 1_000.0 {
            format!("{:.1} KB/s", bytes_per_sec / 1_000.0)
        } else {
            format!("{:.0} B/s", bytes_per_sec)
        }
    }
}
