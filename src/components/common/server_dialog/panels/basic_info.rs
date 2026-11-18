// 基本信息：名称、地址、账号、认证方式、分组和描述

use gpui::prelude::*;
use gpui::*;
use gpui_component::menu::{DropdownMenu, PopupMenuItem};

use crate::components::common::button::{text_button, ButtonKind};
use crate::components::common::input::{
    dropdown_icon_button, field_error, form_field, segmented, text_field,
};
use crate::i18n;
use crate::models::server::AuthType;
use crate::services::storage;

use super::super::helpers::{error_text, field_pair, input_or_placeholder};
use super::super::{input_text, Field, FormErrors, ServerDialogState};

/// 端口一栏的宽度
const PORT_WIDTH: Pixels = px(110.);
/// 分组菜单的最小宽度
const GROUP_MENU_WIDTH: Pixels = px(220.);

/// 渲染基本信息表单
pub fn render_basic_info_form(
    state: Entity<ServerDialogState>,
    errors: &FormErrors,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let s = state.read(cx);
    let lang = s.lang.clone();
    let t = |key| i18n::t(&lang, key);
    let error = |field| error_text(errors, field, &lang);

    let host_error = error(Field::Host);
    let port_error = error(Field::Port);
    let username_error = error(Field::Username);
    let key_error =
        error(Field::PrivateKey).or_else(|| s.key_error.clone().map(SharedString::from));

    // 名称
    let name = form_field(
        t("server_dialog.label"),
        input_or_placeholder(s.label_input.as_ref(), text_field),
        None,
        &p,
    );

    // 主机地址 + 端口
    let address = field_pair(
        form_field(
            t("server_dialog.host"),
            input_or_placeholder(s.host_input.as_ref(), |input| {
                field_error(text_field(input), host_error.is_some(), &p)
            }),
            None,
            &p,
        ),
        form_field(
            t("server_dialog.port"),
            input_or_placeholder(s.port_input.as_ref(), |input| {
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

    // 用户名
    let username = form_field(
        t("server_dialog.username"),
        input_or_placeholder(s.username_input.as_ref(), |input| {
            field_error(text_field(input), username_error.is_some(), &p)
        }),
        username_error,
        &p,
    );

    // 认证方式
    let state_for_auth = state.clone();
    let auth_type = form_field(
        t("server_dialog.auth_type"),
        // 包一层横向容器：分段切换保持自身宽度，不被纵向排列拉满
        div().flex().child(segmented(
            "server-auth-type",
            vec![
                (AuthType::Password, t("server_dialog.auth_password").into()),
                (AuthType::PublicKey, t("server_dialog.auth_key").into()),
            ],
            &s.auth_type,
            false,
            move |auth_type, _, cx| {
                state_for_auth.update(cx, |s, cx| {
                    s.auth_type = auth_type;
                    cx.notify();
                });
            },
            &p,
        )),
        None,
        &p,
    );

    // 密码，或私钥文件 + 私钥密码
    let credentials = if s.auth_type == AuthType::PublicKey {
        let state_for_browse = state.clone();
        let key_row = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().flex_1().min_w_0().child(input_or_placeholder(
                s.private_key_input.as_ref(),
                |input| field_error(text_field(input), key_error.is_some(), &p),
            )))
            .child(
                text_button(
                    "server-browse-key-btn",
                    ButtonKind::Secondary,
                    t("server_dialog.browse"),
                    &p,
                )
                .on_click(move |_, _, cx| pick_private_key(state_for_browse.clone(), cx)),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(form_field(
                t("server_dialog.private_key"),
                key_row,
                key_error,
                &p,
            ))
            .child(form_field(
                t("server_dialog.passphrase"),
                input_or_placeholder(s.passphrase_input.as_ref(), |input| {
                    text_field(input).mask_toggle()
                }),
                None,
                &p,
            ))
    } else {
        form_field(
            t("server_dialog.password"),
            input_or_placeholder(s.password_input.as_ref(), |input| {
                text_field(input).mask_toggle()
            }),
            None,
            &p,
        )
    };

    // 分组：可直接输入新分组，右侧按钮弹出已有分组
    let group_menu = (!s.available_groups.is_empty()).then(|| {
        let groups = s.available_groups.clone();
        let group_input = s.group_input.clone();
        let current = input_text(&s.group_input, cx);
        dropdown_icon_button("server-group-menu-btn", cx).dropdown_menu_with_anchor(
            Corner::TopRight,
            move |menu, _, _| {
                let mut menu = menu.min_w(GROUP_MENU_WIDTH);
                for name in &groups {
                    let group_input = group_input.clone();
                    let value = name.clone();
                    menu = menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(name.trim() == current.trim())
                            .on_click(move |_, window, cx| {
                                if let Some(input) = &group_input {
                                    input.update(cx, |input, cx| {
                                        input.set_value(value.clone(), window, cx)
                                    });
                                }
                            }),
                    );
                }
                menu
            },
        )
    });
    let group = form_field(
        t("server_dialog.group"),
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(input_or_placeholder(s.group_input.as_ref(), text_field)),
            )
            .children(group_menu),
        None,
        &p,
    );

    // 描述
    let description = form_field(
        t("server_dialog.description"),
        input_or_placeholder(s.description_input.as_ref(), text_field),
        None,
        &p,
    );

    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(name)
        .child(address)
        .child(username)
        .child(auth_type)
        .child(credentials)
        .child(group)
        .child(description)
}

/// 选择私钥文件：复制到应用的密钥目录，再把复制后的文件名填进输入框
fn pick_private_key(state: Entity<ServerDialogState>, cx: &mut App) {
    let prompt = i18n::t(&state.read(cx).lang, "server_dialog.pick_private_key");
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some(prompt.into()),
    });
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = receiver.await else {
            return;
        };
        let Some(path) = paths.first() else {
            return;
        };
        let result = storage::store_private_key(path);
        let _ = cx.update(|cx| {
            state.update(cx, |s, cx| {
                match result {
                    Ok(filename) => {
                        s.pending_private_key_path = Some(filename);
                        s.key_error = None;
                    }
                    Err(err) => {
                        s.key_error = Some(format!(
                            "{}{:#}",
                            i18n::t(&s.lang, "server_dialog.key_import_failed"),
                            err
                        ));
                    }
                }
                cx.notify();
            });
        });
    })
    .detach();
}
