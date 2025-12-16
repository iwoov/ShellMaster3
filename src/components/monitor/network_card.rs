// 网络状态区块组件

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme, StyledExt};

use crate::models::monitor::MonitorState;

use super::detail_dialog::{render_detail_button, DetailDialogState, DetailDialogType};

/// 渲染网络状态区块（无卡片边框）
pub fn render_network_card(
    state: &MonitorState,
    dialog_state: Entity<DetailDialogState>,
    cx: &App,
) -> impl IntoElement {
    let title_color = hsla(210.0 / 360.0, 1.0, 0.5, 1.0); // 蓝色标题
    let label_color = cx.theme().foreground;
    let muted_color = cx.theme().muted_foreground;
    let border_color = cx.theme().border;
    let chart_bg = cx.theme().secondary;

    // 获取语言设置
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    // 获取网络接口列表
    let interfaces: Vec<String> = state
        .current_network()
        .map(|n| n.interfaces.iter().map(|i| i.name.clone()).collect())
        .unwrap_or_default();

    let has_interfaces = !interfaces.is_empty();
    // 下拉框显示的是网卡名称，无数据时显示占位符
    let selected_interface_name = interfaces
        .get(state.selected_interface_index)
        .cloned()
        .unwrap_or_else(|| "--".to_string());

    div()
        .w_full()
        .py_2()
        .border_b_1()
        .border_color(border_color)
        .flex()
        .flex_col()
        .gap_2()
        // 标题行
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_medium()
                        .text_color(title_color)
                        .child(crate::i18n::t(&lang, "monitor.network")),
                )
                // 详情按钮
                .child(render_detail_button(
                    dialog_state,
                    DetailDialogType::NetworkInfo,
                    cx,
                )),
        )
        // 网络接口选择行：标签 + 下拉框
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                // 左侧标签 "网络接口:"
                .child(
                    div()
                        .text_xs()
                        .text_color(label_color)
                        .child(crate::i18n::t(&lang, "monitor.network_interface")),
                )
                // 右侧下拉框（显示网卡名称）
                .child(
                    div()
                        .w(px(140.)) // 固定宽度
                        .px_2()
                        .py_1()
                        .bg(cx.theme().background)
                        .rounded(px(4.))
                        .border_1()
                        .border_color(border_color)
                        .flex()
                        .items_center()
                        .justify_between()
                        .cursor_pointer()
                        .hover(|s| s.border_color(cx.theme().primary))
                        .child(
                            div()
                                .text_xs()
                                .overflow_hidden()
                                .text_color(if has_interfaces {
                                    cx.theme().foreground
                                } else {
                                    muted_color
                                })
                                .child(selected_interface_name),
                        )
                        // 下拉箭头
                        .child(
                            svg()
                                .path(crate::constants::icons::CHEVRON_DOWN)
                                .size(px(12.))
                                .text_color(muted_color),
                        ),
                ),
        )
        // 图表区域（网速图表 - 始终显示）
        .child(
            div()
                .w_full()
                .h(px(120.)) // 图表高度
                .bg(chart_bg)
                .rounded(px(6.))
                .flex()
                .items_center()
                .justify_center()
                // 图表占位 - 后续集成实际图表
                .child(render_network_chart(state, cx)),
        )
}

/// 网速数据点结构（用于 Chart）
#[derive(Clone)]
struct SpeedPoint {
    time_offset: i64, // 相对于最新数据点的时间偏移（秒，负数表示更早）
    rx_speed: f64,
    tx_speed: f64,
}

/// 渲染网速图表和速率显示
fn render_network_chart(state: &MonitorState, cx: &App) -> impl IntoElement {
    let muted_color = cx.theme().muted_foreground;
    let chart_line_color_tx = hsla(145.0 / 360.0, 0.6, 0.5, 0.8); // 绿色 TX
    let chart_line_color_rx = hsla(210.0 / 360.0, 0.8, 0.6, 0.8); // 蓝色 RX

    // 获取当前网速
    let (rx_speed, tx_speed) = state.current_speed();
    let rx_speed_str = MonitorState::format_speed(rx_speed);
    let tx_speed_str = MonitorState::format_speed(tx_speed);

    // 获取最新时间戳
    let latest_timestamp = state.speed_history.back().map(|s| s.timestamp).unwrap_or(0);

    // 获取速度历史并转换为 Chart 需要的数据格式
    let chart_data: Vec<SpeedPoint> = state
        .speed_history
        .iter()
        .map(|s| SpeedPoint {
            time_offset: s.timestamp as i64 - latest_timestamp as i64,
            rx_speed: s.rx_speed,
            tx_speed: s.tx_speed,
        })
        .collect();

    // 计算最大速度
    let max_speed = state
        .speed_history
        .iter()
        .flat_map(|s| [s.rx_speed, s.tx_speed])
        .fold(0.0_f64, f64::max);
    let _max_speed_str = MonitorState::format_speed(max_speed);

    let has_data = !chart_data.is_empty() && chart_data.len() >= 2;

    div()
        .size_full()
        .p_1()
        .flex()
        .flex_col()
        .gap_1()
        // 图表区域
        .child(
            div()
                .flex_1()
                .w_full()
                .when(has_data, |s| {
                    s.child(
                        gpui_component::chart::AreaChart::new(chart_data)
                            .x(|d| format!("{}s", d.time_offset))
                            .y(|d| d.rx_speed) // RX
                            .y(|d| d.tx_speed) // TX
                            .stroke(chart_line_color_rx)
                            .stroke(chart_line_color_tx)
                            .fill(chart_line_color_rx.opacity(0.3))
                            .fill(chart_line_color_tx.opacity(0.3))
                            .linear()
                            .linear()
                            .tick_margin(5),
                    )
                })
                .when(!has_data, |s| {
                    s.flex().items_center().justify_center().child(
                        div()
                            .text_xs()
                            .text_color(muted_color)
                            .child("Collecting data..."),
                    )
                }),
        )
        // 底部：速率数值显示
        .child(
            div()
                .w_full()
                .flex()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(div().text_xs().text_color(chart_line_color_rx).child("↓"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted_color)
                                .child(format!("RX: {}", rx_speed_str)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(div().text_xs().text_color(chart_line_color_tx).child("↑"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted_color)
                                .child(format!("TX: {}", tx_speed_str)),
                        ),
                ),
        )
}
