// 监控面板「负载」组：CPU（数值 + 一分钟历史曲线）、内存和交换分区（数值 + 进度条），标题右侧是平均负载

use gpui::*;

use super::detail_dialog::DetailDialogType;
use super::view::{chart_well, metric_row, mono_value, section, MonitorCtx};
use crate::components::common::button::text_tooltip;
use crate::components::common::format::{format_percent, format_usage};
use crate::components::common::panel::{meter_bar, sparkline, usage_color, SparkSeries};
use crate::models::monitor::MonitorState;

/// CPU 曲线的高度
const CPU_CHART_HEIGHT: Pixels = px(32.);

/// 渲染「负载」组
pub(super) fn render_load_section(ctx: &MonitorCtx) -> impl IntoElement {
    let p = &ctx.p;
    let load = ctx.state.current_load();
    let memory_total = ctx
        .state
        .system_info
        .as_ref()
        .map(|info| info.memory.total_bytes)
        .unwrap_or(0);
    let swap_total = ctx
        .state
        .system_info
        .as_ref()
        .map(|info| info.memory.swap_total_bytes)
        .unwrap_or(0);

    // 标题右侧：1 / 5 / 15 分钟平均负载
    let load_average = load.map(|load| {
        let [one, five, fifteen] = load.cpu.load_average;
        div()
            .id("monitor-load-average")
            .child(mono_value(
                format!("{one:.2}  {five:.2}  {fifteen:.2}"),
                11.,
                p.text_faint,
                ctx,
            ))
            .tooltip(text_tooltip(ctx.t("monitor.load_average")))
            .into_any_element()
    });

    let cpu = load.and_then(|load| load.cpu.usage_percent);
    // 远端不是 Linux 时拿不到内存用量，显示「—」
    let memory_load = load.and_then(|load| load.memory.as_ref());
    let memory = memory_load.map(|memory| (memory.used_bytes, memory_total));
    let swap = (swap_total > 0).then(|| {
        (
            memory_load
                .map(|memory| memory.swap_used_bytes)
                .unwrap_or(0),
            swap_total,
        )
    });

    section(
        "monitor-load",
        ctx.t("monitor.section.load"),
        load_average,
        Some(DetailDialogType::LoadInfo),
        ctx,
    )
    .child(
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(metric_row(
                ctx.t("monitor.label.cpu"),
                mono_value(
                    cpu.map(format_percent).unwrap_or_else(|| "—".into()),
                    12.,
                    ctx.value_color(),
                    ctx,
                ),
                ctx,
            ))
            .child(render_cpu_chart(ctx.state, cpu, ctx)),
    )
    .child(render_usage(ctx.t("monitor.label.memory"), memory, ctx))
    .children(swap.map(|swap| render_usage(ctx.t("monitor.label.swap"), Some(swap), ctx)))
}

/// CPU 一分钟历史曲线，颜色跟随当前使用率
fn render_cpu_chart(state: &MonitorState, current: Option<f32>, ctx: &MonitorCtx) -> Div {
    let color = if ctx.stale {
        ctx.p.text_faint
    } else {
        usage_color(current.unwrap_or(0.), &ctx.p)
    };
    chart_well(CPU_CHART_HEIGHT, ctx).child(sparkline(
        vec![SparkSeries {
            values: state.cpu_history(),
            stroke: color,
            fill: Some(color.opacity(0.16)),
        }],
        100.,
        MonitorState::MAX_HISTORY_SIZE,
        CPU_CHART_HEIGHT - px(2.),
    ))
}

/// 用量：标签、「已用 / 总量」和百分比，下面一条按阈值变色的进度条
fn render_usage(label: &'static str, usage: Option<(u64, u64)>, ctx: &MonitorCtx) -> Div {
    let p = &ctx.p;
    let percent = usage
        .filter(|(_, total)| *total > 0)
        .map(|(used, total)| (used as f64 / total as f64 * 100.) as f32);
    let color = if ctx.stale {
        p.text_faint
    } else {
        usage_color(percent.unwrap_or(0.), p)
    };

    let percent_text = percent.map(format_percent).unwrap_or_else(|| "—".into());
    // 「已用 / 总量」只在放得下时显示：左列拖窄后，标签、用量和百分比挤在一行会被裁掉
    let usage_text = usage
        .filter(|(_, total)| *total > 0)
        .map(|(used, total)| format_usage(used, total))
        .filter(|text| {
            let needed = ctx.fit.width(label, 12., FontWeight::NORMAL)
                + ctx.fit.mono_width(text, 11.)
                + ctx.fit.mono_width(&percent_text, 12.)
                + px(8. * 2.);
            needed <= ctx.width
        });

    let value = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .children(usage_text.map(|text| mono_value(text, 11., p.text_faint, ctx)))
        .child(mono_value(percent_text, 12., ctx.value_color(), ctx));

    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(metric_row(label, value, ctx))
        .child(meter_bar(percent.unwrap_or(0.) / 100., color, 4., p))
}
