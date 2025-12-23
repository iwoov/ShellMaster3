// 网络状态区块组件

use gpui::*;
use gpui_component::button::Button;
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::{ActiveTheme, StyledExt};

use crate::models::monitor::MonitorState;
use crate::state::SessionState;

use super::detail_dialog::{render_detail_button, DetailDialogState, DetailDialogType};

/// 渲染网络状态区块（无卡片边框）
pub fn render_network_card(
    state: &MonitorState,
    dialog_state: Entity<DetailDialogState>,
    session_state: Entity<SessionState>,
    tab_id: String,
    cx: &App,
) -> impl IntoElement {
    let title_color = hsla(210.0 / 360.0, 1.0, 0.5, 1.0); // 蓝色标题
    let label_color = cx.theme().foreground;
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
    let selected_index = state.selected_interface_index;
    // 下拉框显示的是网卡名称，无数据时显示占位符
    let selected_interface_name = interfaces
        .get(selected_index)
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
                // 右侧下拉框
                .child(if has_interfaces {
                    render_interface_dropdown(
                        selected_interface_name,
                        interfaces,
                        session_state,
                        tab_id,
                        cx,
                    )
                    .into_any_element()
                } else {
                    // 无接口时显示静态占位符
                    render_static_dropdown("--", cx).into_any_element()
                }),
        )
        // 速率数据显示
        .child(
            div()
                .w_full()
                .bg(chart_bg)
                .rounded(px(6.))
                .p_2()
                .flex()
                .flex_col()
                .gap_2()
                // 历史趋势图
                .child(render_simple_chart(state))
                // 实时数值
                .child(render_network_speed(state, cx)),
        )
}

/// 渲染网络接口下拉选择器
fn render_interface_dropdown(
    selected_name: String,
    interfaces: Vec<String>,
    session_state: Entity<SessionState>,
    tab_id: String,
    cx: &App,
) -> impl IntoElement {
    use gpui::Corner;

    let muted_color = cx.theme().muted_foreground;

    Button::new("network-interface-dropdown")
        .w(px(140.))
        .h(px(28.))
        .outline()
        .justify_start()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .w(px(120.))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground)
                        .overflow_hidden()
                        .child(selected_name),
                )
                .child(
                    svg()
                        .path(crate::constants::icons::CHEVRON_DOWN)
                        .size(px(12.))
                        .text_color(muted_color),
                ),
        )
        .dropdown_menu_with_anchor(Corner::TopLeft, move |menu, _, _| {
            let mut menu = menu.min_w(px(140.));
            for (idx, iface_name) in interfaces.iter().enumerate() {
                let name: SharedString = iface_name.clone().into();
                let session_for_click = session_state.clone();
                let tab_id_for_click = tab_id.clone();
                menu = menu.item(PopupMenuItem::new(name).on_click(move |_, _, cx| {
                    session_for_click.update(cx, |state, _| {
                        if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_for_click)
                        {
                            tab.monitor_state.selected_interface_index = idx;
                            // 清空速度历史以便重新计算新接口的速度
                            tab.monitor_state.speed_history.clear();
                        }
                    });
                }));
            }
            menu
        })
}

/// 渲染静态下拉框（无接口时使用）
fn render_static_dropdown(text: &'static str, cx: &App) -> impl IntoElement {
    let border_color = cx.theme().border;
    let muted_color = cx.theme().muted_foreground;

    div()
        .w(px(140.))
        .px_2()
        .py_1()
        .bg(cx.theme().background)
        .rounded(px(4.))
        .border_1()
        .border_color(border_color)
        .flex()
        .items_center()
        .justify_between()
        .child(div().text_xs().text_color(muted_color).child(text))
        .child(
            svg()
                .path(crate::constants::icons::CHEVRON_DOWN)
                .size(px(12.))
                .text_color(muted_color),
        )
}

/// 渲染简易柱状图（模拟历史趋势）
fn render_simple_chart(state: &MonitorState) -> impl IntoElement {
    let chart_line_color_tx = hsla(145.0 / 360.0, 0.6, 0.5, 0.8); // 绿色 TX
    let chart_line_color_rx = hsla(210.0 / 360.0, 0.8, 0.6, 0.8); // 蓝色 RX

    // 计算最大速度用于归一化
    let max_speed = state
        .speed_history
        .iter()
        .flat_map(|s| [s.rx_speed, s.tx_speed])
        .fold(0.0_f64, f64::max)
        .max(1024.0); // 至少 1KB/s 以避免除以0或过分放大微小波动

    let container_height = 100.0_f32;

    div()
        .w_full()
        .h(px(container_height)) // 固定高度
        .overflow_hidden() // 防止超出
        .flex()
        .items_end()
        .justify_end() // 靠右对齐，紧凑排列
        .gap(px(1.))
        .children(state.speed_history.iter().map(|s| {
            let rx_px = (s.rx_speed as f32 / max_speed as f32 * container_height)
                .clamp(0.0, container_height);
            let tx_px = (s.tx_speed as f32 / max_speed as f32 * container_height)
                .clamp(0.0, container_height);

            div()
                .h_full()
                .flex()
                .items_end()
                .justify_center()
                .gap(px(1.))
                // RX Bar
                .child(
                    div()
                        .w(px(3.)) // 固定宽度 3px 让柱子变细
                        // 最小高度 2px 以便可见
                        .h(px(rx_px))
                        .min_h(px(2.))
                        .bg(chart_line_color_rx)
                        .rounded(px(1.)),
                )
                // TX Bar
                .child(
                    div()
                        .w(px(3.))
                        .h(px(tx_px))
                        .min_h(px(2.))
                        .bg(chart_line_color_tx)
                        .rounded(px(1.)),
                )
        }))
}

/// 渲染网速数值显示
fn render_network_speed(state: &MonitorState, cx: &App) -> impl IntoElement {
    let muted_color = cx.theme().muted_foreground;
    let chart_line_color_tx = hsla(145.0 / 360.0, 0.6, 0.5, 0.8); // 绿色 TX
    let chart_line_color_rx = hsla(210.0 / 360.0, 0.8, 0.6, 0.8); // 蓝色 RX

    // 获取当前网速
    let (rx_speed, tx_speed) = state.current_speed();
    let rx_speed_str = MonitorState::format_speed(rx_speed);
    let tx_speed_str = MonitorState::format_speed(tx_speed);

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
        )
}
