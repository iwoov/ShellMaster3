// 系统监控面板（会话视图左上）：头部（服务器名 + 详情）、断开提示，以及系统 / 负载 / 网络 / 磁盘四组数据

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use gpui_kit::component::scroll::ScrollableElement;

use super::detail_dialog::{DetailDialogState, DetailDialogType};
use super::{disk_card, load_card, network_card, system_card};
use crate::components::common::button::icon_button_sized;
use crate::components::common::format::format_speed;
use crate::components::common::icon::{distribution_icon, icon_sized, server_icon};
use crate::components::common::panel::{
    panel_header, panel_state, section_title, StateIcon, PANEL_PADDING,
};
use crate::components::common::text::TextFitter;
use crate::constants::icons;
use crate::i18n;
use crate::models::monitor::MonitorState;
use crate::models::settings::Language;
use crate::state::{SessionState, SessionStatus, SessionTab};
use crate::theme::Palette;

/// 各组数据共用的渲染上下文
pub(super) struct MonitorCtx<'a> {
    pub state: &'a MonitorState,
    pub tab: &'a SessionTab,
    pub p: Palette,
    pub lang: Language,
    pub fit: TextFitter,
    /// 内容区宽度（去掉面板右边线和左右留白）
    pub width: Pixels,
    /// 会话已断开：数据停止更新，数值改用弱色
    pub stale: bool,
    /// 用户监控偏好（显示项与告警阈值）
    pub settings: crate::models::settings::MonitorSettings,
    pub dialog_state: Entity<DetailDialogState>,
    pub session_state: Entity<SessionState>,
}

impl MonitorCtx<'_> {
    pub fn t(&self, key: &'static str) -> &'static str {
        i18n::t(&self.lang, key)
    }

    /// 数值的颜色：断开后变弱
    pub fn value_color(&self) -> Hsla {
        if self.stale {
            self.p.text_faint
        } else {
            self.p.text
        }
    }
}

/// 渲染监控面板。`width` 是左列宽度（含右边线）
pub fn render_monitor_view(
    tab: &SessionTab,
    width: Pixels,
    dialog_state: Entity<DetailDialogState>,
    session_state: Entity<SessionState>,
    lang: &Language,
    fit: &TextFitter,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let state = &tab.monitor_state;
    let stale = matches!(
        tab.status,
        SessionStatus::Disconnected | SessionStatus::Reconnecting { .. }
    );
    let monitor_settings = crate::services::storage::cached_settings().monitor.clone();
    let ctx = MonitorCtx {
        state,
        tab,
        p,
        lang: lang.clone(),
        fit: fit.clone(),
        width: (width - px(1.) - PANEL_PADDING * 2.).max(px(0.)),
        stale,
        settings: monitor_settings,
        dialog_state,
        session_state,
    };

    let show_load = ctx.settings.show_cpu || ctx.settings.show_memory;
    let body = if state.has_any_data() {
        div()
            .id("monitor-scroll")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scrollbar()
            .child(
                div()
                    .px(PANEL_PADDING)
                    .pt(px(12.))
                    .pb(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .child(system_card::render_system_section(&ctx, cx))
                    .when(show_load, |this| {
                        this.child(load_card::render_load_section(&ctx))
                    })
                    .when(ctx.settings.show_network, |this| {
                        this.child(network_card::render_network_section(&ctx, cx))
                    })
                    .when(ctx.settings.show_disk, |this| {
                        this.child(disk_card::render_disk_section(&ctx))
                    }),
            )
            .into_any_element()
    } else if stale {
        panel_state(
            "monitor-empty",
            StateIcon::Icon(icons::UNPLUG),
            p.text_faint,
            ctx.t("monitor.offline"),
            None,
            &p,
        )
        .into_any_element()
    } else {
        panel_state(
            "monitor-empty",
            StateIcon::Spinner,
            p.text_faint,
            ctx.t("monitor.collecting"),
            None,
            &p,
        )
        .into_any_element()
    };

    div()
        .size_full()
        .min_h(px(0.))
        .bg(p.sidebar)
        .flex()
        .flex_col()
        .child(render_header(&ctx))
        .when(stale && state.has_any_data(), |panel| {
            panel.child(render_paused_banner(&ctx))
        })
        .child(body)
}

/// 头部：发行版图标 + 服务器名 + 系统信息详情按钮
fn render_header(ctx: &MonitorCtx) -> Div {
    let p = &ctx.p;
    let server = ctx.tab.server_data.as_ref();
    let os = ctx
        .state
        .system_info
        .as_ref()
        .map(|info| info.host.os.as_str())
        .or_else(|| server.and_then(|server| server.os_name.as_deref()));
    let description = server
        .and_then(|server| server.description.as_deref())
        .unwrap_or_default();
    let (icon, icon_color) = server_icon(
        distribution_icon(os, &ctx.tab.server_label, description),
        p.is_dark,
        p.text_muted,
    );
    // 名称可用宽度：去掉图标 16、两处间距 8 和详情按钮 24
    let name_width = (ctx.width - px(16. + 8. + 8. + 24.)).max(px(0.));

    panel_header(p)
        .px(PANEL_PADDING)
        .gap(px(8.))
        .child(icon_sized(icon, 16., icon_color))
        .child(
            ctx.fit
                .label_with_tooltip(
                    "monitor-server-name",
                    &ctx.tab.server_label,
                    name_width,
                    13.,
                    FontWeight::SEMIBOLD,
                )
                .flex_none()
                .text_color(p.text),
        )
        .child(div().flex_1())
        .child(detail_button(
            ctx,
            DetailDialogType::SystemInfo,
            "monitor-header-detail",
            24.,
            13.,
        ))
}

/// 会话断开后，头部下方的提示条
fn render_paused_banner(ctx: &MonitorCtx) -> Div {
    let p = &ctx.p;
    div()
        .flex_none()
        .px(PANEL_PADDING)
        .py(px(7.))
        .flex()
        .items_center()
        .gap(px(6.))
        .bg(p.warning.opacity(0.1))
        .border_b_1()
        .border_color(p.border)
        .child(icon_sized(icons::UNPLUG, 12., p.warning))
        .child(
            div()
                .text_size(px(12.))
                .text_color(p.warning)
                .child(ctx.t("monitor.paused")),
        )
}

/// 打开详情弹窗的图标按钮
pub(super) fn detail_button(
    ctx: &MonitorCtx,
    kind: DetailDialogType,
    id: impl Into<ElementId>,
    size: f32,
    icon_size: f32,
) -> Stateful<Div> {
    let dialog_state = ctx.dialog_state.clone();
    icon_button_sized(
        id,
        icons::INFO,
        px(size),
        icon_size,
        ctx.t("monitor.details"),
        &ctx.p,
    )
    .on_click(move |_, _, cx| {
        dialog_state.update(cx, |state, cx| {
            state.open(kind);
            cx.notify();
        });
    })
}

/// 一组数据：小标题行（右侧可放附加内容，悬停时出现详情按钮）+ 内容
pub(super) fn section(
    id: &'static str,
    title: &'static str,
    trailing: Option<AnyElement>,
    detail: Option<DetailDialogType>,
    ctx: &MonitorCtx,
) -> Stateful<Div> {
    let group = SharedString::from(format!("{id}-group"));
    div()
        .id(id)
        .group(group.clone())
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .h(px(20.))
                .flex()
                .items_center()
                .gap(px(6.))
                .child(section_title(title, &ctx.p))
                .child(div().flex_1())
                .children(trailing)
                .children(detail.map(|kind| {
                    detail_button(
                        ctx,
                        kind,
                        SharedString::from(format!("{id}-detail")),
                        20.,
                        12.,
                    )
                    .invisible()
                    .group_hover(group, |s| s.visible())
                })),
        )
}

