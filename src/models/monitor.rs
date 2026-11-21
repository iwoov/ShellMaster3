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
    /// 内核版本
    pub kernel: String,
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
    /// 采样时刻（毫秒）。Linux 为远端开机后的时间，不受改系统时间影响
    pub timestamp_ms: u64,
    pub cpu: CpuLoadInfo,
    /// 内存用量；远端不是 Linux 时拿不到，为 None
    pub memory: Option<MemoryLoadInfo>,
    pub top_cpu_processes: Vec<ProcessInfo>,
    pub top_memory_processes: Vec<ProcessInfo>,
}

/// CPU 负载信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CpuLoadInfo {
    /// CPU 使用率百分比：两次采样之间非空闲时间的占比（含 steal，iowait 算作空闲）。
    /// 远端不是 Linux 时拿不到，为 None
    pub usage_percent: Option<f32>,
    /// 各类 CPU 时间的占比
    pub breakdown: Option<CpuBreakdown>,
    /// 每个逻辑核的使用率百分比，按核编号排列
    pub per_core: Vec<f32>,
    /// 1/5/15 分钟负载
    pub load_average: [f32; 3],
}

/// 两次采样之间各类 CPU 时间的占比（百分比，合计约为 100）
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct CpuBreakdown {
    /// 用户态（含 nice）
    pub user: f32,
    /// 内核态
    pub system: f32,
    /// 等待磁盘 IO
    pub iowait: f32,
    /// 硬中断和软中断
    pub irq: f32,
    /// 被宿主机占用（虚拟机）
    pub steal: f32,
    /// 空闲
    pub idle: f32,
}

/// 内存负载信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryLoadInfo {
    /// 已使用内存（字节）：总量减去可用（MemAvailable），可回收的缓存不算在内
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
    /// CPU 占用百分比：两次采样之间用掉的 CPU 时间，一个核占满为 100（多线程进程可超过 100）
    pub cpu_percent: f32,
    /// 内存占用百分比（RSS / 内存总量）
    pub memory_percent: f32,
    /// 常驻内存 RSS（字节）
    pub memory_bytes: u64,
    /// 运行用户
    pub user: String,
}

// ============================================================================
// 网络状态 (动态，每2秒刷新)
// ============================================================================

/// 网络状态信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkInfo {
    /// 采样时刻（毫秒），同 [`LoadInfo::timestamp_ms`]
    pub timestamp_ms: u64,
    pub global: NetworkGlobalInfo,
    pub interfaces: Vec<NetworkInterfaceInfo>,
}

/// 全局网络信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkGlobalInfo {
    /// TCP 连接总数（各种状态，含 IPv6 和 TIME_WAIT）
    pub tcp_connections: u32,
    /// ESTABLISHED 和 CLOSE-WAIT 状态的连接数
    pub tcp_established: u32,
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
    /// 采样时刻（毫秒），同 [`LoadInfo::timestamp_ms`]
    pub timestamp_ms: u64,
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
    /// 使用率百分比，与 df 一致：已用 /（已用 + 可用），不计为 root 保留的空间
    pub usage_percent: f32,
    /// inode 总数
    pub inodes_total: u64,
    /// inode 已使用
    pub inodes_used: u64,
    /// inode 可用
    pub inodes_available: u64,
}

// ============================================================================
// 磁盘读写 (动态，每2秒刷新)
// ============================================================================

/// 两次采样之间各块磁盘的读写情况
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskIoInfo {
    /// 采样时刻（毫秒），同 [`LoadInfo::timestamp_ms`]
    pub timestamp_ms: u64,
    /// 整块磁盘（不含分区），按 /proc/diskstats 的顺序；dm-*、md* 是建在物理磁盘上的逻辑设备
    pub devices: Vec<DiskIoDevice>,
}

