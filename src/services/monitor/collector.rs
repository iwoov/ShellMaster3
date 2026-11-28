// 解析远端采集脚本（collector.sh）的输出，把原始计数器换算成监控数据。
//
// 脚本每轮输出 `TICK` … `END` 一组行，计数器一律原样输出；这里记住上一轮的计数器，
// 用两轮之差算出 CPU 使用率、每核使用率、进程 CPU 占用和磁盘读写速率。
// 磁盘容量、IP 地址每隔几轮才输出一次，IP 地址在两次之间沿用上一次的结果。

use std::collections::HashMap;

use tracing::debug;

use super::MonitorEvent;
use crate::models::monitor::{
    CpuBreakdown, CpuInfo, CpuLoadInfo, DiskDeviceInfo, DiskInfo, DiskIoDevice, DiskIoInfo,
    HostInfo, LoadInfo, MemoryLoadInfo, MemoryTotalInfo, NetworkGlobalInfo, NetworkInfo,
    NetworkInterfaceInfo, ProcessInfo, SystemInfo,
};

/// 远端采集脚本
const SCRIPT: &str = include_str!("collector.sh");

/// /proc/diskstats 按 512 字节的扇区计数，与磁盘实际的扇区大小无关
const SECTOR_BYTES: f64 = 512.0;

/// 生成发给远端的脚本：采样参数写在脚本前面
pub(super) fn build_script(interval_secs: u64, slow_every: u64) -> String {
    format!(
        "INTERVAL={}\nSLOW_EVERY={}\n{}",
        interval_secs.max(1),
        slow_every.max(1),
        SCRIPT
    )
}

/// 按行接收脚本输出：`SYS` 行立即换算，其余攒到一轮的 `END` 再一起换算
pub(super) struct Collector {
    /// 主机地址（来自连接配置），填进系统信息
    host: String,
    /// 正在接收的一轮
    tick: Option<Tick>,
    /// 上一轮的计数器（只有 Linux 有）
    prev: Option<Counters>,
    /// 各网卡的 IP 地址：每隔几轮更新一次，中间沿用
    addresses: HashMap<String, Vec<String>>,
}

impl Collector {
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            tick: None,
            prev: None,
            addresses: HashMap::new(),
        }
    }

    /// 处理一行输出，返回由此产生的事件。一轮采样在 `END` 行才产生事件，
    /// 没收到 `END` 就开始了下一轮（输出被截断）时，丢弃不完整的那一轮
    pub fn feed(&mut self, line: &str) -> Vec<MonitorEvent> {
        let (tag, rest) = line.split_once(' ').unwrap_or((line, ""));
        match tag {
            "SYS" => match serde_json::from_str::<serde_json::Value>(rest) {
                Ok(value) => vec![MonitorEvent::SystemInfo(parse_system(&value, &self.host))],
                Err(error) => {
                    debug!("[Monitor] Skipping malformed SYS line ({error}): {rest:?}");
                    Vec::new()
                }
            },
            "TICK" => {
                self.tick = Some(Tick {
                    timestamp_ms: parse_timestamp(rest),
                    ..Tick::default()
                });
                Vec::new()
            }
            "END" => match self.tick.take() {
                Some(tick) => self.finish(tick),
                None => Vec::new(),
            },
            _ => {
                if let Some(tick) = self.tick.as_mut() {
                    tick.add(tag, rest);
                }
                Vec::new()
            }
        }
    }

    /// 一轮结束：和上一轮比较，换算成事件
    fn finish(&mut self, tick: Tick) -> Vec<MonitorEvent> {
        if let Some(addresses) = tick.addresses() {
            self.addresses = addresses;
        }
        let prev = self.prev.take();

        let mut events = Vec::new();
        if let Some(load) = load_info(&tick, prev.as_ref()) {
            events.push(MonitorEvent::LoadInfo(load));
        }
        if let Some(network) = self.network_info(&tick) {
            events.push(MonitorEvent::NetworkInfo(network));
        }
        if let Some(io) = prev
            .as_ref()
            .and_then(|prev| disk_io(prev, tick.timestamp_ms, &tick.disks))
        {
            events.push(MonitorEvent::DiskIo(io));
        }
        if let Some(disk) = disk_info(&tick) {
            events.push(MonitorEvent::DiskInfo(disk));
        }

        self.prev = if tick.cpus.is_empty() {
            prev
        } else {
            Some(Counters {
                timestamp_ms: tick.timestamp_ms,
                cpus: tick.cpus,
                disks: tick.disks,
            })
        };
        events
    }

    fn network_info(&self, tick: &Tick) -> Option<NetworkInfo> {
        let snmp = snmp_tcp(&tick.snmp);
        if tick.net.is_empty()
            && snmp.is_empty()
            && tick.sockstat.is_empty()
            && tick.tcp_established.is_none()
        {
            return None;
        }

        let interfaces = tick
            .net
            .iter()
            .map(|counters| {
                let (is_up, mac_address) = tick
                    .iface_state
                    .get(&counters.name)
                    .cloned()
                    .unwrap_or_default();
                NetworkInterfaceInfo {
                    name: counters.name.clone(),
                    mac_address,
                    ip_addresses: self
                        .addresses
                        .get(&counters.name)
                        .cloned()
                        .unwrap_or_default(),
                    rx_bytes: counters.rx_bytes,
                    tx_bytes: counters.tx_bytes,
                    rx_packets: counters.rx_packets,
                    tx_packets: counters.tx_packets,
                    rx_errors: counters.rx_errors,
                    tx_errors: counters.tx_errors,
                    is_up,
                }
            })
            .collect();

        let sock = |key: &str| tick.sockstat.get(key).copied().unwrap_or(0);
        let established = snmp
            .get("CurrEstab")
            .map(|count| to_u32(*count))
            .or(tick.tcp_established)
            .unwrap_or(0);
        Some(NetworkInfo {
            timestamp_ms: tick.timestamp_ms,
            global: NetworkGlobalInfo {
                tcp_connections: to_u32(sock("TCP inuse") + sock("TCP6 inuse") + sock("TCP tw")),
                tcp_established: established,
                tcp_time_wait: to_u32(sock("TCP tw")),
            },
            interfaces,
        })
    }
}

// ============================================================================
// 一轮采样的原始数据
// ============================================================================

/// 一轮采样收到的原始数据
#[derive(Default)]
struct Tick {
    timestamp_ms: u64,
    /// /proc/stat：`cpu`（合计）和 `cpu0`、`cpu1`……
    cpus: Vec<(String, CpuTimes)>,
    load_average: Option<[f32; 3]>,
    /// /proc/meminfo，单位 kB
    meminfo: HashMap<String, u64>,
    net: Vec<NetCounters>,
    /// 网卡名 -> (是否 up, MAC)
    iface_state: HashMap<String, (bool, String)>,
    /// /proc/net/snmp 的 Tcp 行（字段名一行、数值一行）
    snmp: Vec<Vec<String>>,
    /// /proc/net/sockstat：`TCP inuse`、`TCP tw`、`TCP6 inuse` 等
    sockstat: HashMap<String, u64>,
    disks: Vec<(String, DiskCounters)>,
    top_cpu: Vec<RawProcess>,
    top_memory: Vec<RawProcess>,
    /// 非 Linux：ps 直接给出的进程占用
    ps_cpu: Vec<ProcessInfo>,
    ps_memory: Vec<ProcessInfo>,
    /// 非 Linux：netstat 数出的 ESTABLISHED 连接数
    tcp_established: Option<u32>,
    /// `ip -o addr show` 或 `ifconfig -a` 的输出（每隔几轮才有）
    ip_lines: Vec<String>,
    ifconfig_lines: Vec<String>,
    /// df（每隔几轮才有）
    df: Vec<DfRow>,
    /// 挂载点 -> (inode 总数, 已用, 可用)
    inodes: HashMap<String, (u64, u64, u64)>,
}

