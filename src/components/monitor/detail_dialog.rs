// 监控详情弹窗（只读）：系统信息、负载、网络的完整数据

use gpui::prelude::FluentBuilder;
use gpui::*;

use gpui_component::scroll::ScrollableElement;

use crate::components::common::button::text_tooltip;
use crate::components::common::dialog::{
    dialog_header, dialog_panel, fit_to_viewport, keep_focus_in_dialog, DialogOverlay,
};
use crate::components::common::format::{
    format_bytes, format_percent, format_speed, format_uptime, format_usage,
};
use crate::components::common::input::settings_card;
use crate::components::common::panel::usage_color;
use crate::components::common::text::TextFitter;
use crate::models::monitor::{CpuBreakdown, DiskDeviceInfo, DiskIoInfo, MonitorState};
use crate::models::settings::Language;
use crate::theme::Palette;

/// 弹窗的期望尺寸（三种详情一样大），窗口较小时随之收缩
const DIALOG_SIZE: Size<Pixels> = size(px(520.), px(460.));
/// 内容区左右留白
const PADDING_X: Pixels = px(20.);
/// 详情行里标签一栏的宽度
const LABEL_WIDTH: Pixels = px(128.);

/// Detail dialog types
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DetailDialogType {
    None,
    SystemInfo,
    LoadInfo,
    NetworkInfo,
    DiskInfo,
}

/// Detail dialog state
#[derive(Clone)]
pub struct DetailDialogState {
    pub dialog_type: DetailDialogType,
    /// 界面语言，由调用方在每次渲染前传入
    pub lang: Language,
    pub focus_handle: Option<FocusHandle>,
    /// 打开后第一次渲染时聚焦弹窗（Esc 才能关闭）
    focus_pending: bool,
    scroll_handle: ScrollHandle,
}

impl Default for DetailDialogState {
    fn default() -> Self {
        Self {
            dialog_type: DetailDialogType::None,
            lang: Language::default(),
            focus_handle: None,
            focus_pending: false,
            scroll_handle: ScrollHandle::new(),
        }
    }
}

impl DetailDialogState {
    pub fn is_open(&self) -> bool {
        self.dialog_type != DetailDialogType::None
    }

    pub fn open(&mut self, dialog_type: DetailDialogType) {
        self.dialog_type = dialog_type;
        self.focus_pending = true;
        self.scroll_handle.set_offset(Point::default());
    }

    pub fn close(&mut self) {
        self.dialog_type = DetailDialogType::None;
        self.focus_pending = false;
    }

    /// 渲染前调用：记下界面语言，打开后第一次渲染时聚焦弹窗，之后保证焦点留在弹窗里
    pub fn prepare(&mut self, lang: &Language, window: &mut Window, cx: &mut Context<Self>) {
        self.lang = lang.clone();
        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        if self.focus_pending {
            self.focus_pending = false;
            focus_handle.focus(window);
        }
        keep_focus_in_dialog(cx.weak_entity(), Self::is_open, &focus_handle, window);
    }
}

