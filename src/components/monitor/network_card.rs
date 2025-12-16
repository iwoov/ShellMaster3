// 网络状态区块组件

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
        .py_3()
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
                        .text_sm()
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
                .h(px(100.)) // 图表高度
                .bg(chart_bg)
                .rounded(px(6.))
                .flex()
                .items_center()
                .justify_center()
                // 图表占位 - 后续集成实际图表
                .child(render_network_chart_placeholder(has_interfaces, cx)),
        )
}

/// 渲染网速图表占位区域（后续替换为真实图表）
fn render_network_chart_placeholder(_has_data: bool, cx: &App) -> impl IntoElement {
    let muted_color = cx.theme().muted_foreground;
    let chart_line_color = hsla(145.0 / 360.0, 0.6, 0.5, 0.6); // 绿色 TX
    let chart_line_color_rx = hsla(210.0 / 360.0, 0.8, 0.6, 0.6); // 蓝色 RX

    // 模拟简单的折线图占位
    div()
        .size_full()
        .p_2()
        .flex()
        .flex_col()
        .gap_1()
        // 模拟图表线条
        .child(
            div()
                .flex_1()
                .w_full()
                .flex()
                .flex_col()
                .justify_end()
                .gap_1()
                // RX 线（模拟）
                .child(div().w_full().h(px(1.)).bg(chart_line_color_rx))
                // TX 线（模拟）
                .child(div().w_full().h(px(1.)).bg(chart_line_color)),
        )
        // 底部：速率显示
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
                        .child(
                            div()
                                .w(px(8.))
                                .h(px(2.))
                                .bg(chart_line_color_rx)
                                .rounded_full(),
                        )
                        .child(div().text_xs().text_color(muted_color).child("↓ RX: 0 B/s")),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .w(px(8.))
                                .h(px(2.))
                                .bg(chart_line_color)
                                .rounded_full(),
                        )
                        .child(div().text_xs().text_color(muted_color).child("↑ TX: 0 B/s")),
                ),
        )
}
