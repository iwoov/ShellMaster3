// 基本信息面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::i18n;
use crate::models::server::AuthType;
use crate::models::settings::Language;
use crate::services::storage;

use super::super::helpers::{render_form_label, render_group_select};
use super::super::ServerDialogState;

/// 渲染基本信息表单
pub fn render_basic_info_form(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    use gpui_component::input::Input;

    // 加载当前语言
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let state_read = state.read(cx);
    let auth_type = state_read.auth_type.clone();

    // 预先准备输入框元素
    let loading_text = i18n::t(&lang, "common.loading");

    let label_input = if let Some(input) = &state_read.label_input {
        Input::new(input).into_any_element()
    } else {
        div().child(loading_text).into_any_element()
    };

    let host_input = if let Some(input) = &state_read.host_input {
        Input::new(input).into_any_element()
    } else {
        div().child(loading_text).into_any_element()
    };

    let port_input = if let Some(input) = &state_read.port_input {
        Input::new(input).into_any_element()
    } else {
        div().child(loading_text).into_any_element()
    };

    let username_input = if let Some(input) = &state_read.username_input {
        Input::new(input).into_any_element()
    } else {
        div().child(loading_text).into_any_element()
    };

    let password_input = if let Some(input) = &state_read.password_input {
        Input::new(input).mask_toggle().into_any_element()
    } else {
        div().child(loading_text).into_any_element()
    };

    let private_key_input = if let Some(input) = &state_read.private_key_input {
        Input::new(input).into_any_element()
    } else {
        div().child(loading_text).into_any_element()
    };
    let state_for_file_picker = state.clone();

    let passphrase_input = if let Some(input) = &state_read.passphrase_input {
        Input::new(input).mask_toggle().into_any_element()
    } else {
        div().child(loading_text).into_any_element()
    };

    let state_for_group_dropdown = state.clone();

    div()
        .flex()
        .flex_col()
        .gap_3()
        // 服务器分组（使用独立的 render_group_select 组件）
        // 下拉菜单由 render_group_dropdown_overlay 在对话框顶层渲染
        .child(render_group_select(
            i18n::t(&lang, "server_dialog.group"),
            icons::FOLDER,
            state_read.group_input.as_ref(),
            state_for_group_dropdown,
            loading_text,
            cx,
        ))
        // 服务器标签
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_form_label(
                    i18n::t(&lang, "server_dialog.label"),
                    icons::SERVER,
                    cx,
                ))
                .child(label_input),
        )
        // 主机地址
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_form_label(
                    i18n::t(&lang, "server_dialog.host"),
                    icons::GLOBE,
                    cx,
                ))
                .child(host_input),
        )
        // 端口
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_form_label(
                    i18n::t(&lang, "server_dialog.port"),
                    icons::LINK,
                    cx,
                ))
                .child(port_input),
        )
        // 用户名
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_form_label(
                    i18n::t(&lang, "server_dialog.username"),
                    icons::USER,
                    cx,
                ))
                .child(username_input),
        )
        // 认证方式切换
        .child({
            // 获取主题颜色用于切换按钮
            let toggle_bg = cx.theme().muted;
            let selected_bg = cx.theme().popover;
            let unselected_bg = cx.theme().muted;
            let selected_text = cx.theme().foreground;
            let unselected_text = cx.theme().muted_foreground;

            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_form_label(
                    i18n::t(&lang, "server_dialog.auth_type"),
                    icons::LOCK,
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .p_1()
                        .bg(toggle_bg)
                        .rounded_md()
                        .child(render_auth_type_button(
                            state.clone(),
                            AuthType::Password,
                            i18n::t(&lang, "server_dialog.auth_password"),
                            auth_type == AuthType::Password,
                            selected_bg,
                            unselected_bg,
                            selected_text,
                            unselected_text,
                        ))
                        .child(render_auth_type_button(
                            state.clone(),
                            AuthType::PublicKey,
                            i18n::t(&lang, "server_dialog.auth_key"),
                            auth_type == AuthType::PublicKey,
                            selected_bg,
                            unselected_bg,
                            selected_text,
                            unselected_text,
                        )),
                )
        })
        // 动态渲染认证字段
        .children(match auth_type {
            AuthType::Password => Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(render_form_label(
                        i18n::t(&lang, "server_dialog.password"),
                        icons::LOCK,
                        cx,
                    ))
                    .child(password_input)
                    .into_any_element(),
            ),
            AuthType::PublicKey => Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(render_form_label(
                                i18n::t(&lang, "server_dialog.private_key"),
                                icons::CODE,
                                cx,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(div().flex_1().child(private_key_input))
                                    .child(
                                        // 浏览按钮
                                        div()
                                            .id("browse-private-key-btn")
                                            .px_3()
                                            .py_1p5()
                                            .bg(cx.theme().secondary)
                                            .border_1()
                                            .border_color(cx.theme().border)
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(move |s| s.bg(cx.theme().secondary_hover))
                                            .on_click({
                                                let state = state_for_file_picker.clone();
                                                move |_, _, cx| {
                                                    let state = state.clone();
                                                    // 使用 gpui 原生文件选择 API
                                                    let receiver = cx.prompt_for_paths(
                                                        gpui::PathPromptOptions {
                                                            files: true,
                                                            directories: false,
                                                            multiple: false,
                                                            prompt: Some(
                                                                "Select Private Key File".into(),
                                                            ),
                                                        },
                                                    );
                                                    cx.spawn(async move |cx| {
                                                        if let Ok(Ok(Some(paths))) = receiver.await
                                                        {
                                                            if let Some(path) = paths.first() {
                                                                let path_str = path
                                                                    .to_string_lossy()
                                                                    .to_string();
                                                                // 设置待应用的私钥路径，下次渲染时会应用
                                                                let _ = cx.update(|app| {
                                                                    state.update(app, |s, _| {
                                                                        s.pending_private_key_path =
                                                                            Some(path_str);
                                                                        s.needs_refresh = true;
                                                                    });
                                                                });
                                                            }
                                                        }
                                                    })
                                                    .detach();
                                                }
                                            })
                                            .child(render_icon(
                                                icons::FOLDER_OPEN,
                                                cx.theme().foreground.into(),
                                            )),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(render_form_label(
                                i18n::t(&lang, "server_dialog.passphrase"),
                                icons::LOCK,
                                cx,
                            ))
                            .child(passphrase_input),
                    )
                    .into_any_element(),
            ),
        })
}

/// 渲染认证方式切换按钮
fn render_auth_type_button(
    state: Entity<ServerDialogState>,
    auth_type: AuthType,
    label: &'static str,
    selected: bool,
    selected_bg: gpui::Hsla,
    unselected_bg: gpui::Hsla,
    selected_text: gpui::Hsla,
    unselected_text: gpui::Hsla,
) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .bg(if selected { selected_bg } else { unselected_bg })
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            state.update(cx, |s, _| {
                s.auth_type = auth_type.clone();
            });
        })
        .shadow(if selected {
            vec![BoxShadow {
                color: rgba(0x00000010).into(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
            }]
        } else {
            vec![]
        })
        .child(
            div()
                .text_sm()
                .font_weight(if selected {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if selected {
                    selected_text
                } else {
                    unselected_text
                })
                .child(label),
        )
}