impl Tick {
    fn add(&mut self, tag: &str, rest: &str) {
        match tag {
            "CPU" => self.cpus.extend(CpuTimes::parse(rest)),
            "LOADAVG" => self.load_average = parse_load_average(rest),
            "MEM" => {
                if let Some((key, kb)) = parse_meminfo(rest) {
                    self.meminfo.insert(key, kb);
                }
            }
            "NET" => self.net.extend(NetCounters::parse(rest)),
            "IFACE" => {
                let mut parts = rest.split_whitespace();
                if let (Some(name), Some(state), Some(mac)) =
                    (parts.next(), parts.next(), parts.next())
                {
                    let mac = if mac == "-" { "" } else { mac };
                    self.iface_state
                        .insert(name.to_string(), (state == "up", mac.to_string()));
                }
            }
            "SNMP" => self
                .snmp
                .push(rest.split_whitespace().map(str::to_string).collect()),
            "SOCK" => parse_sockstat(rest, &mut self.sockstat),
            "DIO" => self.disks.extend(DiskCounters::parse(rest)),
            "TOPC" => self.top_cpu.extend(RawProcess::parse(rest)),
            "TOPM" => self.top_memory.extend(RawProcess::parse(rest)),
            "PSC" => self.ps_cpu.extend(parse_ps(rest)),
            "PSM" => self.ps_memory.extend(parse_ps(rest)),
            "TCPEST" => self.tcp_established = rest.trim().parse().ok(),
            "ADDR" => self.ip_lines.push(rest.to_string()),
            "IFCFG" => self.ifconfig_lines.push(rest.to_string()),
            "DF" => self.df.extend(DfRow::parse(rest)),
            "DFI" => {
                if let Some((mount, inodes)) = parse_df_inodes(rest) {
                    self.inodes.insert(mount, inodes);
                }
            }
            _ => {}
        }
    }

    /// 这一轮带了 IP 地址时，各网卡的地址
    fn addresses(&self) -> Option<HashMap<String, Vec<String>>> {
        if !self.ip_lines.is_empty() {
            Some(parse_ip_addr(&self.ip_lines))
        } else if !self.ifconfig_lines.is_empty() {
            Some(parse_ifconfig(&self.ifconfig_lines))
        } else {
            None
        }
    }
}

/// 上一轮留下、下一轮用来求差的计数器
struct Counters {
    timestamp_ms: u64,
    cpus: Vec<(String, CpuTimes)>,
    disks: Vec<(String, DiskCounters)>,
}

// ============================================================================
// 换算
// ============================================================================

fn load_info(tick: &Tick, prev: Option<&Counters>) -> Option<LoadInfo> {
    let memory = memory_info(&tick.meminfo);

    if tick.cpus.is_empty() {
        // 不是 Linux：只有平均负载和 ps 给出的进程
        if tick.load_average.is_none() && tick.ps_cpu.is_empty() && tick.ps_memory.is_empty() {
            return None;
        }
        return Some(LoadInfo {
            timestamp_ms: tick.timestamp_ms,
            cpu: CpuLoadInfo {
                load_average: tick.load_average.unwrap_or_default(),
                ..CpuLoadInfo::default()
            },
            memory,
            top_cpu_processes: tick.ps_cpu.clone(),
            top_memory_processes: tick.ps_memory.clone(),
        });
    }

    // 第一轮只记下计数器：使用率要和下一轮比较才算得出来
    let prev = prev?;
    let find = |cpus: &[(String, CpuTimes)], name: &str| {
        cpus.iter()
            .find(|(cpu, _)| cpu == name)
            .map(|(_, times)| *times)
    };
    let total = find(&tick.cpus, "cpu").zip(find(&prev.cpus, "cpu"));
    let usage = total.and_then(|(now, before)| cpu_usage(&before, &now));

    let mut cores: Vec<(u32, f32)> = tick
        .cpus
        .iter()
        .filter_map(|(name, now)| {
            let index = name.strip_prefix("cpu")?.parse().ok()?;
            let before = find(&prev.cpus, name)?;
            Some((index, cpu_usage(&before, now)?.0))
        })
        .collect();
    cores.sort_by_key(|(index, _)| *index);

    // 进程的 CPU 增量按「一个核在这段时间里的滴答数」折算成百分比
    let core_count = tick.cpus.iter().filter(|(name, _)| name != "cpu").count();
    let ticks_per_core = total
        .map(|(now, before)| now.since(&before).total() as f64 / core_count.max(1) as f64)
        .unwrap_or(0.0);
    let mem_total_kb = tick.meminfo.get("MemTotal").copied().unwrap_or(0);
    let process = |raw: &RawProcess| raw.to_info(ticks_per_core, mem_total_kb);

    Some(LoadInfo {
        timestamp_ms: tick.timestamp_ms,
        cpu: CpuLoadInfo {
            usage_percent: usage.map(|(percent, _)| percent),
            breakdown: usage.map(|(_, breakdown)| breakdown),
            per_core: cores.into_iter().map(|(_, percent)| percent).collect(),
            load_average: tick.load_average.unwrap_or_default(),
        },
        memory,
        top_cpu_processes: tick.top_cpu.iter().map(process).collect(),
        top_memory_processes: tick.top_memory.iter().map(process).collect(),
    })
}

/// 两次采样之间的 CPU 使用率和各类时间的占比。
/// 使用率 = 非空闲时间 / 总时间：steal 算在总时间里（虚拟机被宿主机占用的时间），iowait 算作空闲
fn cpu_usage(before: &CpuTimes, now: &CpuTimes) -> Option<(f32, CpuBreakdown)> {
    let delta = now.since(before);
    let total = delta.total();
    if total == 0 {
        return None;
    }
    let percent = |ticks: u64| (ticks as f64 / total as f64 * 100.0) as f32;
    let busy = total - delta.idle - delta.iowait;
    Some((
        percent(busy),
        CpuBreakdown {
            user: percent(delta.user + delta.nice),
            system: percent(delta.system),
            iowait: percent(delta.iowait),
            irq: percent(delta.irq + delta.softirq),
            steal: percent(delta.steal),
            idle: percent(delta.idle),
        },
    ))
}

/// 内存用量：已用 = 总量 - 可用。没有 MemTotal（非 Linux）时为 None
fn memory_info(meminfo: &HashMap<String, u64>) -> Option<MemoryLoadInfo> {
    let kb = |key: &str| meminfo.get(key).copied().unwrap_or(0);
    let total = *meminfo.get("MemTotal")?;
    // 3.14 之前的内核没有 MemAvailable，用空闲 + 缓存近似
    let available = meminfo
        .get("MemAvailable")
        .copied()
        .unwrap_or_else(|| kb("MemFree") + kb("Buffers") + kb("Cached") + kb("SReclaimable"))
        .min(total);
    Some(MemoryLoadInfo {
        used_bytes: (total - available) * 1024,
        available_bytes: available * 1024,
        buffers_bytes: kb("Buffers") * 1024,
        cached_bytes: kb("Cached") * 1024,
        swap_used_bytes: kb("SwapTotal").saturating_sub(kb("SwapFree")) * 1024,
    })
}