/// 渲染详情弹窗覆盖层
pub fn render_detail_dialog(
    dialog_state: Entity<DetailDialogState>,
    monitor_state: &MonitorState,
    viewport: Size<Pixels>,
    fit: &TextFitter,
    cx: &App,
) -> impl IntoElement {
    let state = dialog_state.read(cx);
    if !state.is_open() {
        return div().into_any_element();
    }

    let p = crate::theme::palette(cx);
    let dialog_type = state.dialog_type;
    let lang = state.lang.clone();
    let scroll_handle = state.scroll_handle.clone();
    let dialog_size = fit_to_viewport(DIALOG_SIZE, viewport);
    // 内容可用宽度（去掉左右留白和弹窗边框）
    let content_width = (dialog_size.width - PADDING_X * 2. - px(2.)).max(px(0.));

    let title = match dialog_type {
        DetailDialogType::SystemInfo => crate::i18n::t(&lang, "monitor.system_info"),
        DetailDialogType::LoadInfo => crate::i18n::t(&lang, "monitor.load"),
        DetailDialogType::NetworkInfo => crate::i18n::t(&lang, "monitor.network"),
        DetailDialogType::DiskInfo => crate::i18n::t(&lang, "monitor.disk"),
        DetailDialogType::None => "",
    };

    let content = match dialog_type {
        DetailDialogType::SystemInfo => {
            render_system_detail(monitor_state, &lang, &p).into_any_element()
        }
        DetailDialogType::LoadInfo => {
            render_load_detail(monitor_state, &lang, content_width, fit, &p).into_any_element()
        }
        DetailDialogType::NetworkInfo => {
            render_network_detail(monitor_state, &lang, &p).into_any_element()
        }
        DetailDialogType::DiskInfo => {
            render_disk_detail(monitor_state, &lang, content_width, fit, &p).into_any_element()
        }
        DetailDialogType::None => div().into_any_element(),
    };

    let close = |state: &Entity<DetailDialogState>, cx: &mut App| {
        state.update(cx, |s, cx| {
            s.close();
            cx.notify();
        });
    };
    let state_for_close = dialog_state.clone();
    let state_for_escape = dialog_state.clone();
    let state_for_backdrop = dialog_state.clone();

    let panel = dialog_panel(&p)
        .w(dialog_size.width)
        .h(dialog_size.height)
        .child(dialog_header(
            title,
            None,
            crate::i18n::t(&lang, "common.close"),
            move |_, cx| close(&state_for_close, cx),
            &p,
        ))
        .child(
            div()
                .flex_1()
                .min_h(px(0.))
                .relative()
                .child(
                    div()
                        .id("detail-dialog-content-scroll")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&scroll_handle)
                        .child(div().px(PADDING_X).pt(px(8.)).pb(px(20.)).child(content)),
                )
                .vertical_scrollbar(&scroll_handle),
        );

    // 只读弹窗：点遮罩也可以关闭
    DialogOverlay::new()
        .focus(state.focus_handle.as_ref())
        .on_escape(move |_, cx| close(&state_for_escape, cx))
        .on_backdrop_click(move |_, cx| close(&state_for_backdrop, cx))
        .render("detail-dialog-overlay", panel, &p)
        .into_any_element()
}

/// 没有数据时的提示
fn render_no_data(lang: &Language, p: &Palette) -> Div {
    div()
        .py(px(24.))
        .text_size(px(13.))
        .text_color(p.text_muted)
        .child(crate::i18n::t(lang, "monitor.no_data"))
}

/// 系统信息：基础信息、CPU、内存
fn render_system_detail(state: &MonitorState, lang: &Language, p: &Palette) -> Div {
    let t = |key| crate::i18n::t(lang, key);
    let Some(info) = &state.system_info else {
        return render_no_data(lang, p);
    };
    div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(render_detail_section(
            t("monitor.detail.basic_info"),
            vec![
                (t("monitor.detail.host_address"), info.host.address.clone()),
                (t("monitor.detail.hostname"), info.host.hostname.clone()),
                (t("monitor.detail.os"), info.host.os.clone()),
                (t("monitor.detail.kernel"), info.host.kernel.clone()),
                (
                    t("monitor.detail.uptime"),
                    state
                        .current_uptime()
                        .map(|seconds| format_uptime(seconds, lang))
                        .unwrap_or_default(),
                ),
            ],
            p,
        ))
        .child(render_detail_section(
            t("monitor.detail.cpu"),
            vec![
                (t("monitor.detail.cpu_model"), info.cpu.model.clone()),
                (
                    t("monitor.detail.cores_logical"),
                    info.cpu.cores_logical.to_string(),
                ),
                (
                    t("monitor.detail.cores_physical"),
                    info.cpu.cores_physical.to_string(),
                ),
                (
                    t("monitor.detail.architecture"),
                    info.cpu.architecture.clone(),
                ),
            ],
            p,
        ))
        .child(render_detail_section(
            t("monitor.detail.memory"),
            vec![
                (
                    t("monitor.detail.memory_total"),
                    format_bytes(info.memory.total_bytes),
                ),
                (
                    t("monitor.detail.swap_total"),
                    format_bytes(info.memory.swap_total_bytes),
                ),
            ],
            p,
        ))
}

