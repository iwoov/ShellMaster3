// Monitor detail dialog components

use gpui::*;
use gpui_component::scroll::ScrollableElement;
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
                // 阻止滚动事件穿透到底层内容
                .on_scroll_wheel(|_, _, cx| {
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
                        .min_h(px(0.))
                        .overflow_y_scrollbar()
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
                            DetailDialogType::None => div().into_any_element(),
                        }),
                ),
        )
        .into_any_element()
}

/// Render system info detail(System)
/// 基础信息: ip_address, hostname, os_version, uptime
/// CPU: cpu_info, cpu_cores, physical_cores, architecture
/// 内存: memory_total, swap_total
fn render_system_detail(state: &MonitorState, cx: &App) -> impl IntoElement {
    let value_color = cx.theme().muted_foreground;

    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    if let Some(info) = &state.system_info {
        div()
            .flex()
            .flex_col()
            .gap_3()
            // 基础信息
            .child(render_detail_section(
                crate::i18n::t(&lang, "monitor.detail.basic_info"),
                vec![
                    (
                        crate::i18n::t(&lang, "monitor.detail.host_address"),
                        info.host.address.clone(),
                    ),
                    (
                        crate::i18n::t(&lang, "monitor.detail.hostname"),
                        info.host.hostname.clone(),
                    ),
                    (
                        crate::i18n::t(&lang, "monitor.detail.os"),
                        info.host.os.clone(),
                    ),
                    (
                        crate::i18n::t(&lang, "monitor.detail.kernel"),
                        info.host.kernel.clone(),
                    ),
                    (
                        crate::i18n::t(&lang, "monitor.detail.uptime"),
                        format_uptime(info.host.uptime_seconds, &lang),
                    ),
                ],
                cx,
            ))
            // CPU
            .child(render_detail_section(
                crate::i18n::t(&lang, "monitor.detail.cpu"),
                vec![
                    (
                        crate::i18n::t(&lang, "monitor.detail.cpu_model"),
                        info.cpu.model.clone(),
                    ),
                    (
                        crate::i18n::t(&lang, "monitor.detail.cores_logical"),
                        info.cpu.cores_logical.to_string(),
                    ),
                    (
                        crate::i18n::t(&lang, "monitor.detail.cores_physical"),
                        info.cpu.cores_physical.to_string(),
                    ),
                    (
                        crate::i18n::t(&lang, "monitor.detail.architecture"),
                        info.cpu.architecture.clone(),
                    ),
                ],
                cx,
            ))
            // 内存
            .child(render_detail_section(
                crate::i18n::t(&lang, "monitor.detail.memory"),
                vec![
                    (
                        crate::i18n::t(&lang, "monitor.detail.memory_total"),
                        format_bytes(info.memory.total_bytes),
                    ),
                    (
                        crate::i18n::t(&lang, "monitor.detail.swap_total"),
                        format_bytes(info.memory.swap_total_bytes),
                    ),
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

/// Render load info detail(Process)
/// CPU 占用最高的进程: name, usage, pid
/// 内存占用最高的进程: name, usage, pid
/// 内存详情: Buffers, Cached, Swap使用情况
fn render_load_detail(state: &MonitorState, cx: &App) -> impl IntoElement {
    let value_color = cx.theme().muted_foreground;

    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    if let Some(load) = state.current_load() {
        let swap_total = state
            .system_info
            .as_ref()
            .map(|s| s.memory.swap_total_bytes)
            .unwrap_or(0);
        let swap_usage_str = if swap_total > 0 {
            format!(
                "{}/{}",
                format_bytes(load.memory.swap_used_bytes),
                format_bytes(swap_total)
            )
        } else {
            "0 B / 0 B".to_string()
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            // Top CPU processes
            .child(render_process_table(
                crate::i18n::t(&lang, "monitor.detail.top_cpu_processes"),
                &load.top_cpu_processes,
                true, // is_cpu
                &lang,
                cx,
            ))
            // Top Memory processes
            .child(render_process_table(
                crate::i18n::t(&lang, "monitor.detail.top_mem_processes"),
                &load.top_memory_processes,
                false, // is_memory
                &lang,
                cx,
            ))
            // Memory details
            .child(render_detail_section(
                crate::i18n::t(&lang, "monitor.detail.memory_detail"),
                vec![
                    ("Buffers", format_bytes(load.memory.buffers_bytes)),
                    ("Cached", format_bytes(load.memory.cached_bytes)),
                    (
                        crate::i18n::t(&lang, "monitor.detail.swap_usage"),
                        swap_usage_str,
                    ),
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

/// Render network info detail(Network)
/// 全局网络状态: tcp_established
/// 接口列表: name, ip_address, mac_address, rx_bytes, tx_bytes
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
            // Global Network Status
            .child(render_detail_section(
                crate::i18n::t(&lang, "monitor.detail.global_network"),
                vec![(
                    crate::i18n::t(&lang, "monitor.detail.tcp_connections"),
                    network.global.tcp_established.to_string(),
                )],
                cx,
            ))
            // Interface list
            .children(network.interfaces.iter().map(|iface| {
                render_detail_section(
                    &iface.name,
                    vec![
                        (
                            crate::i18n::t(&lang, "monitor.detail.ip_address"),
                            iface.ip_addresses.join(", "),
                        ),
                        (
                            crate::i18n::t(&lang, "monitor.detail.mac_address"),
                            iface.mac_address.clone(),
                        ),
                        (
                            crate::i18n::t(&lang, "monitor.detail.rx_bytes"),
                            format_bytes(iface.rx_bytes),
                        ),
                        (
                            crate::i18n::t(&lang, "monitor.detail.tx_bytes"),
                            format_bytes(iface.tx_bytes),
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

/// Render process table with simple columns
/// 进程名 (name), CPU/内存使用率 (usage), PID (pid)
fn render_process_table(
    title: &str,
    processes: &[crate::models::monitor::ProcessInfo],
    is_cpu: bool,
    lang: &crate::models::settings::Language,
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
                        .flex_1()
                        .text_xs()
                        .text_color(label_color)
                        .child(crate::i18n::t(lang, "monitor.detail.process_name")),
                )
                .child(
                    div()
                        .w(px(60.))
                        .text_xs()
                        .text_color(label_color)
                        .child(if is_cpu { "CPU%" } else { "MEM%" }),
                )
                .child(
                    div()
                        .w(px(50.))
                        .text_xs()
                        .text_color(label_color)
                        .child("PID"),
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
                                .flex_1()
                                .text_xs()
                                .text_color(value_color)
                                .overflow_hidden()
                                .child(p.name.clone()),
                        )
                        .child(
                            div()
                                .w(px(60.))
                                .text_xs()
                                .text_color(value_color)
                                .child(format!(
                                    "{:.1}%",
                                    if is_cpu {
                                        p.cpu_percent
                                    } else {
                                        p.memory_percent
                                    }
                                )),
                        )
                        .child(
                            div()
                                .w(px(50.))
                                .text_xs()
                                .text_color(value_color)
                                .child(p.pid.to_string()),
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
fn format_uptime(seconds: u64, lang: &crate::models::settings::Language) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let mins = (seconds % 3600) / 60;

    let day_unit = crate::i18n::t(lang, "monitor.detail.uptime_days");
    let hour_unit = crate::i18n::t(lang, "monitor.detail.uptime_hours");
    let min_unit = crate::i18n::t(lang, "monitor.detail.uptime_minutes");

    if days > 0 {
        format!(
            "{}{} {}{} {}{}",
            days, day_unit, hours, hour_unit, mins, min_unit
        )
    } else if hours > 0 {
        format!("{}{} {}{}", hours, hour_unit, mins, min_unit)
    } else {
        format!("{}{}", mins, min_unit)
    }
}
