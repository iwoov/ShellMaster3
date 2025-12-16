// 网络状态区块组件

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme, StyledExt};

use crate::models::monitor::MonitorState;

/// 渲染网络状态区块（无卡片边框）
pub fn render_network_card(state: &MonitorState, cx: &App) -> impl IntoElement {
    let title_color = hsla(210.0 / 360.0, 1.0, 0.5, 1.0); // 蓝色标题
    let muted_color = cx.theme().muted_foreground;
    let border_color = cx.theme().border;

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
    let selected_name = interfaces
        .get(state.selected_interface_index)
        .cloned()
        .unwrap_or_else(|| crate::i18n::t(&lang, "monitor.network_interface").to_string());

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
                .child(crate::i18n::t(&lang, "monitor.network")),
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
                .gap_2()
                // 网络接口选择器（模拟下拉）
                .child(
                    div()
                        .w_full()
                        .px_3()
                        .py_2()
                        .bg(cx.theme().background)
                        .rounded(px(4.))
                        .border_1()
                        .border_color(border_color)
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .text_color(if has_interfaces {
                                    cx.theme().foreground
                                } else {
                                    muted_color
                                })
                                .child(selected_name),
                        )
                        // 下拉箭头
                        .child(
                            svg()
                                .path(crate::constants::icons::CHEVRON_DOWN)
                                .size(px(14.))
                                .text_color(muted_color),
                        ),
                )
                // 无接口提示
                .when(!has_interfaces, |this| {
                    this.child(
                        div()
                            .w_full()
                            .py_4()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted_color)
                                    .child(crate::i18n::t(&lang, "monitor.no_interfaces")),
                            ),
                    )
                }),
        )
}
