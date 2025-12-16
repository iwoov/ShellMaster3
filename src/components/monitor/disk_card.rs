// 磁盘状态区块组件

use gpui::*;
use gpui_component::{ActiveTheme, StyledExt};

use crate::models::monitor::MonitorState;

/// 渲染磁盘状态区块（无卡片边框，最后一个区块无底部边框）
pub fn render_disk_card(state: &MonitorState, cx: &App) -> impl IntoElement {
    let title_color = hsla(210.0 / 360.0, 1.0, 0.5, 1.0); // 蓝色标题
    let foreground = cx.theme().foreground;
    let muted_color = cx.theme().muted_foreground;

    // 获取语言设置
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    // 获取磁盘信息
    let disks = state
        .disk_info
        .as_ref()
        .map(|d| &d.disks)
        .cloned()
        .unwrap_or_default();

    div()
        .w_full()
        .pt_3()
        // 最后一个区块不需要底边框
        .flex()
        .flex_col()
        .gap_2()
        // 标题
        .child(
            div()
                .text_sm()
                .font_medium()
                .text_color(title_color)
                .child(crate::i18n::t(&lang, "monitor.disk")),
        )
        // 内容区域
        .child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_1()
                .children(disks.into_iter().map(|disk| {
                    div()
                        .w_full()
                        .px_3()
                        .py_2()
                        .bg(cx.theme().secondary)
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .text_color(foreground)
                                .child(disk.device.clone()),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(muted_color)
                                .child(disk.mount_point.clone()),
                        )
                })),
        )
}
