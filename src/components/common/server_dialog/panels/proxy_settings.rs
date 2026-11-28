// 代理：通过 HTTP 或 SOCKS5 代理连接

use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::component::switch::Switch;

use crate::components::common::input::{
    field_error, form_field, segmented, settings_card, settings_row, text_field,
};
use crate::i18n;
use crate::models::server::ProxyType;

use super::super::helpers::{error_text, field_pair, input_or_placeholder, notice};
use super::super::{Field, FormErrors, ServerDialogState};

/// 端口一栏的宽度
const PORT_WIDTH: Pixels = px(110.);

/// 渲染代理表单
pub fn render_proxy_settings_form(
    state: Entity<ServerDialogState>,
    errors: &FormErrors,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let s = state.read(cx);
    let lang = s.lang.clone();
    let t = |key| i18n::t(&lang, key);
    let enabled = s.enable_proxy;

    let state_for_toggle = state.clone();
    let toggle = Switch::new("server-proxy-switch")
        .checked(enabled)
        .on_click(move |checked, _, cx| {
            state_for_toggle.update(cx, |s, cx| {
                s.enable_proxy = *checked;
                cx.notify();
            });
        });

    let form = div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(settings_card(&p).child(settings_row(
            t("server_dialog.enable_proxy"),
            Some(t("server_dialog.proxy_desc").into()),
            toggle,
            &p,
        )));
    if !enabled {
        return form;
    }

    let host_error = error_text(errors, Field::ProxyHost, &lang);
    let port_error = error_text(errors, Field::ProxyPort, &lang);

    let state_for_type = state.clone();
    let proxy_type = form_field(
        t("server_dialog.proxy_type"),
        div().flex().child(segmented(
            "server-proxy-type",
            vec![
                (ProxyType::Http, "HTTP".into()),
                (ProxyType::Socks5, "SOCKS5".into()),
            ],
            &s.proxy_type,
            false,
            move |proxy_type, _, cx| {
                state_for_type.update(cx, |s, cx| {
                    s.proxy_type = proxy_type;
                    cx.notify();
                });
            },
            &p,
        )),
        None,
        &p,
    );

    let address = field_pair(
        form_field(
            t("server_dialog.proxy_host"),
            input_or_placeholder(s.proxy_host_input.as_ref(), |input| {
                field_error(text_field(input), host_error.is_some(), &p)
            }),
            None,
            &p,
        ),
        form_field(
            t("server_dialog.proxy_port"),
            input_or_placeholder(s.proxy_port_input.as_ref(), |input| {
                field_error(text_field(input), port_error.is_some(), &p)
            }),
            None,
            &p,
        ),
        PORT_WIDTH,
        [host_error.clone(), port_error.clone()]
            .into_iter()
            .flatten(),
        &p,
    );

    form
        // 跳板机优先：同时启用时连接不经过这个代理
        .when(s.enable_jump_host, |form| {
            form.child(notice(t("server_dialog.proxy_ignored"), p.warning))
        })
        .child(proxy_type)
        .child(address)
        .child(form_field(
            t("server_dialog.proxy_username"),
            input_or_placeholder(s.proxy_username_input.as_ref(), text_field),
            None,
            &p,
        ))
        .child(form_field(
            t("server_dialog.proxy_password"),
            input_or_placeholder(s.proxy_password_input.as_ref(), |input| {
                text_field(input).mask_toggle()
            }),
            None,
            &p,
        ))
}
