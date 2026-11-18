// 监控详情弹窗（只读）：系统信息、负载、网络的完整数据

use gpui::*;

use gpui_component::scroll::ScrollableElement;
use gpui_component::ActiveTheme;

use crate::components::common::dialog::{
    dialog_header, dialog_panel, fit_to_viewport, keep_focus_in_dialog, DialogOverlay,
};
use crate::components::common::input::settings_card;
use crate::components::common::text::TextFitter;
use crate::constants::icons;
use crate::models::monitor::MonitorState;
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

/// 监控区块标题旁的「详情」按钮
pub fn render_detail_button(
    dialog_state: Entity<DetailDialogState>,
    dialog_type: DetailDialogType,
    cx: &App,
) -> impl IntoElement {
    let muted_color = cx.theme().muted_foreground;

    div()
        .id(SharedString::from(format!("detail-btn-{:?}", dialog_type)))
        .w(px(20.))
        .h(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .cursor_pointer()
        .hover(|s| s.bg(cx.theme().secondary_hover))
        .on_click(move |_, _, cx| {
            dialog_state.update(cx, |s, cx| {
                s.open(dialog_type);
                cx.notify();
            });
        })
        .child(
            svg()
                .path(icons::EXPAND)
                .size(px(14.))
                .text_color(muted_color),
        )
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
                    format_uptime(info.host.uptime_seconds, lang),
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
    let swap_usage = if swap_total > 0 {
        format!(
            "{} / {}",
            format_bytes(load.memory.swap_used_bytes),
            format_bytes(swap_total)
        )
    } else {
        "0 B / 0 B".to_string()
    };

    div()
        .flex()
        .flex_col()
        .gap(px(16.))
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
                ("Buffers", format_bytes(load.memory.buffers_bytes)),
                ("Cached", format_bytes(load.memory.cached_bytes)),
                (t("monitor.detail.swap_usage"), swap_usage),
            ],
            p,
        ))
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

/// Format bytes to human readable
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

/// Format uptime seconds to readable string
fn format_uptime(seconds: u64, lang: &crate::models::settings::Language) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let mins = (seconds % 3600) / 60;

    let day_unit = crate::i18n::t(lang, "monitor.detail.uptime_days");
    let hour_unit = crate::i18n::t(lang, "monitor.detail.uptime_hours");
    let min_unit = crate::i18n::t(lang, "monitor.detail.uptime_minutes");

    if days > 0 {
        format!(
            "{}{} {}{} {}{}",
            days, day_unit, hours, hour_unit, mins, min_unit
        )
    } else if hours > 0 {
        format!("{}{} {}{}", hours, hour_unit, mins, min_unit)
    } else {
        format!("{}{}", mins, min_unit)
    }
}