/// 负载：CPU、内存占用最高的进程，内存详情
fn render_load_detail(
    state: &MonitorState,
    lang: &Language,
    content_width: Pixels,
    fit: &TextFitter,
    p: &Palette,
) -> Div {
    let t = |key| crate::i18n::t(lang, key);
    let Some(load) = state.current_load() else {
        return render_no_data(lang, p);
    };
    let swap_total = state
        .system_info
        .as_ref()
        .map(|s| s.memory.swap_total_bytes)
        .unwrap_or(0);
    // 远端不是 Linux 时拿不到内存用量，各项显示「—」
    let memory = load.memory.as_ref();
    let bytes = |value: Option<u64>| value.map(format_bytes).unwrap_or_else(|| "—".to_string());
    let swap_usage = match memory {
        Some(memory) if swap_total > 0 => format!(
            "{} / {}",
            format_bytes(memory.swap_used_bytes),
            format_bytes(swap_total)
        ),
        Some(_) => "0 B / 0 B".to_string(),
        None => "—".to_string(),
    };

    div()
        .flex()
        .flex_col()
        .gap(px(16.))
        // 远端不是 Linux、或刚连上还没有第二次采样时没有这两块
        .children(
            load.cpu
                .usage_percent
                .zip(load.cpu.breakdown)
                .map(|(usage, breakdown)| {
                    render_cpu_time(usage, &breakdown, lang, content_width, fit, p)
                }),
        )
        .children(
            (!load.cpu.per_core.is_empty())
                .then(|| render_per_core(&load.cpu.per_core, lang, content_width, fit, p)),
        )
        .child(render_process_table(
            t("monitor.detail.top_cpu_processes"),
            &load.top_cpu_processes,
            true,
            lang,
            content_width,
            fit,
            p,
        ))
        .child(render_process_table(
            t("monitor.detail.top_mem_processes"),
            &load.top_memory_processes,
            false,
            lang,
            content_width,
            fit,
            p,
        ))
        .child(render_detail_section(
            t("monitor.detail.memory_detail"),
            vec![
                ("Buffers", bytes(memory.map(|m| m.buffers_bytes))),
                ("Cached", bytes(memory.map(|m| m.cached_bytes))),
                (t("monitor.detail.swap_usage"), swap_usage),
            ],
            p,
        ))
}

/// 卡片的左右留白（CPU 时间分布、每核使用率两张卡片）
const CARD_PADDING: Pixels = px(14.);

/// 卡片里可用的宽度：去掉卡片边框和左右留白
fn card_inner_width(content_width: Pixels) -> Pixels {
    (content_width - px(2.) - CARD_PADDING * 2.).max(px(0.))
}

/// 分组标题行：左边标题，右边一段摘要（可选）
fn section_header(title: &str, summary: Option<Div>, p: &Palette) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(render_section_title(title, p))
        .child(div().flex_1())
        .children(summary)
}

/// 摘要：弱色的说明文字 + 等宽的数值，例如「使用率 51.5%」
fn summary(label: String, value: String, fit: &TextFitter, p: &Palette) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .child(
            div()
                .text_size(px(12.))
                .text_color(p.text_faint)
                .child(label),
        )
        .child(fit.mono_container(12.).text_color(p.text).child(value))
}

/// CPU 时间各项的颜色，按显示顺序固定分配：用户态、内核态、中断、宿主机占用、IO 等待。
/// 两种主题下相邻两色都用 dataviz 的校验脚本查过（含色盲模拟），第一色就是主色
fn cpu_time_colors(p: &Palette) -> [Hsla; 5] {
    let pick = |dark: u32, light: u32| -> Hsla { rgb(if p.is_dark { dark } else { light }).into() };
    [
        p.primary,
        pick(0xD95926, 0xEB6834),
        pick(0x199E70, 0x1BAF7A),
        pick(0xC98500, 0xEDA100),
        pick(0xD55181, 0xE87BA4),
    ]
}

