// Monitor detail dialog components

use gpui::*;
use gpui_component::{ActiveTheme, StyledExt};

use crate::constants::icons;
use crate::models::monitor::MonitorState;

/// Detail dialog types
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DetailDialogType {
    None,
    SystemInfo,
    LoadInfo,
    NetworkInfo,
    DiskInfo,
}

/// Detail dialog state
#[derive(Clone)]
pub struct DetailDialogState {
    pub dialog_type: DetailDialogType,
}

impl Default for DetailDialogState {
    fn default() -> Self {
        Self {
            dialog_type: DetailDialogType::None,
        }
    }
}

impl DetailDialogState {
    pub fn is_open(&self) -> bool {
        self.dialog_type != DetailDialogType::None
    }

    pub fn open(&mut self, dialog_type: DetailDialogType) {
        self.dialog_type = dialog_type;
    }

    pub fn close(&mut self) {
        self.dialog_type = DetailDialogType::None;
    }
}

/// Render detail icon button for each section
pub fn render_detail_button(
    dialog_state: Entity<DetailDialogState>,
    dialog_type: DetailDialogType,
    cx: &App,
) -> impl IntoElement {
    let muted_color = cx.theme().muted_foreground;

    div()
        .id(SharedString::from(format!("detail-btn-{:?}", dialog_type)))
        .w(px(20.))
        .h(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .cursor_pointer()
        .hover(|s| s.bg(cx.theme().secondary_hover))
        .on_click(move |_, _, cx| {
            dialog_state.update(cx, |s, _| {
                s.open(dialog_type);
            });
        })
        .child(
            svg()
                .path(icons::EXPAND)
                .size(px(14.))
                .text_color(muted_color),
        )
}

/// Render detail dialog overlay
pub fn render_detail_dialog(
    dialog_state: Entity<DetailDialogState>,
    monitor_state: &MonitorState,
    cx: &App,
) -> impl IntoElement {
    let state = dialog_state.read(cx);

    if !state.is_open() {
        return div().into_any_element();
    }

    let dialog_type = state.dialog_type;
    let bg_overlay = hsla(0.0, 0.0, 0.0, 0.5);
    let dialog_bg = cx.theme().popover;
    let border_color = cx.theme().border;
    let title_color = hsla(210.0 / 360.0, 1.0, 0.5, 1.0);
    let muted_color = cx.theme().muted_foreground;

    // Get language
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    let title = match dialog_type {
        DetailDialogType::SystemInfo => crate::i18n::t(&lang, "monitor.system_info").to_string(),
        DetailDialogType::LoadInfo => crate::i18n::t(&lang, "monitor.load").to_string(),
        DetailDialogType::NetworkInfo => crate::i18n::t(&lang, "monitor.network").to_string(),
        DetailDialogType::DiskInfo => crate::i18n::t(&lang, "monitor.disk").to_string(),
        DetailDialogType::None => String::new(),
    };

    let dialog_state_close = dialog_state.clone();

    div()
        .id("detail-dialog-backdrop")
        .absolute()
        .inset_0()
        .bg(bg_overlay)
        .flex()
        .items_start() // 从顶部开始
        .pt(px(80.)) // 距离顶部 80px，使弹窗更居中
        .justify_center()
        // Close on backdrop click
        .on_click(move |_, _, cx| {
            dialog_state_close.update(cx, |s, _| s.close());
        })
        .child(
            div()
                .w(px(500.))
                .h(px(400.)) // 固定高度，4个弹窗大小一致
                .bg(dialog_bg)
                .rounded_lg()
                .border_1()
                .border_color(border_color)
                .shadow_lg()
                .overflow_hidden()
                .flex()
                .flex_col()
                // Stop propagation to prevent closing when clicking inside dialog
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                // Header
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(border_color)
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_base()
                                .font_medium()
                                .text_color(title_color)
                                .child(title),
                        )
                        .child({
                            let dialog_state_x = dialog_state.clone();
                            div()
                                .id("close-detail-dialog")
                                .w(px(24.))
                                .h(px(24.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.))
                                .cursor_pointer()
                                .hover(|s| s.bg(cx.theme().secondary_hover))
                                .on_click(move |_, _, cx| {
                                    dialog_state_x.update(cx, |s, _| s.close());
                                })
                                .child(svg().path(icons::X).size(px(16.)).text_color(muted_color))
                        }),
                )
                // Content
                .child(
                    div()
                        .id("detail-dialog-content-scroll")
                        .flex_1()
                        .overflow_y_scroll()
                        .p_4()
                        .child(match dialog_type {
                            DetailDialogType::SystemInfo => {
                                render_system_detail(monitor_state, cx).into_any_element()
                            }
                            DetailDialogType::LoadInfo => {
                                render_load_detail(monitor_state, cx).into_any_element()
                            }
                            DetailDialogType::NetworkInfo => {
                                render_network_detail(monitor_state, cx).into_any_element()
                            }
                            DetailDialogType::DiskInfo => {
                                render_disk_detail(monitor_state, cx).into_any_element()
                            }
                            DetailDialogType::None => div().into_any_element(),
                        }),
                ),
        )
        .into_any_element()
}

