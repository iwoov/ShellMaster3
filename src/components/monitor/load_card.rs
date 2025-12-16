// 系统负载区块组件

use gpui::*;
use gpui_component::{ActiveTheme, StyledExt};

use crate::constants::icons;
use crate::models::monitor::MonitorState;

/// 渲染系统负载区块（无卡片边框）
pub fn render_load_card(state: &MonitorState, cx: &App) -> impl IntoElement {
    let title_color = hsla(210.0 / 360.0, 1.0, 0.5, 1.0); // 蓝色标题
    let label_color = cx.theme().foreground;
    let muted_color = cx.theme().muted_foreground;
    let green_color = hsla(145.0 / 360.0, 0.63, 0.42, 1.0); // 绿色进度条
    let border_color = cx.theme().border;

    // 获取语言设置
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    // 获取负载信息
    let (cpu_percent, memory_percent, network_rx, network_tx) =
        if let Some(load) = state.current_load() {
            let total_memory = state
                .system_info
                .as_ref()
                .map(|s| s.memory.total_bytes)
                .unwrap_or(1);
            let mem_percent = if total_memory > 0 {
                (load.memory.used_bytes as f32 / total_memory as f32) * 100.0
            } else {
                0.0
            };

            // 获取网络流量（从最新的网络信息）
            let (rx, tx) = if let Some(network) = state.current_network() {
                let iface = network.interfaces.first();
                if let Some(iface) = iface {
                    (iface.rx_bytes, iface.tx_bytes)
                } else {
                    (0, 0)
                }
            } else {
                (0, 0)
            };

            (load.cpu.usage_percent, mem_percent, rx, tx)
        } else {
            (0.0, 0.0, 0, 0)
        };

    div()
        .w_full()
        .py_3()
        .border_b_1()
        .border_color(border_color)
        .flex()
        .flex_col()
        .gap_2()
        // 标题
        .child(
            div()
                .text_sm()
                .font_medium()
                .text_color(title_color)
                .child(crate::i18n::t(&lang, "monitor.load")),
        )
        // 内容区域
        .child(
            div()
                .w_full()
                .px_3()
                .py_2()
                .bg(cx.theme().secondary)
                .rounded(px(6.))
                .flex()
                .flex_col()
                .gap_3()
                // CPU 使用率行
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        // CPU 图标
                        .child(svg().path(icons::CPU).size(px(18.)).text_color(green_color))
                        // 进度条
                        .child(render_progress_bar(cpu_percent, green_color, cx))
                        // 百分比文字
                        .child(
                            div()
                                .text_sm()
                                .text_color(label_color)
                                .child(format!("{:.1}%", cpu_percent)),
                        ),
                )
                // 内存使用率行
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        // 内存图标
                        .child(
                            svg()
                                .path(icons::MEMORY)
                                .size(px(18.))
                                .text_color(green_color),
                        )
                        // 进度条
                        .child(render_progress_bar(memory_percent, green_color, cx))
                        // 百分比文字
                        .child(
                            div()
                                .text_sm()
                                .text_color(label_color)
                                .child(format!("{:.1}%", memory_percent)),
                        )
                        // 网络流量
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted_color)
                                .ml_auto()
                                .child(format_bytes_pair(network_rx, network_tx)),
                        ),
                ),
        )
}

/// 渲染进度条
fn render_progress_bar(percent: f32, color: Hsla, cx: &App) -> impl IntoElement {
    let bg_color = cx.theme().border;
    let width_percent = percent.clamp(0.0, 100.0);

    div()
        .h(px(8.))
        .flex_1()
        .min_w(px(50.))
        .bg(bg_color)
        .rounded(px(4.))
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(relative(width_percent / 100.0))
                .bg(color)
                .rounded(px(4.)),
        )
}

/// 格式化字节对（接收/发送）
fn format_bytes_pair(rx: u64, tx: u64) -> String {
    format!("{}/{}", format_bytes(rx), format_bytes(tx))
}

/// 格式化字节数
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