/// 两次采样之间各块磁盘的读写速率
fn disk_io(
    prev: &Counters,
    timestamp_ms: u64,
    disks: &[(String, DiskCounters)],
) -> Option<DiskIoInfo> {
    if disks.is_empty() {
        return None;
    }
    let elapsed_ms = timestamp_ms
        .checked_sub(prev.timestamp_ms)
        .filter(|ms| *ms > 0)?;
    let secs = elapsed_ms as f64 / 1000.0;
    let devices = disks
        .iter()
        .filter_map(|(name, now)| {
            // 开机以来没读写过的设备（没用上的 dm 设备等）不显示
            if now.reads == 0 && now.writes == 0 {
                return None;
            }
            let (_, before) = prev.disks.iter().find(|(disk, _)| disk == name)?;
            let reads = now.reads.saturating_sub(before.reads);
            let writes = now.writes.saturating_sub(before.writes);
            let wait_ms = now.read_ms.saturating_sub(before.read_ms)
                + now.write_ms.saturating_sub(before.write_ms);
            Some(DiskIoDevice {
                name: name.clone(),
                read_bytes_per_sec: now.sectors_read.saturating_sub(before.sectors_read) as f64
                    * SECTOR_BYTES
                    / secs,
                write_bytes_per_sec: now.sectors_written.saturating_sub(before.sectors_written)
                    as f64
                    * SECTOR_BYTES
                    / secs,
                reads_per_sec: reads as f64 / secs,
                writes_per_sec: writes as f64 / secs,
                busy_percent: (now.io_ms.saturating_sub(before.io_ms) as f64 / elapsed_ms as f64
                    * 100.0)
                    .min(100.0) as f32,
                await_ms: if reads + writes > 0 {
                    wait_ms as f32 / (reads + writes) as f32
                } else {
                    0.0
                },
            })
        })
        .collect();
    Some(DiskIoInfo {
        timestamp_ms,
        devices,
    })
}

/// 磁盘容量：这一轮带了 df 输出时才有
fn disk_info(tick: &Tick) -> Option<DiskInfo> {
    if tick.df.is_empty() {
        return None;
    }
    let disks = tick
        .df
        .iter()
        .filter(|row| row.total_kb > 0)
        .map(|row| {
            let (inodes_total, inodes_used, inodes_available) = tick
                .inodes
                .get(&row.mount_point)
                .copied()
                .unwrap_or_default();
            let usable = row.used_kb + row.available_kb;
            DiskDeviceInfo {
                device: row.device.clone(),
                mount_point: row.mount_point.clone(),
                fs_type: row.fs_type.clone(),
                total_bytes: row.total_kb * 1024,
                used_bytes: row.used_kb * 1024,
                available_bytes: row.available_kb * 1024,
                usage_percent: if usable > 0 {
                    (row.used_kb as f64 / usable as f64 * 100.0) as f32
                } else {
                    0.0
                },
                inodes_total,
                inodes_used,
                inodes_available,
            }
        })
        .collect();
    Some(DiskInfo {
        timestamp_ms: tick.timestamp_ms,
        disks,
    })
}

fn to_u32(value: u64) -> u32 {
    value.min(u32::MAX as u64) as u32
}

// ============================================================================
// 各类行的解析
// ============================================================================

/// 取出前 `n` 个以空白分隔的字段，以及剩下的部分（去掉开头的空白，中间的空格保留，
/// 用于含空格的进程名、挂载点）
fn split_fields(s: &str, n: usize) -> Option<(Vec<&str>, &str)> {
    let mut fields = Vec::with_capacity(n);
    let mut rest = s;
    for _ in 0..n {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if end == 0 {
            return None;
        }
        fields.push(&rest[..end]);
        rest = &rest[end..];
    }
    Some((fields, rest.trim_start()))
}

fn parse_u64(s: &str) -> u64 {
    s.parse().unwrap_or(0)
}

/// 进程名里的控制字符换成问号
fn clean_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// `TICK` 行的时刻（秒，可带小数）换成毫秒
fn parse_timestamp(rest: &str) -> u64 {
    rest.split_whitespace()
        .next()
        .and_then(|secs| secs.parse::<f64>().ok())
        .filter(|secs| secs.is_finite() && *secs >= 0.0)
        .map(|secs| (secs * 1000.0).round() as u64)
        .unwrap_or(0)
}

/// /proc/stat 中一行 cpu 的各类时间（时钟滴答数）
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CpuTimes {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    steal: u64,
}

impl CpuTimes {
    /// 解析 `cpu  1 2 3 …` 或 `cpu3 1 2 3 …`；老内核缺少的字段按 0 处理
    fn parse(line: &str) -> Option<(String, CpuTimes)> {
        let mut parts = line.split_whitespace();
        let name = parts.next().filter(|name| name.starts_with("cpu"))?;
        let mut values = [0u64; 8];
        for value in values.iter_mut() {
            match parts.next() {
                Some(field) => *value = field.parse().ok()?,
                None => break,
            }
        }
        let [user, nice, system, idle, iowait, irq, softirq, steal] = values;
        Some((
            name.to_string(),
            CpuTimes {
                user,
                nice,
                system,
                idle,
                iowait,
                irq,
                softirq,
                steal,
            },
        ))
    }

    /// 总时间。guest 时间已经计入 user，不再另加
    fn total(&self) -> u64 {
        self.user
            + self.nice
            + self.system
            + self.idle
            + self.iowait
            + self.irq
            + self.softirq
            + self.steal
    }

    /// 相对于 `before` 的增量；计数器变小（iowait 在部分内核上会回退）时按 0 处理
    fn since(&self, before: &CpuTimes) -> CpuTimes {
        CpuTimes {
            user: self.user.saturating_sub(before.user),
            nice: self.nice.saturating_sub(before.nice),
            system: self.system.saturating_sub(before.system),
            idle: self.idle.saturating_sub(before.idle),
            iowait: self.iowait.saturating_sub(before.iowait),
            irq: self.irq.saturating_sub(before.irq),
            softirq: self.softirq.saturating_sub(before.softirq),
            steal: self.steal.saturating_sub(before.steal),
        }
    }
}

/// Linux 的 /proc/loadavg（`0.52 0.58 0.59 2/412 12345`）或 BSD 的 sysctl（`{ 0.52 0.58 0.59 }`）
fn parse_load_average(rest: &str) -> Option<[f32; 3]> {
    let mut values = rest
        .split_whitespace()
        .filter_map(|field| field.trim_matches(['{', '}']).parse::<f32>().ok());
    Some([values.next()?, values.next()?, values.next()?])
}

/// `MemTotal:       16384000 kB` -> ("MemTotal", 16384000)
fn parse_meminfo(rest: &str) -> Option<(String, u64)> {
    let (key, value) = rest.split_once(':')?;
    let kb = value.split_whitespace().next()?.parse().ok()?;
    Some((key.trim().to_string(), kb))
}

/// 一块网卡的累计流量（/proc/net/dev）
struct NetCounters {
    name: String,
    rx_bytes: u64,
    rx_packets: u64,
    rx_errors: u64,
    tx_bytes: u64,
    tx_packets: u64,
    tx_errors: u64,
}

