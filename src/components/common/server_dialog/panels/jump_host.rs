// 跳板机设置面板

use gpui::prelude::*;
use gpui::*;

use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::services::storage;

use super::super::helpers::{render_form_label, render_switch};
use super::super::ServerDialogState;

/// 渲染跳板机设置表单
pub fn render_jump_host_form(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    use gpui_component::input::Input;

    // 加载当前语言
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let state_read = state.read(cx);
    let enabled = state_read.enable_jump_host;

    let loading_text = i18n::t(&lang, "common.loading");
    let jump_host_input = if let Some(input) = &state_read.jump_host_input {
        Input::new(input).into_any_element()
    } else {
        div().child(loading_text).into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(render_form_label(
                    i18n::t(&lang, "server_dialog.enable_jump_host"),
                    icons::LINK,
                    cx,
                ))
                .child({
                    let state = state.clone();
                    render_switch(enabled, move |_, _, cx| {
                        state.update(cx, |s, _| {
                            s.enable_jump_host = !s.enable_jump_host;
                        });
                    })
                }),
        )
        .children(if enabled {
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(render_form_label(
                        i18n::t(&lang, "server_dialog.jump_host_address"),
                        icons::SERVER,
                        cx,
                    ))
                    .child(jump_host_input),
            )
        } else {
            None
        })
}