/// Render system info detail
fn render_system_detail(state: &MonitorState, cx: &App) -> impl IntoElement {
    let label_color = cx.theme().foreground;
    let value_color = cx.theme().muted_foreground;

    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    if let Some(info) = &state.system_info {
        div()
            .flex()
            .flex_col()
            .gap_3()
            // Host section
            .child(render_detail_section(
                "主机信息",
                vec![
                    ("主机名", info.host.hostname.clone()),
                    ("操作系统", info.host.os.clone()),
                    ("运行时间", format_uptime(info.host.uptime_seconds)),
                ],
                cx,
            ))
            // CPU section
            .child(render_detail_section(
                "CPU 信息",
                vec![
                    ("型号", info.cpu.model.clone()),
                    ("物理核心", info.cpu.cores_physical.to_string()),
                    ("逻辑核心", info.cpu.cores_logical.to_string()),
                    ("架构", info.cpu.architecture.clone()),
                ],
                cx,
            ))
            // Memory section
            .child(render_detail_section(
                "内存信息",
                vec![
                    ("总内存", format_bytes(info.memory.total_bytes)),
                    ("Swap 总计", format_bytes(info.memory.swap_total_bytes)),
                ],
                cx,
            ))
    } else {
        div()
            .text_sm()
            .text_color(value_color)
            .child(crate::i18n::t(&lang, "monitor.no_data"))
    }
}

/// Render load info detail
fn render_load_detail(state: &MonitorState, cx: &App) -> impl IntoElement {
    let value_color = cx.theme().muted_foreground;

    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    if let Some(load) = state.current_load() {
        let total_mem = state
            .system_info
            .as_ref()
            .map(|s| s.memory.total_bytes)
            .unwrap_or(1);

        div()
            .flex()
            .flex_col()
            .gap_3()
            // CPU section
            .child(render_detail_section(
                "CPU 负载",
                vec![
                    ("使用率", format!("{:.1}%", load.cpu.usage_percent)),
                    ("1分钟负载", format!("{:.2}", load.cpu.load_average[0])),
                    ("5分钟负载", format!("{:.2}", load.cpu.load_average[1])),
                    ("15分钟负载", format!("{:.2}", load.cpu.load_average[2])),
                ],
                cx,
            ))
            // Memory section
            .child(render_detail_section(
                "内存使用",
                vec![
                    ("已使用", format_bytes(load.memory.used_bytes)),
                    ("可用", format_bytes(load.memory.available_bytes)),
                    ("Buffers", format_bytes(load.memory.buffers_bytes)),
                    ("Cached", format_bytes(load.memory.cached_bytes)),
                    ("Swap 已使用", format_bytes(load.memory.swap_used_bytes)),
                ],
                cx,
            ))
            // Top CPU processes
            .child(render_process_table(
                "Top CPU 进程",
                &load.top_cpu_processes,
                cx,
            ))
            // Top Memory processes
            .child(render_process_table(
                "Top 内存进程",
                &load.top_memory_processes,
                cx,
            ))
    } else {
        div()
            .text_sm()
            .text_color(value_color)
            .child(crate::i18n::t(&lang, "monitor.no_data"))
    }
}

/// Render network info detail
fn render_network_detail(state: &MonitorState, cx: &App) -> impl IntoElement {
    let value_color = cx.theme().muted_foreground;

    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    if let Some(network) = state.current_network() {
        div()
            .flex()
            .flex_col()
            .gap_3()
            // Global TCP section
            .child(render_detail_section(
                "TCP 连接统计",
                vec![
                    ("总连接数", network.global.tcp_connections.to_string()),
                    ("ESTABLISHED", network.global.tcp_established.to_string()),
                    ("LISTEN", network.global.tcp_listen.to_string()),
                    ("TIME_WAIT", network.global.tcp_time_wait.to_string()),
                ],
                cx,
            ))
            // Interfaces
            .children(network.interfaces.iter().map(|iface| {
                render_detail_section(
                    &format!("网卡: {}", iface.name),
                    vec![
                        ("MAC", iface.mac_address.clone()),
                        ("IP", iface.ip_addresses.join(", ")),
                        ("状态", if iface.is_up { "UP" } else { "DOWN" }.to_string()),
                        (
                            "接收",
                            format!(
                                "{} ({} pkts)",
                                format_bytes(iface.rx_bytes),
                                iface.rx_packets
                            ),
                        ),
                        (
                            "发送",
                            format!(
                                "{} ({} pkts)",
                                format_bytes(iface.tx_bytes),
                                iface.tx_packets
                            ),
                        ),
                        (
                            "错误",
                            format!("RX: {} / TX: {}", iface.rx_errors, iface.tx_errors),
                        ),
                    ],
                    cx,
                )
            }))
    } else {
        div()
            .text_sm()
            .text_color(value_color)
            .child(crate::i18n::t(&lang, "monitor.no_data"))
    }
}