/// CPU 时间分布：一条细分段条（各段按占比，段间留 2px 缝），下面是各项名称和占比。
/// 用户态、内核态、中断、宿主机占用合起来就是标题右侧的使用率；IO 等待算空闲
fn render_cpu_time(
    usage: f32,
    breakdown: &CpuBreakdown,
    lang: &Language,
    content_width: Pixels,
    fit: &TextFitter,
    p: &Palette,
) -> Div {
    const SEGMENT_GAP: Pixels = px(2.);
    const LEGEND_GAP: Pixels = px(22.);
    let t = |key| crate::i18n::t(lang, key);
    let colors = cpu_time_colors(p);
    let parts = [
        (t("monitor.detail.cpu_user"), breakdown.user, None),
        (t("monitor.detail.cpu_system"), breakdown.system, None),
        (t("monitor.detail.cpu_irq"), breakdown.irq, None),
        (
            t("monitor.detail.cpu_steal"),
            breakdown.steal,
            Some(t("monitor.detail.cpu_steal_hint")),
        ),
        (
            t("monitor.detail.cpu_iowait"),
            breakdown.iowait,
            Some(t("monitor.detail.cpu_iowait_hint")),
        ),
    ];
    let inner = card_inner_width(content_width);

    // 分段条：占比为 0 的项不画；最后一段是空闲，占满剩下的宽度
    let visible: Vec<(Hsla, f32)> = parts
        .iter()
        .zip(colors)
        .filter(|((_, value, _), _)| *value > 0.)
        .map(|((_, value, _), color)| (color, *value))
        .collect();
    let available = (inner - SEGMENT_GAP * visible.len() as f32).max(px(0.));
    let bar = div()
        .h(px(8.))
        .flex()
        .gap(SEGMENT_GAP)
        .rounded(px(4.))
        .overflow_hidden()
        .children(visible.iter().map(|(color, value)| {
            div()
                .flex_none()
                .h_full()
                .w((available * (value / 100.)).max(px(1.)))
                .bg(*color)
        }))
        .child(div().flex_1().h_full().bg(p.hover_strong));

    // 图例：三列，每项「色块 名称 …… 占比」，宿主机占用和 IO 等待悬停有说明
    let item_width = ((inner - LEGEND_GAP * 2.) / 3.).floor();
    let legend_item =
        |index: usize, color: Hsla, label: &str, value: f32, hint: Option<&'static str>| {
            div()
                .id(("cpu-time-legend", index))
                .w(item_width)
                .h(px(20.))
                .flex()
                .items_center()
                .gap(px(6.))
                .child(div().flex_none().size(px(8.)).rounded(px(2.)).bg(color))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(12.))
                        .text_color(p.text_muted)
                        .child(label.to_string()),
                )
                .child(div().flex_1())
                .child(
                    fit.mono_container(12.)
                        .flex_none()
                        .text_color(p.text)
                        .child(format_percent(value)),
                )
                .when_some(hint, |item, hint| item.tooltip(text_tooltip(hint)))
        };
    let legend = div()
        .flex()
        .flex_wrap()
        .gap_x(LEGEND_GAP)
        .gap_y(px(2.))
        .children(parts.iter().zip(colors).enumerate().map(
            |(index, ((label, value, hint), color))| {
                legend_item(index, color, label, *value, *hint)
            },
        ))
        .child(legend_item(
            parts.len(),
            p.hover_strong,
            t("monitor.detail.cpu_idle"),
            breakdown.idle,
            None,
        ));

    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(section_header(
            t("monitor.detail.cpu_time"),
            Some(summary(
                t("monitor.detail.usage").to_string(),
                format_percent(usage),
                fit,
                p,
            )),
            p,
        ))
        .child(
            settings_card(p)
                .p(CARD_PADDING)
                .gap(px(12.))
                .child(bar)
                .child(legend),
        )
}

