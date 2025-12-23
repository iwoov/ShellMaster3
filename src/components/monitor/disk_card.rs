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
        .pt_2()
        // 最后一个区块不需要底边框
        .flex()
        .flex_col()
        .gap_2()
        // 标题行
        .child(
            div().flex().items_center().gap_2().child(
                div()
                    .text_xs()
                    .font_medium()
                    .text_color(title_color)
                    .child(crate::i18n::t(&lang, "monitor.disk")),
            ), // 详情按钮 - 磁盘无需弹窗
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
                        .px_2()
                        .py_2()
                        .bg(cx.theme().secondary)
                        .rounded(px(6.))
                        .flex()
                        .flex_col()
                        .gap_1()
                        // 第一行：路径和挂载点
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_bold()
                                        .text_color(foreground)
                                        .child(disk.mount_point.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted_color)
                                        .child(format!("( {} )", disk.device)),
                                ),
                        )
                        // 第二行：进度条（带内嵌文字）
                        .child(
                            div()
                                .w_full()
                                .h(px(20.)) // 更粗的进度条
                                .bg(cx.theme().muted)
                                .rounded(px(4.))
                                .overflow_hidden()
                                .relative()
                                // 进度填充
                                .child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .top_0()
                                        .h_full()
                                        .w(relative(disk.usage_percent / 100.0))
                                        .bg(if disk.usage_percent > 90.0 {
                                            hsla(0.0, 0.8, 0.5, 1.0) // 红色警告
                                        } else if disk.usage_percent > 75.0 {
                                            hsla(40.0 / 360.0, 0.8, 0.5, 1.0) // 橙色警告
                                        } else {
                                            title_color // 正常蓝色
                                        })
                                        .rounded(px(4.)),
                                )
                                // 文字层（左侧百分比，右侧容量）
                                .child(
                                    div()
                                        .absolute()
                                        .inset_0()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .px_2()
                                        // 左侧：百分比
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_medium()
                                                .text_color(foreground)
                                                .child(format!("{:.1}%", disk.usage_percent)),
                                        )
                                        // 右侧：容量
                                        .child(div().text_xs().text_color(muted_color).child(
                                            format!(
                                                "{} / {}",
                                                format_bytes(disk.used_bytes),
                                                format_bytes(disk.total_bytes)
                                            ),
                                        )),
                                ),
                        )
                })),
        )
}

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