/// 一块磁盘的读写速率
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DiskIoDevice {
    /// 设备名（sda、nvme0n1、dm-0……）
    pub name: String,
    /// 读取速率（字节/秒）
    pub read_bytes_per_sec: f64,
    /// 写入速率（字节/秒）
    pub write_bytes_per_sec: f64,
    /// 每秒完成的读请求数
    pub reads_per_sec: f64,
    /// 每秒完成的写请求数
    pub writes_per_sec: f64,
    /// 繁忙度：有 IO 在进行的时间占比（百分比）
    pub busy_percent: f32,
    /// 平均每个请求的耗时（毫秒），没有请求时为 0
    pub await_ms: f32,
}

impl DiskIoDevice {
    /// 建在其他磁盘之上的设备：dm-*（LVM、加密盘）和 md*（软 RAID）。
    /// 它们的读写最终落在底下的物理磁盘上，合计时再加一遍就重复了
    fn is_stacked(&self) -> bool {
        self.name.starts_with("dm-") || self.name.starts_with("md")
    }
}

impl DiskIoInfo {
    /// 计入合计的设备：物理磁盘；只有 dm、md 这类设备时（看不到底下的物理盘）就用它们
    fn counted(&self) -> impl Iterator<Item = &DiskIoDevice> {
        let has_physical = self.devices.iter().any(|device| !device.is_stacked());
        self.devices
            .iter()
            .filter(move |device| !has_physical || !device.is_stacked())
    }

    /// 各块磁盘的读取、写入速率合计（字节/秒）
    pub fn total_rates(&self) -> (f64, f64) {
        self.counted().fold((0.0, 0.0), |(read, write), device| {
            (
                read + device.read_bytes_per_sec,
                write + device.write_bytes_per_sec,
            )
        })
    }

    /// 最忙的那块磁盘（繁忙度最高）；没有磁盘时为 None
    pub fn busiest(&self) -> Option<&DiskIoDevice> {
        self.counted()
            .max_by(|a, b| a.busy_percent.total_cmp(&b.busy_percent))
    }
}

// ============================================================================
// UI 状态
// ============================================================================

/// 网速快照（用于图表显示）
#[derive(Debug, Clone, Default)]
pub struct NetworkSpeedSnapshot {
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
    /// 收到系统信息的时刻：运行时间按「收到的值 + 之后经过的时间」显示，不会停住
    pub system_info_received_at: Option<std::time::Instant>,
    /// 负载历史（最近1分钟）；收到第一条数据之前为空
    pub load_history: VecDeque<LoadInfo>,
    /// 网络历史（最近1分钟）；收到第一条数据之前为空
    pub network_history: VecDeque<NetworkInfo>,
    /// 最新磁盘信息
    pub disk_info: Option<DiskInfo>,
    /// 磁盘读写历史（最近1分钟）；收到第二轮采样之前为空
    pub disk_io_history: VecDeque<DiskIoInfo>,
    /// 当前选中的网络接口索引
    pub selected_interface_index: usize,
    /// 网速历史（用于图表，最近30秒）
    pub speed_history: VecDeque<NetworkSpeedSnapshot>,
}

impl MonitorState {
    /// 获取当前负载信息
    pub fn current_load(&self) -> Option<&LoadInfo> {
        self.load_history.back()
    }

    /// 获取当前网络信息
    pub fn current_network(&self) -> Option<&NetworkInfo> {
        self.network_history.back()
    }

    /// 是否已经收到过任何监控数据
    pub fn has_any_data(&self) -> bool {
        self.system_info.is_some()
            || !self.load_history.is_empty()
            || !self.network_history.is_empty()
            || self.disk_info.is_some()
    }

    /// 当前的运行时间（秒）：收到时的值加上之后经过的时间
    pub fn current_uptime(&self) -> Option<u64> {
        self.uptime_at(std::time::Instant::now())
    }

    fn uptime_at(&self, now: std::time::Instant) -> Option<u64> {
        let info = self.system_info.as_ref()?;
        let elapsed = self
            .system_info_received_at
            .map(|received| now.saturating_duration_since(received).as_secs())
            .unwrap_or(0);
        Some(info.host.uptime_seconds.saturating_add(elapsed))
    }