impl NetCounters {
    /// `eth0 <接收 8 项> <发送 8 项>`
    fn parse(rest: &str) -> Option<NetCounters> {
        let mut parts = rest.split_whitespace();
        let name = parts.next()?.to_string();
        let values: Vec<u64> = parts.map(parse_u64).collect();
        if values.len() < 11 {
            return None;
        }
        Some(NetCounters {
            name,
            rx_bytes: values[0],
            rx_packets: values[1],
            rx_errors: values[2],
            tx_bytes: values[8],
            tx_packets: values[9],
            tx_errors: values[10],
        })
    }
}

/// `TCP: inuse 5 orphan 0 tw 2 alloc 7 mem 1` -> `TCP inuse` = 5、`TCP tw` = 2 ……
fn parse_sockstat(rest: &str, into: &mut HashMap<String, u64>) {
    let mut parts = rest.split_whitespace();
    let Some(protocol) = parts.next().map(|p| p.trim_end_matches(':')) else {
        return;
    };
    while let (Some(key), Some(value)) = (parts.next(), parts.next()) {
        if let Ok(value) = value.parse() {
            into.insert(format!("{protocol} {key}"), value);
        }
    }
}

/// /proc/net/snmp 的 Tcp 两行（字段名、数值）-> 字段名到数值的映射
fn snmp_tcp(rows: &[Vec<String>]) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    for pair in rows.windows(2) {
        let (names, values) = (&pair[0], &pair[1]);
        // 字段名那一行在前：第二列不是数字
        let is_header = names
            .get(1)
            .is_some_and(|name| name.parse::<i64>().is_err());
        if !is_header {
            continue;
        }
        for (name, value) in names.iter().zip(values).skip(1) {
            // MaxConn 等字段可能是 -1
            if let Ok(value) = value.parse::<i64>() {
                map.insert(name.clone(), value.max(0) as u64);
            }
        }
    }
    map
}

/// 一块磁盘的累计读写（/proc/diskstats）
#[derive(Clone, Copy, Debug, Default)]
struct DiskCounters {
    reads: u64,
    sectors_read: u64,
    read_ms: u64,
    writes: u64,
    sectors_written: u64,
    write_ms: u64,
    /// 有 IO 在进行的累计时间
    io_ms: u64,
}

impl DiskCounters {
    /// `8 0 sda <读次数> <合并> <读扇区> <读耗时> <写次数> <合并> <写扇区> <写耗时> <进行中> <IO 时间> …`
    fn parse(rest: &str) -> Option<(String, DiskCounters)> {
        let fields: Vec<&str> = rest.split_whitespace().collect();
        if fields.len() < 13 {
            return None;
        }
        let value = |index: usize| parse_u64(fields[index]);
        Some((
            fields[2].to_string(),
            DiskCounters {
                reads: value(3),
                sectors_read: value(5),
                read_ms: value(6),
                writes: value(7),
                sectors_written: value(9),
                write_ms: value(10),
                io_ms: value(12),
            },
        ))
    }
}

/// 脚本挑出的进程（Linux）：CPU 是两轮之间的滴答数增量
struct RawProcess {
    pid: u32,
    cpu_ticks: u64,
    rss_kb: u64,
    user: String,
    name: String,
}

impl RawProcess {
    /// `<pid> <CPU 增量> <RSS kB> <用户> <进程名（可能含空格）>`
    fn parse(rest: &str) -> Option<RawProcess> {
        let (fields, name) = split_fields(rest, 4)?;
        Some(RawProcess {
            pid: fields[0].parse().ok()?,
            cpu_ticks: parse_u64(fields[1]),
            rss_kb: parse_u64(fields[2]),
            user: fields[3].to_string(),
            name: clean_name(name),
        })
    }

    /// `ticks_per_core` 是一个核在这段时间里的滴答数，`mem_total_kb` 是内存总量
    fn to_info(&self, ticks_per_core: f64, mem_total_kb: u64) -> ProcessInfo {
        ProcessInfo {
            pid: self.pid,
            name: self.name.clone(),
            cpu_percent: if ticks_per_core > 0.0 {
                (self.cpu_ticks as f64 / ticks_per_core * 100.0) as f32
            } else {
                0.0
            },
            memory_percent: if mem_total_kb > 0 {
                (self.rss_kb as f64 / mem_total_kb as f64 * 100.0) as f32
            } else {
                0.0
            },
            memory_bytes: self.rss_kb * 1024,
            user: self.user.clone(),
        }
    }
}

/// 非 Linux 的 ps 输出：`<pid> <%cpu> <%mem> <RSS kB> <用户> <命令路径（可能含空格）>`
fn parse_ps(rest: &str) -> Option<ProcessInfo> {
    let (fields, command) = split_fields(rest, 5)?;
    let name = command.rsplit('/').next().unwrap_or(command);
    Some(ProcessInfo {
        pid: fields[0].parse().ok()?,
        name: clean_name(name),
        cpu_percent: fields[1].parse().unwrap_or(0.0),
        memory_percent: fields[2].parse().unwrap_or(0.0),
        memory_bytes: parse_u64(fields[3]) * 1024,
        user: fields[4].to_string(),
    })
}

/// `ip -o addr show` 的输出 -> 网卡名到地址列表
fn parse_ip_addr(lines: &[String]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for line in lines {
        // `2: eth0    inet 10.0.0.5/24 brd … scope global eth0\ …`
        let mut parts = line.split_whitespace().skip(1);
        let (Some(name), Some(family), Some(address)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if family != "inet" && family != "inet6" {
            continue;
        }
        // veth 等网卡显示为 `veth1@if5`
        let name = name.split('@').next().unwrap_or(name);
        map.entry(name.to_string())
            .or_default()
            .push(bare_address(address));
    }
    map
}

/// `ifconfig -a` 的输出（新版 net-tools、旧版 net-tools 两种格式）-> 网卡名到地址列表
fn parse_ifconfig(lines: &[String]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(char::is_whitespace) {
            // 新版 `eth0: flags=…`，旧版 `eth0      Link encap:…`；别名 `eth0:1` 归到 eth0
            current = line
                .split_whitespace()
                .next()
                .and_then(|name| name.split(':').next())
                .map(str::to_string);
            continue;
        }
        let Some(name) = &current else {
            continue;
        };
        let mut words = line.split_whitespace();
        if matches!(words.next(), Some("inet" | "inet6")) {
            // 新版 `inet 10.0.0.5  netmask …`；旧版 `inet addr:10.0.0.5  Bcast:…`、`inet6 addr: fe80::1/64 …`
            let mut address = words.next();
            if address == Some("addr:") {
                address = words.next();
            }
            if let Some(address) = address.map(|a| a.trim_start_matches("addr:")) {
                if !address.is_empty() {
                    map.entry(name.clone())
                        .or_default()
                        .push(bare_address(address));
                }
            }
        }
    }
    map
}

/// 去掉地址后面的前缀长度（`/24`）和作用域（`%eth0`）
fn bare_address(address: &str) -> String {
    address
        .split(['/', '%'])
        .next()
        .unwrap_or(address)
        .to_string()
}

/// df -P -k 的一行（前面加了文件系统类型）
struct DfRow {
    fs_type: String,
    device: String,
    total_kb: u64,
    used_kb: u64,
    available_kb: u64,
    mount_point: String,
}

