// 监控面板「网络」组：网卡选择、下载 / 上传速度、半分钟网速曲线

use gpui::*;
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::Sizable;

use super::detail_dialog::DetailDialogType;
use super::view::{chart_well, empty_hint, section, speed_row, MonitorCtx};
use crate::components::common::icon::icon_sized;
use crate::components::common::panel::{sparkline, SparkSeries};
use crate::constants::icons;
use crate::models::monitor::MonitorState;

/// 网速曲线的高度
const NETWORK_CHART_HEIGHT: Pixels = px(40.);
/// 网卡下拉按钮里名称的最大宽度
const INTERFACE_NAME_MAX_WIDTH: Pixels = px(96.);
/// 曲线纵轴的最小上限（1 KB/s），避免很小的流量被放大成满格
const MIN_CHART_MAX: f64 = 1024.;

/// 渲染「网络」组
pub(super) fn render_network_section(ctx: &MonitorCtx, cx: &App) -> impl IntoElement {
    let network = ctx.state.current_network();
    let interfaces: Vec<String> = network
        .map(|network| network.interfaces.iter().map(|i| i.name.clone()).collect())
        .unwrap_or_default();

    let picker = (!interfaces.is_empty())
        .then(|| render_interface_picker(interfaces.clone(), ctx, cx).into_any_element());

    let body: AnyElement = if network.is_none() {
        empty_hint("—", ctx).into_any_element()
    } else if interfaces.is_empty() {
        empty_hint(ctx.t("monitor.no_interfaces"), ctx).into_any_element()
    } else {
        render_speeds(ctx).into_any_element()
    };

    section(
        "monitor-network",
        ctx.t("monitor.section.network"),
        picker,
        Some(DetailDialogType::NetworkInfo),
        ctx,
    )
    .child(body)
}

/// 下载 / 上传速度和曲线
fn render_speeds(ctx: &MonitorCtx) -> Div {
    let p = &ctx.p;
    let (rx, tx) = ctx.state.current_speed();
    let (rx_color, tx_color) = if ctx.stale {
        (p.text_faint, p.text_faint)
    } else {
        (p.primary, p.success)
    };
    let max = ctx
        .state
        .speed_history
        .iter()
        .flat_map(|s| [s.rx_speed, s.tx_speed])
        .fold(MIN_CHART_MAX, f64::max);
    let rx_values = ctx
        .state
        .speed_history
        .iter()
        .map(|s| s.rx_speed as f32)
        .collect();
    let tx_values = ctx
        .state
        .speed_history
        .iter()
        .map(|s| s.tx_speed as f32)
        .collect();

    // 一行一个方向（左列很窄，两个方向放一行会挤在一起）
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(speed_row(
            icons::DOWNLOAD,
            rx_color,
            ctx.t("monitor.label.download"),
            rx,
            ctx,
        ))
        .child(speed_row(
            icons::UPLOAD,
            tx_color,
            ctx.t("monitor.label.upload"),
            tx,
            ctx,
        ))
        .child(chart_well(NETWORK_CHART_HEIGHT, ctx).child(sparkline(
            vec![
                SparkSeries {
                    values: rx_values,
                    stroke: rx_color,
                    fill: Some(rx_color.opacity(0.16)),
                },
                SparkSeries {
                    values: tx_values,
                    stroke: tx_color,
                    fill: Some(tx_color.opacity(0.10)),
                },
            ],
            max as f32,
            MonitorState::MAX_HISTORY_SIZE,
            NETWORK_CHART_HEIGHT - px(2.),
        )))
}

/// 小标题右侧的网卡下拉：当前网卡名 + 箭头，菜单里勾选当前网卡
fn render_interface_picker(
    interfaces: Vec<String>,
    ctx: &MonitorCtx,
    cx: &App,
) -> impl IntoElement {
    let p = &ctx.p;
    let selected = ctx.state.selected_interface_index;
    let name = interfaces.get(selected).cloned().unwrap_or_default();
    let session_state = ctx.session_state.clone();
    let tab_id = ctx.tab.id.clone();
    // 透明底、悬停时显示浅底色
    let style = ButtonCustomVariant::new(cx)
        .color(gpui::transparent_black())
        .foreground(p.text_muted)
        .border(gpui::transparent_black())
        .hover(p.hover_strong)
        .active(p.hover_strong);

    Button::new("monitor-interface-picker")
        .custom(style)
        .xsmall()
        .h(px(20.))
        .px(px(6.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(
                    ctx.fit
                        .mono_label(&name, INTERFACE_NAME_MAX_WIDTH, 11.)
                        .text_color(p.text_muted),
                )
                .child(icon_sized(icons::CHEVRON_DOWN, 10., p.text_faint)),
        )
        .dropdown_menu_with_anchor(Corner::TopRight, move |menu, _, _| {
            let mut menu = menu.min_w(px(140.));
            for (index, name) in interfaces.iter().enumerate() {
                let session_state = session_state.clone();
                let tab_id = tab_id.clone();
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(name.clone()))
                        .checked(index == selected)
                        .on_click(move |_, _, cx| {
                            session_state.update(cx, |state, cx| {
                                if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                    tab.monitor_state.select_interface(index);
                                }
                                cx.notify();
                            });
                        }),
                );
            }
            menu
        })
}