/// 每核使用率：每个核一根柱（到 75% / 90% 变色），只标出最高那个核的数值，悬停柱子显示该核的数值；
/// 右侧是 100% / 50% / 0 的刻度。核多时柱子变细，编号每 8 个标一个
fn render_per_core(
    values: &[f32],
    lang: &Language,
    content_width: Pixels,
    fit: &TextFitter,
    p: &Palette,
) -> Div {
    /// 柱子上方给最高值标签留的高度
    const HEAD: Pixels = px(16.);
    const PLOT: Pixels = px(64.);
    /// 柱子下方编号的高度
    const INDEX: Pixels = px(16.);
    /// 右侧刻度的宽度
    const AXIS: Pixels = px(30.);
    let t = |key| crate::i18n::t(lang, key);
    let count = values.len();
    let few = count <= 16;
    let inner = card_inner_width(content_width);
    let plot_width = (inner - AXIS).max(px(0.));
    let slot = plot_width / count.max(1) as f32;
    let bar_width = if few {
        (slot * 0.5).min(px(24.))
    } else {
        (slot - px(2.5)).max(px(2.))
    };
    let radius = if few { px(4.) } else { px(1.5) };
    let highest = values
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(index, value)| (index, *value));

    // 横线和刻度：100% 和 50% 用细线，0 用分隔线的颜色
    let rule = |fraction: f32, color: Hsla| {
        let top = HEAD + PLOT * (1. - fraction) - if fraction == 0. { px(1.) } else { px(0.) };
        div()
            .absolute()
            .left_0()
            .top(top)
            .w(plot_width)
            .h(px(1.))
            .bg(color)
    };
    let tick = |fraction: f32, label: &'static str| {
        fit.mono_container(10.)
            .absolute()
            .right_0()
            .top(HEAD + PLOT * (1. - fraction) - px(6.))
            .h(px(12.))
            .line_height(px(12.))
            .text_color(p.text_faint)
            .child(label)
    };

    let bars = div()
        .absolute()
        .top_0()
        .left_0()
        .w(plot_width)
        .h(HEAD + PLOT + INDEX)
        .flex()
        .children(values.iter().enumerate().map(|(index, value)| {
            let value = if value.is_finite() {
                value.clamp(0., 100.)
            } else {
                0.
            };
            let label = (highest.map(|(i, _)| i) == Some(index)).then(|| {
                fit.mono_container(10.)
                    .mb(px(3.))
                    .text_color(p.text)
                    .child(format!("{value:.0}%"))
            });
            let number = (few || index % 8 == 0).then(|| {
                fit.mono_container(10.)
                    .text_color(p.text_faint)
                    .child(index.to_string())
            });
            div()
                .id(("core", index))
                .flex_none()
                .w(slot)
                .h_full()
                .flex()
                .flex_col()
                .items_center()
                .tooltip(text_tooltip(format!("CPU {index}: {value:.0}%")))
                .child(
                    div()
                        .h(HEAD + PLOT)
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_end()
                        .children(label)
                        .child(
                            div()
                                .w(bar_width)
                                .h((PLOT * (value / 100.)).max(px(1.)))
                                .rounded_t(radius)
                                .bg(usage_color(value, p)),
                        ),
                )
                .child(div().h(INDEX).pt(px(2.)).children(number))
        }));

    let chart = div()
        .relative()
        .w(inner)
        .h(HEAD + PLOT + INDEX)
        .child(rule(1., p.line))
        .child(rule(0.5, p.line))
        .child(rule(0., p.border))
        .child(tick(1., "100%"))
        .child(tick(0.5, "50%"))
        .child(tick(0., "0"))
        .child(bars);

    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(section_header(
            t("monitor.detail.per_core"),
            highest.map(|(_, value)| {
                summary(
                    format!(
                        "{count} {} · {}",
                        t("monitor.detail.cores_unit"),
                        t("monitor.detail.highest")
                    ),
                    format!("{value:.0}%"),
                    fit,
                    p,
                )
            }),
            p,
        ))
        .child(
            settings_card(p)
                .px(CARD_PADDING)
                .pt(px(10.))
                .pb(px(6.))
                .child(chart),
        )
}

/// 磁盘：每块磁盘的读写，每个挂载点的空间
fn render_disk_detail(
    state: &MonitorState,
    lang: &Language,
    content_width: Pixels,
    fit: &TextFitter,
    p: &Palette,
) -> Div {
    let io = state.current_disk_io().filter(|io| !io.devices.is_empty());
    let disks = state
        .disk_info
        .as_ref()
        .filter(|info| !info.disks.is_empty());
    if io.is_none() && disks.is_none() {
        return render_no_data(lang, p);
    }
    div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .children(io.map(|io| render_disk_io_table(io, lang, content_width, fit, p)))
        .children(
            disks.map(|info| render_disk_space_table(&info.disks, lang, content_width, fit, p)),
        )
}

