// 监控面板「系统」组：操作系统、运行时间、连接地址（可复制）

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::view::{section, MonitorCtx};
use crate::components::common::button::{icon_button_custom, icon_button_sized, IconButtonLook};
use crate::components::common::format::format_uptime;
use crate::constants::icons;
use crate::state::SessionState;

/// 地址行右侧复制按钮的边长（含间距）
const COPY_BUTTON: Pixels = px(20.);
const COPY_GAP: Pixels = px(4.);

/// 渲染「系统」组
pub(super) fn render_system_section(ctx: &MonitorCtx, cx: &App) -> impl IntoElement {
    let p = &ctx.p;
    let info = ctx.state.system_info.as_ref();

    let os = info
        .map(|info| info.host.os.clone())
        .filter(|os| !os.is_empty());
    let uptime = ctx
        .state
        .current_uptime()
        .map(|seconds| format_uptime(seconds, &ctx.lang));
    let address = info
        .map(|info| info.host.address.clone())
        .filter(|address| !address.is_empty())
        .or_else(|| {
            ctx.tab
                .server_data
                .as_ref()
                .map(|server| format!("{}@{}:{}", server.username, server.host, server.port))
        });

    let labels = [
        ctx.t("monitor.label.os"),
        ctx.t("monitor.label.uptime"),
        ctx.t("monitor.label.address"),
    ];
    // 标签一栏按最长的标签定宽，值从同一位置开始
    let label_width = labels
        .iter()
        .map(|label| ctx.fit.width(label, 12., FontWeight::NORMAL))
        .fold(px(0.), |widest, width| widest.max(width))
        + px(12.);
    let value_width = (ctx.width - label_width).max(px(0.));

    let copy_key = format!("monitor-address-{}", ctx.tab.id);
    let copied = ctx.session_state.read(cx).is_copied(&copy_key);

    section(
        "monitor-system",
        ctx.t("monitor.section.system"),
        None,
        None,
        ctx,
    )
    .child(
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(info_row(
                "monitor-os",
                labels[0],
                label_width,
                os.as_deref(),
                value_width,
                false,
                ctx,
            ))
            .child(info_row(
                "monitor-uptime",
                labels[1],
                label_width,
                uptime.as_deref(),
                value_width,
                false,
                ctx,
            ))
            .child(
                info_row(
                    "monitor-address",
                    labels[2],
                    label_width,
                    address.as_deref(),
                    (value_width - COPY_BUTTON - COPY_GAP).max(px(0.)),
                    true,
                    ctx,
                )
                .group("monitor-address-row")
                .gap(COPY_GAP)
                .when_some(address, |row, address| {
                    let session_state = ctx.session_state.clone();
                    let button = if copied {
                        icon_button_custom(
                            "monitor-copy-address",
                            icons::CHECK,
                            IconButtonLook {
                                size: COPY_BUTTON,
                                icon_size: 12.,
                                icon_color: p.success,
                                hover_icon_color: p.success,
                                rest_bg: None,
                                enabled: true,
                            },
                            ctx.t("monitor.copied"),
                            p,
                        )
                    } else {
                        icon_button_sized(
                            "monitor-copy-address",
                            icons::COPY,
                            COPY_BUTTON,
                            12.,
                            ctx.t("monitor.copy_address"),
                            p,
                        )
                        .invisible()
                        .group_hover("monitor-address-row", |s| s.visible())
                    };
                    row.child(div().flex_1())
                        .child(button.on_click(move |_, _, cx| {
                            SessionState::copy_with_feedback(
                                &session_state,
                                copy_key.clone(),
                                address.clone(),
                                cx,
                            );
                        }))
                }),
            ),
    )
}

/// 一行键值：标签定宽，值截断后悬停显示全文；没有值时显示弱色的短横线
fn info_row(
    id: &'static str,
    label: &'static str,
    label_width: Pixels,
    value: Option<&str>,
    value_width: Pixels,
    mono: bool,
    ctx: &MonitorCtx,
) -> Div {
    let p = &ctx.p;
    let value: AnyElement = match value {
        Some(value) if mono => ctx
            .fit
            .mono_label_with_tooltip(id, value, value_width, 12.)
            .flex_none()
            .text_color(ctx.value_color())
            .into_any_element(),
        Some(value) => ctx
            .fit
            .label_with_tooltip(id, value, value_width, 12., FontWeight::NORMAL)
            .flex_none()
            .text_color(ctx.value_color())
            .into_any_element(),
        None => div()
            .text_size(px(12.))
            .text_color(p.text_faint)
            .child("—")
            .into_any_element(),
    };

    div()
        .h(px(20.))
        .flex()
        .items_center()
        .child(
            div()
                .flex_none()
                .w(label_width)
                .text_size(px(12.))
                .text_color(p.text_faint)
                .child(label),
        )
        .child(value)
}