/// 一行指标：左边标签，右边数值
pub(super) fn metric_row(label: &'static str, value: impl IntoElement, ctx: &MonitorCtx) -> Div {
    div()
        .h(px(18.))
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .flex_none()
                .whitespace_nowrap()
                .text_size(px(12.))
                .text_color(ctx.p.text_muted)
                .child(label),
        )
        .child(div().flex_1())
        .child(value)
}

/// 等宽数字（数值跳动时宽度不变）
pub(super) fn mono_value(
    text: impl Into<SharedString>,
    size: f32,
    color: Hsla,
    ctx: &MonitorCtx,
) -> Div {
    ctx.fit
        .mono_container(size)
        .flex_none()
        .text_color(color)
        .child(text.into())
}

/// 一行速率：彩色图标（兼作曲线的图例）+ 标签，速率靠右。网络的下载 / 上传、磁盘的读取 / 写入共用
pub(super) fn speed_row(
    icon: &'static str,
    color: Hsla,
    label: &'static str,
    bytes_per_sec: f64,
    ctx: &MonitorCtx,
) -> Div {
    div()
        .h(px(18.))
        .flex()
        .items_center()
        .gap(px(6.))
        .child(icon_sized(icon, 12., color))
        .child(
            div()
                .flex_none()
                .text_size(px(12.))
                .text_color(ctx.p.text_muted)
                .child(label),
        )
        .child(div().flex_1())
        .child(mono_value(
            format_speed(bytes_per_sec),
            12.,
            ctx.value_color(),
            ctx,
        ))
}

/// 曲线外面的浅底框
pub(super) fn chart_well(height: Pixels, ctx: &MonitorCtx) -> Div {
    div()
        .w_full()
        .h(height)
        .rounded(px(6.))
        .bg(ctx.p.inset)
        .border_1()
        .border_color(ctx.p.line)
        .overflow_hidden()
        .flex()
        .items_end()
}

/// 没有数据时的占位文字
pub(super) fn empty_hint(text: &'static str, ctx: &MonitorCtx) -> Div {
    div()
        .text_size(px(12.))
        .text_color(ctx.p.text_faint)
        .child(text)
}