/// 每块磁盘一行：读取、写入速率，每秒读 / 写请求数，繁忙度，平均每个请求的耗时
fn render_disk_io_table(
    io: &DiskIoInfo,
    lang: &Language,
    content_width: Pixels,
    fit: &TextFitter,
    p: &Palette,
) -> Div {
    const RATE: Pixels = px(76.);
    const IOPS: Pixels = px(64.);
    const BUSY: Pixels = px(36.);
    const AWAIT: Pixels = px(52.);
    let t = |key| crate::i18n::t(lang, key);
    // 设备名一栏：去掉卡片边框、行内边距、其余各栏和五处间距
    let name_width =
        (content_width - px(2.) - px(28.) - RATE * 2. - IOPS - BUSY - AWAIT - px(60.)).max(px(24.));
    let cell = |width: Pixels| div().flex_none().w(width);

    let header = detail_row(p)
        .min_h(px(32.))
        .text_size(px(12.))
        .text_color(p.text_muted)
        .child(div().flex_1().min_w_0().child(t("monitor.detail.device")))
        .child(cell(RATE).child(t("monitor.label.read")))
        .child(cell(RATE).child(t("monitor.label.write")))
        .child(cell(IOPS).child(t("monitor.detail.iops")))
        .child(cell(BUSY).child(t("monitor.disk_busy")))
        .child(cell(AWAIT).child(t("monitor.detail.await")));

    let rows = io.devices.iter().enumerate().map(|(index, device)| {
        let requests = device.reads_per_sec + device.writes_per_sec;
        detail_row(p)
            .text_color(p.text)
            .child(
                div().flex_1().min_w_0().child(
                    fit.mono_label_with_tooltip(
                        ("disk-io-device", index),
                        &device.name,
                        name_width,
                        12.,
                    )
                    .text_color(p.text),
                ),
            )
            .child(cell(RATE).child(format_speed(device.read_bytes_per_sec)))
            .child(cell(RATE).child(format_speed(device.write_bytes_per_sec)))
            .child(cell(IOPS).child(format!(
                "{:.0} / {:.0}",
                device.reads_per_sec, device.writes_per_sec
            )))
            .child(cell(BUSY).child(format!("{:.0}%", device.busy_percent)))
            .child(
                cell(AWAIT)
                    .text_color(p.text_muted)
                    .child(if requests > 0. {
                        format!("{:.1} ms", device.await_ms)
                    } else {
                        "—".to_string()
                    }),
            )
    });

    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(render_section_title(t("monitor.detail.disk_io"), p))
        .child(settings_card(p).child(header).children(rows))
}

/// 每个挂载点一行：挂载点和设备、文件系统类型、已用 / 总量、使用率、inode 使用率
fn render_disk_space_table(
    disks: &[DiskDeviceInfo],
    lang: &Language,
    content_width: Pixels,
    fit: &TextFitter,
    p: &Palette,
) -> Div {
    const FS: Pixels = px(40.);
    const USED: Pixels = px(112.);
    const PERCENT: Pixels = px(44.);
    let t = |key| crate::i18n::t(lang, key);
    // 挂载点一栏：去掉卡片边框、行内边距、其余各栏和四处间距
    let mount_column =
        (content_width - px(2.) - px(28.) - FS - USED - PERCENT * 2. - px(48.)).max(px(48.));
    let cell = |width: Pixels| div().flex_none().w(width);

    let header = detail_row(p)
        .min_h(px(32.))
        .text_size(px(12.))
        .text_color(p.text_muted)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(t("monitor.detail.mount_point")),
        )
        .child(cell(FS).child(t("monitor.detail.fs_type")))
        .child(cell(USED).child(t("monitor.detail.used_total")))
        .child(cell(PERCENT).child(t("monitor.detail.usage")))
        .child(cell(PERCENT).child(t("monitor.detail.inodes")));

    let rows = disks.iter().enumerate().map(|(index, disk)| {
        // 设备名去掉 /dev/ 前缀，跟在挂载点后面用弱色显示
        let device = disk.device.strip_prefix("/dev/").unwrap_or(&disk.device);
        let device_width = fit.mono_width(device, 11.);
        let mount_width = (mount_column - device_width - px(6.)).max(px(24.));
        let inodes = if disk.inodes_total > 0 {
            format!(
                "{:.0}%",
                disk.inodes_used as f64 / disk.inodes_total as f64 * 100.
            )
        } else {
            "—".to_string()
        };
        detail_row(p)
            .text_color(p.text)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        fit.mono_label_with_tooltip(
                            ("disk-space-mount", index),
                            &disk.mount_point,
                            mount_width,
                            12.,
                        )
                        .flex_none()
                        .text_color(p.text),
                    )
                    .child(
                        fit.mono_container(11.)
                            .flex_none()
                            .text_color(p.text_faint)
                            .child(device.to_string()),
                    ),
            )
            .child(
                cell(FS)
                    .text_color(p.text_muted)
                    .child(disk.fs_type.clone()),
            )
            .child(cell(USED).child(format_usage(disk.used_bytes, disk.total_bytes)))
            .child(cell(PERCENT).child(format!("{:.0}%", disk.usage_percent)))
            .child(cell(PERCENT).text_color(p.text_muted).child(inodes))
    });

    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(render_section_title(t("monitor.detail.disk_space"), p))
        .child(settings_card(p).child(header).children(rows))
}

