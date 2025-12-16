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
}

impl MonitorState {
    /// 创建带有 mock 数据的状态（用于 UI 开发）
    pub fn with_mock_data() -> Self {
        Self {
            enabled: true,
            system_info: Some(SystemInfo {
                host: HostInfo {
                    address: "110.42.98.184".to_string(),
                    hostname: "my-server".to_string(),
                    os: "获取中...".to_string(),
                    uptime_seconds: 0,
                },
                cpu: CpuInfo::default(),
                memory: MemoryTotalInfo::default(),
            }),
            load_history: VecDeque::new(),
            network_history: VecDeque::new(),
            disk_info: Some(DiskInfo {
                timestamp: 0,
                disks: vec![DiskDeviceInfo {
                    device: "/dev/sda1".to_string(),
                    mount_point: "/".to_string(),
                    fs_type: "ext4".to_string(),
                    total_bytes: 107374182400,
                    used_bytes: 53687091200,
                    available_bytes: 48318382080,
                    usage_percent: 52.6,
                    ..Default::default()
                }],
            }),
            selected_interface_index: 0,
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
}
