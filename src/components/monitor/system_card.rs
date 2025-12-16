// 系统信息区块组件

use gpui::*;
use gpui_component::{ActiveTheme, StyledExt};

use crate::constants::icons;
use crate::models::monitor::MonitorState;

use super::detail_dialog::{render_detail_button, DetailDialogState, DetailDialogType};

/// 渲染系统信息区块（无卡片边框）
pub fn render_system_card(
    state: &MonitorState,
    dialog_state: Entity<DetailDialogState>,
    cx: &App,
) -> impl IntoElement {
    let title_color = hsla(210.0 / 360.0, 1.0, 0.5, 1.0); // 蓝色标题
    let label_color = cx.theme().foreground;
    let muted_color = cx.theme().muted_foreground;
    let border_color = cx.theme().border;

    // 获取语言设置
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    // 获取系统信息
    let (host_address, os_info, uptime_text) = if let Some(info) = &state.system_info {
        let uptime = format_uptime(info.host.uptime_seconds);
        (info.host.address.clone(), info.host.os.clone(), uptime)
    } else {
        (
            "获取中...".to_string(),
            "获取中...".to_string(),
            "0s".to_string(),
        )
    };

    let host_address_for_copy = host_address.clone();

    div()
        .w_full()
        .pb_3()
        .border_b_1()
        .border_color(border_color)
        .flex()
        .flex_col()
        .gap_2()
        // 标题行：系统信息 + 详情按钮
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
                        .child(crate::i18n::t(&lang, "monitor.system_info")),
                )
                // 详情按钮
                .child(render_detail_button(
                    dialog_state,
                    DetailDialogType::SystemInfo,
                    cx,
                )),
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
                // 主机地址行
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(label_color)
                                        .child(crate::i18n::t(&lang, "monitor.host_address")),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(label_color)
                                        .child(host_address.clone()),
                                ),
                        )
                        // 复制按钮
                        .child(
                            div()
                                .id("copy-host-address")
                                .cursor_pointer()
                                .rounded(px(4.))
                                .p_1()
                                .hover(|s| s.bg(cx.theme().list_active))
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        host_address_for_copy.clone(),
                                    ));
                                })
                                .child(
                                    svg()
                                        .path(icons::COPY)
                                        .size(px(14.))
                                        .text_color(muted_color),
                                ),
                        ),
                )
                // 操作系统行
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .text_color(label_color)
                                .child(crate::i18n::t(&lang, "monitor.os")),
                        )
                        .child(div().text_sm().text_color(muted_color).child(os_info)),
                )
                // 运行时间行
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .text_color(label_color)
                                .child(crate::i18n::t(&lang, "monitor.uptime")),
                        )
                        .child(div().text_sm().text_color(label_color).child(uptime_text)),
                ),
        )
}

/// 格式化运行时间
fn format_uptime(seconds: u64) -> String {
    if seconds == 0 {
        return "0s".to_string();
    }

    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}m", minutes));
    }
    if secs > 0 || parts.is_empty() {
        parts.push(format!("{}s", secs));
    }

    parts.join(" ")
}