/// 网络：全局连接数，各网卡的地址和流量
fn render_network_detail(state: &MonitorState, lang: &Language, p: &Palette) -> Div {
    let t = |key| crate::i18n::t(lang, key);
    let Some(network) = state.current_network() else {
        return render_no_data(lang, p);
    };
    div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(render_detail_section(
            t("monitor.detail.global_network"),
            vec![(
                t("monitor.detail.tcp_connections"),
                network.global.tcp_established.to_string(),
            )],
            p,
        ))
        .children(network.interfaces.iter().map(|iface| {
            render_detail_section(
                &iface.name,
                vec![
                    (
                        t("monitor.detail.ip_address"),
                        iface.ip_addresses.join(", "),
                    ),
                    (t("monitor.detail.mac_address"), iface.mac_address.clone()),
                    (t("monitor.detail.rx_bytes"), format_bytes(iface.rx_bytes)),
                    (t("monitor.detail.tx_bytes"), format_bytes(iface.tx_bytes)),
                ],
                p,
            )
        }))
}

/// 分组标题（与设置页一致）
fn render_section_title(title: &str, p: &Palette) -> Div {
    div()
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.text_muted)
        .child(title.to_string())
}

/// 一组键值：标题 + 卡片，卡片里每行左边标签、右边值（值过长时换行）
fn render_detail_section(title: &str, items: Vec<(&str, String)>, p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(render_section_title(title, p))
        .child(
            settings_card(p).children(items.into_iter().map(|(label, value)| {
                detail_row(p)
                    .child(
                        div()
                            .flex_none()
                            .w(LABEL_WIDTH)
                            .text_color(p.text_muted)
                            .child(label.to_string()),
                    )
                    .child(div().flex_1().min_w_0().text_color(p.text).child(value))
            })),
        )
}

/// 卡片里的一行：上边一条分隔线（第一行的被卡片边框裁掉）
fn detail_row(p: &Palette) -> Div {
    div()
        .mt(px(-1.))
        .border_t_1()
        .border_color(p.line)
        .min_h(px(36.))
        .px(px(14.))
        .py(px(8.))
        .flex()
        .items_center()
        .gap(px(12.))
        .text_size(px(13.))
}

/// 进程表：进程名、CPU / 内存占用、PID，最多 5 行
fn render_process_table(
    title: &str,
    processes: &[crate::models::monitor::ProcessInfo],
    is_cpu: bool,
    lang: &Language,
    content_width: Pixels,
    fit: &TextFitter,
    p: &Palette,
) -> Div {
    const USAGE_WIDTH: Pixels = px(64.);
    const PID_WIDTH: Pixels = px(64.);
    // 进程名可用宽度：去掉卡片边框、行内边距、另外两栏和间距
    let name_width =
        (content_width - px(2.) - px(28.) - USAGE_WIDTH - PID_WIDTH - px(24.)).max(px(0.));

    let header = detail_row(p)
        .min_h(px(32.))
        .text_size(px(12.))
        .text_color(p.text_muted)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(crate::i18n::t(lang, "monitor.detail.process_name")),
        )
        .child(
            div()
                .flex_none()
                .w(USAGE_WIDTH)
                .child(if is_cpu { "CPU%" } else { "MEM%" }),
        )
        .child(div().flex_none().w(PID_WIDTH).child("PID"));

    let rows = processes
        .iter()
        .take(5)
        .enumerate()
        .map(|(index, process)| {
            let usage = if is_cpu {
                process.cpu_percent
            } else {
                process.memory_percent
            };
            detail_row(p)
                .text_color(p.text)
                .child(div().flex_1().min_w_0().child(fit.label_with_tooltip(
                    SharedString::from(format!("process-{}-{index}", is_cpu)),
                    &process.name,
                    name_width,
                    13.,
                    FontWeight::NORMAL,
                )))
                .child(
                    div()
                        .flex_none()
                        .w(USAGE_WIDTH)
                        .child(format!("{:.1}%", usage)),
                )
                .child(
                    div()
                        .flex_none()
                        .w(PID_WIDTH)
                        .text_color(p.text_muted)
                        .child(process.pid.to_string()),
                )
        });

    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(render_section_title(title, p))
        .child(settings_card(p).child(header).children(rows))
}