    /// 最近一分钟的 CPU 使用率（百分比），按时间先后排列；拿不到使用率的采样跳过
    pub fn cpu_history(&self) -> Vec<f32> {
        self.load_history
            .iter()
            .filter_map(|load| load.cpu.usage_percent)
            .collect()
    }

    /// 最新一次的磁盘读写
    pub fn current_disk_io(&self) -> Option<&DiskIoInfo> {
        self.disk_io_history.back()
    }

    /// 最近一分钟各次采样的读取、写入速率合计（字节/秒），按时间先后排列
    pub fn disk_io_history_totals(&self) -> (Vec<f32>, Vec<f32>) {
        self.disk_io_history
            .iter()
            .map(|io| {
                let (read, write) = io.total_rates();
                (read as f32, write as f32)
            })
            .unzip()
    }

    // ========================================================================
    // 创建和更新方法
    // ========================================================================

    /// 历史记录最大条目数（1分钟，每2秒一条 = 30条）
    pub const MAX_HISTORY_SIZE: usize = 30;

    /// 创建初始化的空状态：在收到真实数据之前界面显示「正在获取」
    pub fn empty() -> Self {
        Self {
            enabled: true,
            ..Self::default()
        }
    }

    /// 更新系统信息
    pub fn update_system_info(&mut self, info: SystemInfo) {
        self.system_info = Some(info);
        self.system_info_received_at = Some(std::time::Instant::now());
    }

    /// 更新负载信息（并维护历史记录）
    pub fn update_load_info(&mut self, info: LoadInfo) {
        self.load_history.push_back(info);
        // 限制历史记录数量
        while self.load_history.len() > Self::MAX_HISTORY_SIZE {
            self.load_history.pop_front();
        }
    }

    /// 更新磁盘读写信息（并维护历史记录）
    pub fn update_disk_io(&mut self, info: DiskIoInfo) {
        self.disk_io_history.push_back(info);
        while self.disk_io_history.len() > Self::MAX_HISTORY_SIZE {
            self.disk_io_history.pop_front();
        }
    }

    /// 更新网络信息（并维护历史记录和网速计算）
    pub fn update_network_info(&mut self, info: NetworkInfo) {
        if self.network_history.is_empty() {
            // 首次收到网络信息：自动选择流量最大的 "up" 状态接口
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
            if let Some(speed) = speed_between(prev_info, &info, self.selected_interface_index) {
                self.speed_history.push_back(speed);
                while self.speed_history.len() > Self::MAX_HISTORY_SIZE {
                    self.speed_history.pop_front();
                }
            }
        }

        self.network_history.push_back(info);
        // 限制历史记录数量
        while self.network_history.len() > Self::MAX_HISTORY_SIZE {
            self.network_history.pop_front();
        }
    }