impl DfRow {
    /// `<类型> <设备> <总量> <已用> <可用> <使用率> <挂载点（可能含空格）>`
    fn parse(rest: &str) -> Option<DfRow> {
        let (fields, mount_point) = split_fields(rest, 6)?;
        if mount_point.is_empty() {
            return None;
        }
        Some(DfRow {
            fs_type: fields[0].to_string(),
            device: fields[1].to_string(),
            total_kb: parse_u64(fields[2]),
            used_kb: parse_u64(fields[3]),
            available_kb: parse_u64(fields[4]),
            mount_point: mount_point.to_string(),
        })
    }
}

/// df -P -i 的一行：`<设备> <总数> <已用> <可用> <使用率> <挂载点>` -> (挂载点, (总数, 已用, 可用))
fn parse_df_inodes(rest: &str) -> Option<(String, (u64, u64, u64))> {
    let (fields, mount_point) = split_fields(rest, 5)?;
    if mount_point.is_empty() {
        return None;
    }
    Some((
        mount_point.to_string(),
        (
            parse_u64(fields[1]),
            parse_u64(fields[2]),
            parse_u64(fields[3]),
        ),
    ))
}

// ============================================================================
// 系统信息（SYS 行的 JSON）
// ============================================================================

/// 把 JSON 数字解析为 u64，兼容把大整数写成浮点或科学计数法的情况
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
    let text = |value: &serde_json::Value| value.as_str().unwrap_or("unknown").to_string();
    SystemInfo {
        host: HostInfo {
            address: host_address.to_string(),
            hostname: text(&parsed["host"]["hostname"]),
            os: text(&parsed["host"]["os"]),
            kernel: text(&parsed["host"]["kernel"]),
            uptime_seconds: ju64(&parsed["host"]["uptime_seconds"]),
        },
        cpu: CpuInfo {
            model: text(&parsed["cpu"]["model"]),
            cores_physical: ju64(&parsed["cpu"]["cores_physical"]).max(1) as u32,
            cores_logical: ju64(&parsed["cpu"]["cores_logical"]).max(1) as u32,
            architecture: text(&parsed["cpu"]["architecture"]),
        },
        memory: MemoryTotalInfo {
            total_bytes: ju64(&parsed["memory"]["total_bytes"]),
            swap_total_bytes: ju64(&parsed["memory"]["swap_total_bytes"]),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(collector: &mut Collector, lines: &str) -> Vec<MonitorEvent> {
        lines
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .flat_map(|line| collector.feed(line))
            .collect()
    }

    fn load(events: &[MonitorEvent]) -> Option<&LoadInfo> {
        events.iter().find_map(|event| match event {
            MonitorEvent::LoadInfo(info) => Some(info),
            _ => None,
        })
    }

    fn network(events: &[MonitorEvent]) -> Option<&NetworkInfo> {
        events.iter().find_map(|event| match event {
            MonitorEvent::NetworkInfo(info) => Some(info),
            _ => None,
        })
    }

    fn disk_io_of(events: &[MonitorEvent]) -> Option<&DiskIoInfo> {
        events.iter().find_map(|event| match event {
            MonitorEvent::DiskIo(info) => Some(info),
            _ => None,
        })
    }

    fn disk(events: &[MonitorEvent]) -> Option<&DiskInfo> {
        events.iter().find_map(|event| match event {
            MonitorEvent::DiskInfo(info) => Some(info),
            _ => None,
        })
    }

    #[track_caller]
    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "expected {expected}, got {actual}"
        );
    }

    /// 两轮之间：2 个核、2 秒，合计 500 个滴答（每核 250）
    const FIRST_TICK: &str = "
        TICK 100.00 400.00
        CPU cpu  1000 100 500 8000 200 50 50 100 0 0
        CPU cpu0 500 50 250 4000 100 25 25 50 0 0
        CPU cpu1 500 50 250 4000 100 25 25 50 0 0
        LOADAVG 0.50 0.40 0.30 1/200 4321
        MEM MemTotal:        8000000 kB
        MEM MemAvailable:    6000000 kB
        MEM Buffers:          100000 kB
        MEM Cached:          3000000 kB
        MEM SwapTotal:       2000000 kB
        MEM SwapFree:        1500000 kB
        DIO    8       0 sda 1000 0 20000 300 500 0 8000 200 0 450 600
        DIO  253       0 dm-1 0 0 0 0 0 0 0 0 0 0 0
        END
    ";
    const SECOND_TICK: &str = "
        TICK 102.00 404.00
        CPU cpu  1100 100 550 8300 220 50 50 130 0 0
        CPU cpu0 580 50 280 4120 110 25 25 60 0 0
        CPU cpu1 520 50 270 4180 110 25 25 70 0 0
        LOADAVG 0.60 0.45 0.31 2/201 4330
        MEM MemTotal:        8000000 kB
        MEM MemAvailable:    5000000 kB
        MEM Buffers:          100000 kB
        MEM Cached:          3000000 kB
        MEM SwapTotal:       2000000 kB
        MEM SwapFree:        1500000 kB
        DIO    8       0 sda 1100 0 22000 340 600 0 9000 260 0 1450 700
        DIO  253       0 dm-1 0 0 0 0 0 0 0 0 0 0 0
        TOPC 4242 150 200000 alice my (weird) app
        TOPM 4242 150 200000 alice my (weird) app
        TOPM 1 0 10000 root systemd
        END
    ";

    #[test]
    fn first_tick_only_primes_the_counters() {
        let mut collector = Collector::new("10.0.0.1");
        let events = feed_all(&mut collector, FIRST_TICK);
        assert!(load(&events).is_none());
        assert!(disk_io_of(&events).is_none());
    }

    #[test]
    fn cpu_usage_counts_steal_as_busy_and_iowait_as_idle() {
        let mut collector = Collector::new("10.0.0.1");
        feed_all(&mut collector, FIRST_TICK);
        let events = feed_all(&mut collector, SECOND_TICK);
        let cpu = &load(&events).expect("second tick reports load").cpu;

        // 合计增量：user 100、system 50、idle 300、iowait 20、steal 30，共 500
        assert_close(cpu.usage_percent.unwrap() as f64, 36.0);
        let breakdown = cpu.breakdown.unwrap();
        assert_close(breakdown.user as f64, 20.0);
        assert_close(breakdown.system as f64, 10.0);
        assert_close(breakdown.iowait as f64, 4.0);
        assert_close(breakdown.steal as f64, 6.0);
        assert_close(breakdown.idle as f64, 60.0);
        // cpu0：(250 - 120 - 10) / 250；cpu1：(250 - 180 - 10) / 250
        assert_eq!(cpu.per_core.len(), 2);
        assert_close(cpu.per_core[0] as f64, 48.0);
        assert_close(cpu.per_core[1] as f64, 24.0);
        assert_eq!(cpu.load_average, [0.60, 0.45, 0.31]);
    }

    #[test]
    fn process_cpu_is_relative_to_one_core() {
        let mut collector = Collector::new("10.0.0.1");
        feed_all(&mut collector, FIRST_TICK);
        let events = feed_all(&mut collector, SECOND_TICK);
        let load = load(&events).unwrap();

        // 150 个滴答 / 每核 250 个滴答 = 60%
        let top = &load.top_cpu_processes[0];
        assert_eq!((top.pid, top.name.as_str()), (4242, "my (weird) app"));
        assert_eq!(top.user, "alice");
        assert_close(top.cpu_percent as f64, 60.0);
        // 200000 kB / 8000000 kB
        assert_close(top.memory_percent as f64, 2.5);
        assert_eq!(top.memory_bytes, 200_000 * 1024);
        assert_eq!(load.top_memory_processes.len(), 2);
        assert_eq!(load.top_memory_processes[1].name, "systemd");
    }

    #[test]
    fn memory_used_is_total_minus_available() {
        let mut collector = Collector::new("10.0.0.1");
        feed_all(&mut collector, FIRST_TICK);
        let events = feed_all(&mut collector, SECOND_TICK);
        let memory = load(&events).unwrap().memory.clone().unwrap();
        assert_eq!(memory.used_bytes, 3_000_000 * 1024);
        assert_eq!(memory.available_bytes, 5_000_000 * 1024);
        assert_eq!(memory.cached_bytes, 3_000_000 * 1024);
        assert_eq!(memory.swap_used_bytes, 500_000 * 1024);

        // 没有 MemAvailable 的老内核：空闲 + Buffers + Cached + SReclaimable
        let old_kernel: HashMap<String, u64> = [
            ("MemTotal", 1000),
            ("MemFree", 100),
            ("Buffers", 50),
            ("Cached", 200),
            ("SReclaimable", 50),
        ]
        .into_iter()
        .map(|(key, kb)| (key.to_string(), kb))
        .collect();
        assert_eq!(memory_info(&old_kernel).unwrap().used_bytes, 600 * 1024);
    }

    #[test]
    fn disk_io_rates_come_from_diskstats_deltas() {
        let mut collector = Collector::new("10.0.0.1");
        feed_all(&mut collector, FIRST_TICK);
        let events = feed_all(&mut collector, SECOND_TICK);
        let io = disk_io_of(&events).expect("second tick reports disk io");

        // dm-1 开机以来没有读写，不显示
        assert_eq!(io.devices.len(), 1);
        let sda = &io.devices[0];
        assert_eq!(sda.name, "sda");
        // 2 秒内：读 100 次、2000 扇区；写 100 次、1000 扇区；IO 时间 1000 ms
        assert_close(sda.read_bytes_per_sec, 512_000.0);
        assert_close(sda.write_bytes_per_sec, 256_000.0);
        assert_close(sda.reads_per_sec, 50.0);
        assert_close(sda.writes_per_sec, 50.0);
        assert_close(sda.busy_percent as f64, 50.0);
        // (40 + 60) ms / 200 次
        assert_close(sda.await_ms as f64, 0.5);
    }

    #[test]
    fn incomplete_tick_is_dropped() {
        let mut collector = Collector::new("10.0.0.1");
        feed_all(&mut collector, FIRST_TICK);
        // 第二轮在 END 之前被截断，紧接着开始第三轮：只按完整的第三轮和第一轮比较
        let events = feed_all(
            &mut collector,
            "
            TICK 101.00 0
            CPU cpu  1 1 1 1 1 1 1 1 0 0
            ",
        );
        assert!(events.is_empty());
        let events = feed_all(&mut collector, SECOND_TICK);
        assert_close(
            load(&events).unwrap().cpu.usage_percent.unwrap() as f64,
            36.0,
        );
    }

    #[test]
    fn counters_going_backwards_do_not_underflow() {
        // 远端重启后计数器从头开始：本轮的差值按 0 处理，不报错
        let mut collector = Collector::new("10.0.0.1");
        feed_all(&mut collector, SECOND_TICK);
        let events = feed_all(&mut collector, FIRST_TICK);
        let load = load(&events).unwrap();
        assert_eq!(load.cpu.usage_percent, None);
        assert!(disk_io_of(&events).is_none());
    }

    #[test]
    fn network_combines_counters_state_and_cached_addresses() {
        let mut collector = Collector::new("10.0.0.1");
        let events = feed_all(
            &mut collector,
            r"
            TICK 100.00 0
            NET lo 1000 10 0 0 0 0 0 0 1000 10 0 0 0 0 0 0
            IFACE lo unknown 00:00:00:00:00:00
            NET eth0 123456789 1000 1 2 0 0 0 0 987654321 2000 3 4 0 0 0 0
            IFACE eth0 up 52:54:00:12:34:56
            NET wg0 10 1 0 0 0 0 0 0 20 2 0 0 0 0 0 0
            IFACE wg0 unknown -
            SNMP Tcp: RtoAlgorithm RtoMin RtoMax MaxConn ActiveOpens PassiveOpens AttemptFails EstabResets CurrEstab InSegs OutSegs
            SNMP Tcp: 1 200 120000 -1 100 50 3 2 7 10000 9000
            SOCK TCP: inuse 9 orphan 0 tw 4 alloc 12 mem 3
            SOCK TCP6: inuse 2
            ADDR 1: lo    inet 127.0.0.1/8 scope host lo\       valid_lft forever preferred_lft forever
            ADDR 2: eth0    inet 10.0.0.5/24 brd 10.0.0.255 scope global eth0\       valid_lft forever
            ADDR 2: eth0    inet6 fe80::5054:ff:fe12:3456/64 scope link \       valid_lft forever
            END
            ",
        );
        let info = network(&events).unwrap();
        assert_eq!(info.timestamp_ms, 100_000);
        assert_eq!(info.global.tcp_established, 7);
        assert_eq!(info.global.tcp_time_wait, 4);
        assert_eq!(info.global.tcp_connections, 15);

        let names: Vec<_> = info.interfaces.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["lo", "eth0", "wg0"]);
        let eth0 = &info.interfaces[1];
        assert!(eth0.is_up);
        assert_eq!(eth0.mac_address, "52:54:00:12:34:56");
        assert_eq!(eth0.ip_addresses, ["10.0.0.5", "fe80::5054:ff:fe12:3456"]);
        assert_eq!((eth0.rx_bytes, eth0.tx_bytes), (123_456_789, 987_654_321));
        assert_eq!((eth0.rx_errors, eth0.tx_errors), (1, 3));
        assert_eq!(info.interfaces[2].mac_address, "");

        // 下一轮没有 ADDR 行：沿用上一次的地址
        let events = feed_all(
            &mut collector,
            "
            TICK 102.00 0
            NET eth0 123456999 1001 1 2 0 0 0 0 987654999 2001 3 4 0 0 0 0
            IFACE eth0 up 52:54:00:12:34:56
            END
            ",
        );
        let eth0 = &network(&events).unwrap().interfaces[0];
        assert_eq!(eth0.ip_addresses, ["10.0.0.5", "fe80::5054:ff:fe12:3456"]);
    }

    #[test]
    fn ifconfig_output_in_both_formats() {
        let lines: Vec<String> = [
            "eth0: flags=4163<UP,BROADCAST,RUNNING,MULTICAST>  mtu 1500",
            "        inet 10.0.0.5  netmask 255.255.255.0  broadcast 10.0.0.255",
            "        inet6 fe80::1  prefixlen 64  scopeid 0x20<link>",
            "",
            "eth1      Link encap:Ethernet  HWaddr 00:11:22:33:44:55",
            "          inet addr:192.168.1.9  Bcast:192.168.1.255  Mask:255.255.255.0",
            "          inet6 addr: fe80::2/64 Scope:Link",
            "eth1:1    Link encap:Ethernet  HWaddr 00:11:22:33:44:55",
            "          inet addr:192.168.1.10  Bcast:192.168.1.255  Mask:255.255.255.0",
        ]
        .iter()
        .map(|line| line.to_string())
        .collect();
        let map = parse_ifconfig(&lines);
        assert_eq!(map["eth0"], ["10.0.0.5", "fe80::1"]);
        assert_eq!(map["eth1"], ["192.168.1.9", "fe80::2", "192.168.1.10"]);
    }

    #[test]
    fn disk_capacity_handles_spaces_in_mount_points() {
        let mut collector = Collector::new("10.0.0.1");
        let events = feed_all(
            &mut collector,
            "
            TICK 100.00 0
            DF ext4 /dev/sda1 41152736 12345678 26706518 32% /
            DF xfs /dev/sdb1 1000000 250000 700000 27% /mnt/my data
            DF nfs4 server:/export 0 0 0 - /mnt/empty
            DFI /dev/sda1 2621440 123456 2497984 5% /
            DFI /dev/sdb1 500000 1000 499000 1% /mnt/my data
            END
            ",
        );
        let disks = &disk(&events).unwrap().disks;
        // 容量为 0 的跳过
        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].fs_type, "ext4");
        assert_eq!(disks[0].total_bytes, 41_152_736 * 1024);
        assert_eq!(disks[0].inodes_used, 123_456);
        let data = &disks[1];
        assert_eq!(data.mount_point, "/mnt/my data");
        assert_eq!(data.device, "/dev/sdb1");
        // 与 df 一致：已用 /（已用 + 可用）
        assert_close(data.usage_percent as f64, 26.315_79);
        assert_eq!(
            (data.inodes_total, data.inodes_used, data.inodes_available),
            (500_000, 1000, 499_000)
        );
    }

    #[test]
    fn non_linux_ticks_report_load_average_and_processes_only() {
        let mut collector = Collector::new("10.0.0.1");
        let events = feed_all(
            &mut collector,
            "
            TICK 1795154475
            LOADAVG { 2.11 2.04 2.30 }
            PSC 41279  18.9  1.0 159984 alice            /Applications/ExampleApp.app/Contents/MacOS/example-desktop
            PSM 41818   5.7  3.8 631488 alice            /Applications/Browser.app/Contents/Frameworks/Browser Helper (Renderer).app/Contents/MacOS/Browser Helper (Renderer)
            TCPEST 12
            END
            ",
        );
        let load = load(&events).unwrap();
        assert_eq!(load.cpu.usage_percent, None);
        assert!(load.memory.is_none());
        assert_eq!(load.cpu.load_average, [2.11, 2.04, 2.30]);
        let top = &load.top_cpu_processes[0];
        assert_eq!((top.pid, top.name.as_str()), (41279, "example-desktop"));
        assert_close(top.cpu_percent as f64, 18.9);
        assert_eq!(top.memory_bytes, 159_984 * 1024);
        assert_eq!(
            load.top_memory_processes[0].name,
            "Browser Helper (Renderer)"
        );
        assert_eq!(network(&events).unwrap().global.tcp_established, 12);
        assert!(network(&events).unwrap().interfaces.is_empty());
    }

    #[test]
    fn sys_line_becomes_system_info() {
        let mut collector = Collector::new("10.0.0.1");
        let events = collector.feed(
            r#"SYS {"host":{"hostname":"web","os":"Ubuntu 24.04 LTS","kernel":"6.8.0","uptime_seconds":1.67237e+10},"cpu":{"model":"EPYC","cores_physical":4,"cores_logical":8,"architecture":"x86_64"},"memory":{"total_bytes":16723714048,"swap_total_bytes":0}}"#,
        );
        let [MonitorEvent::SystemInfo(info)] = events.as_slice() else {
            panic!("expected one SystemInfo, got {events:?}");
        };
        assert_eq!(info.host.address, "10.0.0.1");
        assert_eq!(info.host.os, "Ubuntu 24.04 LTS");
        assert_eq!(info.cpu.cores_logical, 8);
        assert_eq!(info.memory.total_bytes, 16_723_714_048);
    }

    /// 在样例目录上跑一遍真实的脚本（用本机的 sh 和 awk），检查 awk 程序挑出的行能被正确解析。
    /// 两轮读到的文件相同，所以 CPU 增量为 0；增量的换算由上面的单元测试覆盖
    #[cfg(unix)]
    #[test]
    fn script_reads_a_fixture_system() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let fixture = Fixture::new();
        let root = fixture.root.display().to_string();
        let script = format!(
            "INTERVAL=0\nSLOW_EVERY=1\nMAX_TICKS=2\nROOT='{root}'\n\
             DF_K='cat {root}/df_k.txt'\nDF_I='cat {root}/df_i.txt'\n\
             IP_CMD='cat {root}/ip.txt'\nIP_TAG=ADDR\n{SCRIPT}"
        );
        let mut child = Command::new("sh")
            .arg("-s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run sh");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "script failed: {stderr}\n--- stdout ---\n{stdout}"
        );

        let mut collector = Collector::new("10.0.0.1");
        let mut ticks: Vec<Vec<MonitorEvent>> = Vec::new();
        for line in stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let events = collector.feed(line);
            if line == "END" {
                ticks.push(events);
            }
        }
        assert_eq!(ticks.len(), 2, "two ticks expected:\n{stdout}\n{stderr}");

        // 第一轮：网络和磁盘容量，没有负载
        assert!(load(&ticks[0]).is_none());
        let net = network(&ticks[0]).expect("network in first tick");
        let names: Vec<_> = net.interfaces.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["lo", "eth0"], "veth is skipped:\n{stdout}");
        let eth0 = &net.interfaces[1];
        assert!(eth0.is_up);
        assert_eq!(eth0.mac_address, "52:54:00:12:34:56");
        assert_eq!(eth0.rx_bytes, 123_456_789);
        assert_eq!(eth0.ip_addresses, ["10.0.0.5", "fe80::5054:ff:fe12:3456"]);
        assert_eq!(net.global.tcp_established, 7);
        assert_eq!(net.global.tcp_connections, 15);

        let disks = &disk(&ticks[0]).expect("disk capacity in first tick").disks;
        let mounts: Vec<_> = disks.iter().map(|d| d.mount_point.as_str()).collect();
        assert_eq!(
            mounts,
            ["/", "/mnt/my data"],
            "pseudo filesystems are skipped"
        );
        assert_eq!(disks[1].fs_type, "xfs");
        assert_eq!(disks[1].inodes_total, 500_000);

        // 第二轮：负载（CPU 增量为 0 所以没有使用率）、内存、进程，磁盘读写
        let load = load(&ticks[1]).expect("load in second tick");
        assert_eq!(load.cpu.usage_percent, None);
        assert_eq!(load.cpu.load_average, [0.5, 0.4, 0.3]);
        let memory = load.memory.clone().unwrap();
        assert_eq!(memory.used_bytes, 2_000_000 * 1024);
        assert_eq!(memory.swap_used_bytes, 500_000 * 1024);
        // CPU 时间没变，CPU 排行为空；内存排行按 RSS，内核线程（RSS 为 0）不算
        assert!(load.top_cpu_processes.is_empty());
        let top: Vec<_> = load
            .top_memory_processes
            .iter()
            .map(|p| (p.pid, p.name.as_str(), p.user.as_str(), p.memory_bytes))
            .collect();
        assert_eq!(
            top,
            [
                (4242, "my (weird) app", "alice", 200_000 * 1024),
                (1, "systemd", "root", 10_000 * 1024),
            ]
        );

        // 磁盘读写：两轮读到的开机时间相同，算不出速率，只检查 awk 挑出的是整块磁盘
        let dio_devices: Vec<_> = stdout
            .lines()
            .filter_map(|line| line.strip_prefix("DIO "))
            .filter_map(|rest| rest.split_whitespace().nth(2))
            .collect();
        assert_eq!(
            dio_devices,
            ["sda", "dm-0", "sda", "dm-0"],
            "partitions and loop devices are skipped"
        );
        assert!(disk_io_of(&ticks[1]).is_none());
    }

    /// 临时的样例目录：仿照 Linux 的 /proc、/sys、/etc，测试结束时删除
    struct Fixture {
        root: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let root = std::env::temp_dir().join(format!(
                "shellmaster-collector-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let files: &[(&str, &str)] = &[
                ("proc/uptime", "1000.50 3000.00\n"),
                (
                    "proc/stat",
                    "cpu  100 0 50 800 20 5 5 20 0 0\n\
                     cpu0 50 0 25 400 10 2 3 10 0 0\n\
                     cpu1 50 0 25 400 10 3 2 10 0 0\n\
                     intr 12345 0 0 0\nctxt 999\nbtime 1700000000\nprocesses 500\n\
                     procs_running 1\nprocs_blocked 0\n",
                ),
                ("proc/loadavg", "0.50 0.40 0.30 1/200 4321\n"),
                (
                    "proc/meminfo",
                    "MemTotal:        8000000 kB\nMemFree:         1000000 kB\n\
                     MemAvailable:    6000000 kB\nBuffers:          100000 kB\n\
                     Cached:          3000000 kB\nSwapCached:            0 kB\n\
                     Active(anon):     123456 kB\nSReclaimable:     200000 kB\n\
                     SwapTotal:       2000000 kB\nSwapFree:        1500000 kB\n",
                ),
                (
                    "proc/net/dev",
                    "Inter-|   Receive                            |  Transmit\n \
                     face |bytes    packets errs drop fifo frame compressed multicast|bytes\n    \
                     lo:    1000      10    0    0    0     0          0         0     1000      10    0    0    0     0       0          0\n  \
                     eth0:123456789 1000 1 2 0 0 0 0 987654321 2000 3 4 0 0 0 0\n\
                     veth1234: 5 5 0 0 0 0 0 0 5 5 0 0 0 0 0 0\n",
                ),
                ("sys/class/net/lo/operstate", "unknown\n"),
                ("sys/class/net/lo/address", "00:00:00:00:00:00\n"),
                ("sys/class/net/eth0/operstate", "up\n"),
                ("sys/class/net/eth0/address", "52:54:00:12:34:56\n"),
                (
                    "proc/net/snmp",
                    "Ip: Forwarding DefaultTTL\nIp: 1 64\n\
                     Tcp: RtoAlgorithm RtoMin RtoMax MaxConn ActiveOpens PassiveOpens AttemptFails EstabResets CurrEstab InSegs OutSegs RetransSegs\n\
                     Tcp: 1 200 120000 -1 100 50 3 2 7 10000 9000 12\n\
                     Udp: InDatagrams\nUdp: 5\n",
                ),
                (
                    "proc/net/sockstat",
                    "sockets: used 300\nTCP: inuse 9 orphan 0 tw 4 alloc 12 mem 3\nUDP: inuse 2 mem 1\n",
                ),
                ("proc/net/sockstat6", "TCP6: inuse 2\nUDP6: inuse 1\n"),
                (
                    "proc/diskstats",
                    "   8       0 sda 1000 10 20000 300 500 5 8000 200 0 450 600\n   \
                     8       1 sda1 900 10 18000 250 400 5 7000 150 0 400 500\n   \
                     7       0 loop0 10 0 20 1 0 0 0 0 0 1 1\n \
                     253       0 dm-0 800 0 16000 200 450 0 7500 180 0 420 380\n",
                ),
                ("sys/block/sda/stat", "1000 10 20000 300\n"),
                ("sys/block/dm-0/stat", "800 0 16000 200\n"),
                ("sys/block/loop0/stat", "10 0 20 1\n"),
                (
                    "proc/1/stat",
                    "1 (systemd) S 0 1 1 0 -1 4194560 1000 2000 10 20 150 50 30 40 20 0 1 0 5 170000000 2500 18446744073709551615\n",
                ),
                ("proc/1/status", "Name:\tsystemd\nUid:\t0\t0\t0\t0\nVmRSS:\t   10000 kB\n"),
                (
                    "proc/4242/stat",
                    "4242 (my (weird) app) R 1 4242 4242 0 -1 4194304 10 0 0 0 900 100 0 0 20 0 4 0 100 500000000 50000 18446744073709551615\n",
                ),
                ("proc/4242/status", "Name:\tmy (weird) app\nUid:\t1000\t1000\t1000\t1000\nVmRSS:\t  200000 kB\n"),
                (
                    "proc/77/stat",
                    "77 (kworker/0:1-events) I 2 0 0 0 -1 69238880 0 0 0 0 5 30 0 0 20 0 1 0 50 0 0 18446744073709551615\n",
                ),
                ("proc/77/status", "Name:\tkworker/0:1-events\nUid:\t0\t0\t0\t0\n"),
                (
                    "etc/passwd",
                    "root:x:0:0:root:/root:/bin/bash\nalice:x:1000:1000::/home/alice:/bin/sh\n",
                ),
                (
                    "proc/mounts",
                    "/dev/sda1 / ext4 rw,relatime 0 0\ntmpfs /run tmpfs rw 0 0\n\
                     /dev/sdb1 /mnt/my\\040data xfs rw 0 0\n\
                     overlay /var/lib/docker/overlay2/x/merged overlay rw 0 0\n",
                ),
                (
                    "df_k.txt",
                    "Filesystem 1024-blocks Used Available Capacity Mounted on\n\
                     /dev/sda1 41152736 12345678 26706518 32% /\n\
                     tmpfs 1000 0 1000 0% /run\n\
                     /dev/sdb1 1000000 250000 750000 25% /mnt/my data\n\
                     overlay 41152736 12345678 26706518 32% /var/lib/docker/overlay2/x/merged\n",
                ),
                (
                    "df_i.txt",
                    "Filesystem Inodes IUsed IFree IUse% Mounted on\n\
                     /dev/sda1 2621440 123456 2497984 5% /\n\
                     /dev/sdb1 500000 1000 499000 1% /mnt/my data\n",
                ),
                (
                    "ip.txt",
                    "1: lo    inet 127.0.0.1/8 scope host lo\\       valid_lft forever preferred_lft forever\n\
                     2: eth0    inet 10.0.0.5/24 brd 10.0.0.255 scope global eth0\\       valid_lft forever preferred_lft forever\n\
                     2: eth0    inet6 fe80::5054:ff:fe12:3456/64 scope link \\       valid_lft forever preferred_lft forever\n\
                     5: veth1234@if4    inet6 fe80::1/64 scope link \\       valid_lft forever preferred_lft forever\n",
                ),
            ];
            for (path, content) in files {
                let path = root.join(path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, content).unwrap();
            }
            Self { root }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}
