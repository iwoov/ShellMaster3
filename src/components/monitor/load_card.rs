// 系统负载区块组件

use gpui::*;
use gpui_component::{ActiveTheme, StyledExt};

use crate::constants::icons;
use crate::models::monitor::MonitorState;

/// 渲染系统负载区块（无卡片边框）
pub fn render_load_card(state: &MonitorState, cx: &App) -> impl IntoElement {
    let title_color = hsla(210.0 / 360.0, 1.0, 0.5, 1.0); // 蓝色标题
    let green_color = hsla(145.0 / 360.0, 0.63, 0.42, 1.0); // 绿色进度条
    let border_color = cx.theme().border;

    // 获取语言设置
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    // 获取负载信息
    let (cpu_percent, memory_percent, memory_used, memory_total) =
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

            (
                load.cpu.usage_percent,
                mem_percent,
                load.memory.used_bytes,
                total_memory,
            )
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
                        // 进度条（带内嵌文字）
                        .child(render_cpu_progress_bar(cpu_percent, green_color, cx)),
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
                        // 进度条（带内嵌文字：左侧百分比，右侧内存数值）
                        .child(render_memory_progress_bar(
                            memory_percent,
                            memory_used,
                            memory_total,
                            green_color,
                            cx,
                        )),
                ),
        )
}

/// 渲染 CPU 进度条（百分比显示在进度条内部左侧）
fn render_cpu_progress_bar(percent: f32, color: Hsla, cx: &App) -> impl IntoElement {
    let bg_color = cx.theme().border;
    let width_percent = percent.clamp(0.0, 100.0);
    let text_color = cx.theme().foreground;

    div()
        .h(px(20.)) // 更粗的进度条
        .flex_1()
        .min_w(px(80.))
        .bg(bg_color)
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
                .w(relative(width_percent / 100.0))
                .bg(color)
                .rounded(px(4.)),
        )
        // 百分比文字（左侧）
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .px_2()
                .child(
                    div()
                        .text_xs()
                        .font_medium()
                        .text_color(text_color)
                        .child(format!("{:.1}%", percent)),
                ),
        )
}

/// 渲染内存进度条（左侧百分比，右侧内存数值）
fn render_memory_progress_bar(
    percent: f32,
    used_bytes: u64,
    total_bytes: u64,
    color: Hsla,
    cx: &App,
) -> impl IntoElement {
    let bg_color = cx.theme().border;
    let width_percent = percent.clamp(0.0, 100.0);
    let text_color = cx.theme().foreground;
    let muted_color = cx.theme().muted_foreground;

    let memory_text = format!("{}/{}", format_bytes(used_bytes), format_bytes(total_bytes));

    div()
        .h(px(20.)) // 更粗的进度条
        .flex_1()
        .min_w(px(80.))
        .bg(bg_color)
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
                .w(relative(width_percent / 100.0))
                .bg(color)
                .rounded(px(4.)),
        )
        // 文字层（左侧百分比，右侧内存数值）
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
                        .text_color(text_color)
                        .child(format!("{:.1}%", percent)),
                )
                // 右侧：内存数值
                .child(div().text_xs().text_color(muted_color).child(memory_text)),
        )
}

/// 格式化字节数
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1}T", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}
