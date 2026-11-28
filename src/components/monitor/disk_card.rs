// 监控面板「磁盘」组：上面是各块磁盘读写速率的合计（读取、写入两行和一分钟曲线，标题右侧是最忙那块盘的繁忙度），
// 下面每个挂载点一行（挂载点 + 已用 / 总量）和一条按阈值变色的进度条。每块盘的明细在「磁盘」详情弹窗里

use gpui_kit::*;

use super::detail_dialog::DetailDialogType;
use super::view::{chart_well, empty_hint, mono_value, section, speed_row, MonitorCtx};
use crate::components::common::button::text_tooltip;
use crate::components::common::format::format_usage;
use crate::components::common::panel::{
    meter_bar, sparkline, usage_color_with_threshold, SparkSeries,
};
use crate::constants::icons;
use crate::models::monitor::{DiskDeviceInfo, MonitorState};

/// 读写曲线的高度
const IO_CHART_HEIGHT: Pixels = px(32.);
/// 曲线纵轴的最小上限（1 KB/s），避免很小的读写被放大成满格
const MIN_CHART_MAX: f32 = 1024.;

/// 渲染「磁盘」组
pub(super) fn render_disk_section(ctx: &MonitorCtx) -> impl IntoElement {
    // 标题右侧：最忙那块盘的繁忙度，悬停显示是哪块盘
    let busy = ctx
        .state
        .current_disk_io()
        .and_then(|io| io.busiest())
        .map(|device| {
            div()
                .id("monitor-disk-busy")
                .flex()
                .items_center()
                .gap(px(4.))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(ctx.p.text_faint)
                        .child(ctx.t("monitor.disk_busy")),
                )
                .child(mono_value(
                    format!("{:.0}%", device.busy_percent),
                    11.,
                    ctx.p.text_faint,
                    ctx,
                ))
                .tooltip(text_tooltip(format!(
                    "{} · {}",
                    ctx.t("monitor.disk_busy_hint"),
                    device.name
                )))
                .into_any_element()
        });

    let mounts: AnyElement = match ctx.state.disk_info.as_ref() {
        None => empty_hint("—", ctx).into_any_element(),
        Some(info) if info.disks.is_empty() => {
            empty_hint(ctx.t("monitor.no_data"), ctx).into_any_element()
        }
        Some(info) => div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .children(
                info.disks
                    .iter()
                    .enumerate()
                    .map(|(index, disk)| render_disk(index, disk, ctx)),
            )
            .into_any_element(),
    };

    section(
        "monitor-disk",
        ctx.t("monitor.section.disk"),
        busy,
        Some(DetailDialogType::DiskInfo),
        ctx,
    )
    // 远端拿不到读写数据（不是 Linux）时只显示挂载点
    .children(ctx.state.current_disk_io().map(|_| render_io(ctx)))
    .child(mounts)
}

/// 读写合计：读取、写入两行和一分钟曲线，画法与网络相同
fn render_io(ctx: &MonitorCtx) -> Div {
    let p = &ctx.p;
    let (read, write) = ctx
        .state
        .current_disk_io()
        .map(|io| io.total_rates())
        .unwrap_or_default();
    let (read_color, write_color) = if ctx.stale {
        (p.text_faint, p.text_faint)
    } else {
        (p.primary, p.success)
    };
    let (read_history, write_history) = ctx.state.disk_io_history_totals();
    let max = read_history
        .iter()
        .chain(&write_history)
        .copied()
        .fold(MIN_CHART_MAX, f32::max);

    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        // 和下面的挂载点隔开一点
        .mb(px(4.))
        .child(speed_row(
            icons::DOWNLOAD,
            read_color,
            ctx.t("monitor.label.read"),
            read,
            ctx,
        ))
        .child(speed_row(
            icons::UPLOAD,
            write_color,
            ctx.t("monitor.label.write"),
            write,
            ctx,
        ))
        .child(chart_well(IO_CHART_HEIGHT, ctx).child(sparkline(
            vec![
                SparkSeries {
                    values: read_history,
                    stroke: read_color,
                    fill: Some(read_color.opacity(0.16)),
                },
                SparkSeries {
                    values: write_history,
                    stroke: write_color,
                    fill: Some(write_color.opacity(0.10)),
                },
            ],
            max,
            MonitorState::MAX_HISTORY_SIZE,
            IO_CHART_HEIGHT - px(2.),
        )))
}

/// 一个挂载点：挂载点和「已用 / 总量」一行，下面一条进度条
fn render_disk(index: usize, disk: &DiskDeviceInfo, ctx: &MonitorCtx) -> Div {
    let p = &ctx.p;
    let usage = format_usage(disk.used_bytes, disk.total_bytes);
    let percent = if disk.usage_percent.is_finite() {
        disk.usage_percent.clamp(0., 100.)
    } else {
        0.
    };
    let color = if ctx.stale {
        p.text_faint
    } else {
        usage_color_with_threshold(percent, ctx.settings.disk_alert_threshold as f32, p)
    };
    // 挂载点可用宽度：去掉右侧「已用 / 总量」和两者之间的间距
    let mount_width = (ctx.width - ctx.fit.mono_width(&usage, 11.) - px(8.)).max(px(24.));
    // 悬停挂载点时显示设备和文件系统
    let detail = match (disk.device.is_empty(), disk.fs_type.is_empty()) {
        (false, false) => format!("{}  ({}, {})", disk.mount_point, disk.device, disk.fs_type),
        (false, true) => format!("{}  ({})", disk.mount_point, disk.device),
        _ => disk.mount_point.clone(),
    };

    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .id(SharedString::from(format!("monitor-disk-{index}")))
                .h(px(18.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .tooltip(text_tooltip(detail))
                .child(
                    ctx.fit
                        .mono_label(&disk.mount_point, mount_width, 12.)
                        .flex_none()
                        .text_color(ctx.value_color()),
                )
                .child(mono_value(usage, 11., p.text_faint, ctx)),
        )
        .child(meter_bar(percent / 100., color, 4., p))
}