/// Render disk info detail
fn render_disk_detail(state: &MonitorState, cx: &App) -> impl IntoElement {
    let value_color = cx.theme().muted_foreground;

    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    if let Some(disk) = state.disk_info.as_ref() {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(disk.disks.iter().map(|d| {
                render_detail_section(
                    &format!("{} → {}", d.device, d.mount_point),
                    vec![
                        ("文件系统", d.fs_type.clone()),
                        ("总容量", format_bytes(d.total_bytes)),
                        (
                            "已使用",
                            format!("{} ({:.1}%)", format_bytes(d.used_bytes), d.usage_percent),
                        ),
                        ("可用", format_bytes(d.available_bytes)),
                        ("inode 总计", d.inodes_total.to_string()),
                        ("inode 已用", d.inodes_used.to_string()),
                        ("inode 可用", d.inodes_available.to_string()),
                    ],
                    cx,
                )
            }))
    } else {
        div()
            .text_sm()
            .text_color(value_color)
            .child(crate::i18n::t(&lang, "monitor.no_data"))
    }
}

/// Render a detail section with title and key-value pairs
fn render_detail_section(title: &str, items: Vec<(&str, String)>, cx: &App) -> impl IntoElement {
    let section_bg = cx.theme().secondary;
    let title_color = cx.theme().foreground;
    let label_color = cx.theme().muted_foreground;
    let value_color = cx.theme().foreground;

    div()
        .w_full()
        .bg(section_bg)
        .rounded(px(6.))
        .overflow_hidden()
        // Title
        .child(
            div()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(title_color)
                        .child(title.to_string()),
                ),
        )
        // Items
        .child(
            div()
                .px_3()
                .py_2()
                .flex()
                .flex_col()
                .gap_1()
                .children(items.into_iter().map(|(label, value)| {
                    div()
                        .flex()
                        .justify_between()
                        .child(
                            div()
                                .text_xs()
                                .text_color(label_color)
                                .child(label.to_string()),
                        )
                        .child(div().text_xs().text_color(value_color).child(value))
                })),
        )
}

/// Render process table
fn render_process_table(
    title: &str,
    processes: &[crate::models::monitor::ProcessInfo],
    cx: &App,
) -> impl IntoElement {
    let section_bg = cx.theme().secondary;
    let title_color = cx.theme().foreground;
    let label_color = cx.theme().muted_foreground;
    let value_color = cx.theme().foreground;

    div()
        .w_full()
        .bg(section_bg)
        .rounded(px(6.))
        .overflow_hidden()
        // Title
        .child(
            div()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(title_color)
                        .child(title.to_string()),
                ),
        )
        // Header
        .child(
            div()
                .px_3()
                .py_1()
                .flex()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .w(px(50.))
                        .text_xs()
                        .text_color(label_color)
                        .child("PID"),
                )
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(label_color)
                        .child("Name"),
                )
                .child(
                    div()
                        .w(px(50.))
                        .text_xs()
                        .text_color(label_color)
                        .child("CPU%"),
                )
                .child(
                    div()
                        .w(px(50.))
                        .text_xs()
                        .text_color(label_color)
                        .child("MEM%"),
                )
                .child(
                    div()
                        .w(px(60.))
                        .text_xs()
                        .text_color(label_color)
                        .child("User"),
                ),
        )
        // Rows
        .child(
            div()
                .px_3()
                .py_1()
                .flex()
                .flex_col()
                .children(processes.iter().take(5).map(|p| {
                    div()
                        .flex()
                        .py_1()
                        .child(
                            div()
                                .w(px(50.))
                                .text_xs()
                                .text_color(value_color)
                                .child(p.pid.to_string()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(value_color)
                                .overflow_hidden()
                                .child(p.name.clone()),
                        )
                        .child(
                            div()
                                .w(px(50.))
                                .text_xs()
                                .text_color(value_color)
                                .child(format!("{:.1}", p.cpu_percent)),
                        )
                        .child(
                            div()
                                .w(px(50.))
                                .text_xs()
                                .text_color(value_color)
                                .child(format!("{:.1}", p.memory_percent)),
                        )
                        .child(
                            div()
                                .w(px(60.))
                                .text_xs()
                                .text_color(label_color)
                                .child(p.user.clone()),
                        )
                })),
        )
}

/// Format bytes to human readable
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format uptime seconds to readable string
fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let mins = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}天 {}小时 {}分钟", days, hours, mins)
    } else if hours > 0 {
        format!("{}小时 {}分钟", hours, mins)
    } else {
        format!("{}分钟", mins)
    }
}