    /// 切换网卡：按已有的网络历史重新计算这块网卡的网速，曲线不用从零开始
    pub fn select_interface(&mut self, index: usize) {
        self.selected_interface_index = index;
        self.speed_history = self
            .network_history
            .iter()
            .zip(self.network_history.iter().skip(1))
            .filter_map(|(prev, curr)| speed_between(prev, curr, index))
            .collect();
        while self.speed_history.len() > Self::MAX_HISTORY_SIZE {
            self.speed_history.pop_front();
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
}

/// 两次网络采样之间，`curr` 中第 `index` 块网卡的网速。
/// 按网卡名称在上一次采样里找对应的网卡（网卡列表的顺序可能变化）。
fn speed_between(
    prev: &NetworkInfo,
    curr: &NetworkInfo,
    index: usize,
) -> Option<NetworkSpeedSnapshot> {
    let elapsed_ms = curr.timestamp_ms.saturating_sub(prev.timestamp_ms);
    if elapsed_ms == 0 {
        return None;
    }
    let elapsed_secs = elapsed_ms as f64 / 1000.0;
    let current = curr.interfaces.get(index)?;
    let previous = prev
        .interfaces
        .iter()
        .find(|iface| iface.name == current.name)?;
    Some(NetworkSpeedSnapshot {
        rx_speed: current.rx_bytes.saturating_sub(previous.rx_bytes) as f64 / elapsed_secs,
        tx_speed: current.tx_bytes.saturating_sub(previous.tx_bytes) as f64 / elapsed_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn iface(name: &str, rx: u64, tx: u64) -> NetworkInterfaceInfo {
        NetworkInterfaceInfo {
            name: name.to_string(),
            rx_bytes: rx,
            tx_bytes: tx,
            is_up: true,
            ..Default::default()
        }
    }

    fn sample(secs: u64, interfaces: Vec<NetworkInterfaceInfo>) -> NetworkInfo {
        NetworkInfo {
            timestamp_ms: secs * 1000,
            interfaces,
            ..Default::default()
        }
    }

    #[test]
    fn empty_state_has_no_placeholder_data() {
        let state = MonitorState::empty();
        assert!(state.enabled);
        assert!(state.current_load().is_none());
        assert!(state.current_network().is_none());
        assert!(state.speed_history.is_empty());
        assert!(!state.has_any_data());
    }

    #[test]
    fn uptime_keeps_counting_after_it_was_received() {
        let mut state = MonitorState::empty();
        assert_eq!(state.current_uptime(), None);
        let mut info = SystemInfo::default();
        info.host.uptime_seconds = 100;
        state.update_system_info(info);
        let received = state.system_info_received_at.unwrap();
        assert_eq!(state.uptime_at(received), Some(100));
        assert_eq!(
            state.uptime_at(received + Duration::from_secs(61)),
            Some(161)
        );
        // 时钟回退时不会变小
        if let Some(earlier) = received.checked_sub(Duration::from_secs(5)) {
            assert_eq!(state.uptime_at(earlier), Some(100));
        }
    }

    #[test]
    fn first_sample_picks_the_busiest_interface_and_later_samples_add_speeds() {
        let mut state = MonitorState::empty();
        state.update_network_info(sample(
            10,
            vec![iface("lo", 10, 10), iface("eth0", 500, 100)],
        ));
        assert_eq!(state.selected_interface_index, 1);
        assert!(state.speed_history.is_empty());

        state.update_network_info(sample(
            12,
            vec![iface("lo", 10, 10), iface("eth0", 2500, 300)],
        ));
        assert_eq!(state.current_speed(), (1000.0, 100.0));
    }

    #[test]
    fn switching_interfaces_rebuilds_the_speed_history() {
        let mut state = MonitorState::empty();
        state.update_network_info(sample(10, vec![iface("eth0", 0, 0), iface("eth1", 0, 0)]));
        state.update_network_info(sample(
            12,
            vec![iface("eth0", 200, 0), iface("eth1", 40, 20)],
        ));
        state.update_network_info(sample(
            14,
            vec![iface("eth0", 400, 0), iface("eth1", 80, 60)],
        ));

        state.select_interface(1);
        let speeds: Vec<_> = state
            .speed_history
            .iter()
            .map(|s| (s.rx_speed, s.tx_speed))
            .collect();
        assert_eq!(speeds, vec![(20.0, 10.0), (20.0, 20.0)]);
    }

    #[test]
    fn speeds_match_interfaces_by_name_and_skip_bad_samples() {
        let prev = sample(10, vec![iface("eth0", 100, 0), iface("eth1", 0, 0)]);
        // 网卡顺序变了：按名称对应
        let curr = sample(12, vec![iface("eth1", 50, 0), iface("eth0", 300, 0)]);
        let speed = speed_between(&prev, &curr, 1).unwrap();
        assert_eq!(speed.rx_speed, 100.0);
        // 时间戳没变、网卡不存在时不计算
        assert!(speed_between(&prev, &sample(10, curr.interfaces.clone()), 0).is_none());
        assert!(speed_between(&prev, &curr, 5).is_none());
    }

    #[test]
    fn speeds_use_the_exact_interval_between_samples() {
        // 两次采样实际隔了 2.5 秒：按毫秒算，不会因为取整成 2 秒或 3 秒而偏差 20% 以上
        let prev = NetworkInfo {
            timestamp_ms: 10_000,
            interfaces: vec![iface("eth0", 0, 0)],
            ..Default::default()
        };
        let curr = NetworkInfo {
            timestamp_ms: 12_500,
            interfaces: vec![iface("eth0", 5_000, 2_500)],
            ..Default::default()
        };
        let speed = speed_between(&prev, &curr, 0).unwrap();
        assert_eq!((speed.rx_speed, speed.tx_speed), (2_000.0, 1_000.0));
    }

    #[test]
    fn cpu_history_skips_samples_without_usage() {
        let mut state = MonitorState::empty();
        for usage in [Some(10.0), None, Some(30.0)] {
            let mut load = LoadInfo::default();
            load.cpu.usage_percent = usage;
            state.update_load_info(load);
        }
        assert_eq!(state.cpu_history(), vec![10.0, 30.0]);
    }

    #[test]
    fn disk_io_history_keeps_the_last_minute() {
        let mut state = MonitorState::empty();
        for i in 0..(MonitorState::MAX_HISTORY_SIZE as u64 + 5) {
            state.update_disk_io(DiskIoInfo {
                timestamp_ms: i,
                devices: Vec::new(),
            });
        }
        assert_eq!(state.disk_io_history.len(), MonitorState::MAX_HISTORY_SIZE);
        assert_eq!(state.disk_io_history.front().unwrap().timestamp_ms, 5);
    }

    fn disk(name: &str, read: f64, write: f64, busy: f32) -> DiskIoDevice {
        DiskIoDevice {
            name: name.to_string(),
            read_bytes_per_sec: read,
            write_bytes_per_sec: write,
            busy_percent: busy,
            ..Default::default()
        }
    }

    #[test]
    fn disk_totals_skip_volumes_stacked_on_physical_disks() {
        // LVM（dm-0）和软 RAID（md0）的读写已经算在 sda、sdb 上
        let io = DiskIoInfo {
            timestamp_ms: 0,
            devices: vec![
                disk("sda", 100.0, 10.0, 20.0),
                disk("sdb", 50.0, 5.0, 60.0),
                disk("dm-0", 100.0, 10.0, 90.0),
                disk("md0", 50.0, 5.0, 95.0),
            ],
        };
        assert_eq!(io.total_rates(), (150.0, 15.0));
        assert_eq!(io.busiest().unwrap().name, "sdb");
    }

    #[test]
    fn disk_totals_fall_back_to_stacked_devices_when_alone() {
        let io = DiskIoInfo {
            timestamp_ms: 0,
            devices: vec![disk("dm-0", 30.0, 3.0, 5.0), disk("dm-1", 20.0, 2.0, 7.0)],
        };
        assert_eq!(io.total_rates(), (50.0, 5.0));
        assert_eq!(io.busiest().unwrap().name, "dm-1");

        let empty = DiskIoInfo::default();
        assert_eq!(empty.total_rates(), (0.0, 0.0));
        assert!(empty.busiest().is_none());
    }

    #[test]
    fn disk_io_history_totals_follow_the_samples() {
        let mut state = MonitorState::empty();
        assert!(state.current_disk_io().is_none());
        for read in [1.0, 2.0] {
            state.update_disk_io(DiskIoInfo {
                timestamp_ms: 0,
                devices: vec![
                    disk("nvme0n1", read, read / 2.0, 1.0),
                    disk("sda", read, 0.0, 1.0),
                ],
            });
        }
        assert_eq!(
            state.disk_io_history_totals(),
            (vec![2.0, 4.0], vec![0.5, 1.0])
        );
        assert_eq!(
            state.current_disk_io().unwrap().devices[0].read_bytes_per_sec,
            2.0
        );
    }
}
