// 跳板机设置面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::services::storage;

use super::super::helpers::{render_form_label, render_switch};
use super::super::{JumpHostCandidate, ServerDialogState};

/// 渲染跳板机设置表单
pub fn render_jump_host_form(state: Entity<ServerDialogState>, cx: &App) -> impl IntoElement {
    // 加载当前语言
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let state_read = state.read(cx);
    let enabled = state_read.enable_jump_host;
    let selected_id = state_read.jump_host_id.clone();
    let candidates = state_read.jump_host_candidates.clone();
    let unresolved = state_read.jump_host_unresolved.clone();

    let muted_foreground = cx.theme().muted_foreground;
    let warn_color: Hsla = rgb(0xf59e0b).into();

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
                        state.update(cx, |s, cx| {
                            s.enable_jump_host = !s.enable_jump_host;
                            cx.notify();
                        });
                    })
                }),
        )
        .children(enabled.then(|| {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_form_label(
                    i18n::t(&lang, "server_dialog.jump_host_select"),
                    icons::SERVER,
                    cx,
                ))
                // 旧版本保存的跳板机地址匹配不到服务器时提示重新选择
                .children(unresolved.map(|reference| {
                    div()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(warn_color.opacity(0.1))
                        .border_1()
                        .border_color(warn_color.opacity(0.4))
                        .text_xs()
                        .text_color(warn_color)
                        .child(
                            i18n::t(&lang, "server_dialog.jump_host_unresolved")
                                .replace("{}", &reference),
                        )
                }))
                .child(if candidates.is_empty() {
                    div()
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(muted_foreground)
                        .child(i18n::t(&lang, "server_dialog.jump_host_empty"))
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(candidates.into_iter().map(|candidate| {
                            let is_selected = selected_id.as_deref() == Some(candidate.id.as_str());
                            render_candidate(state.clone(), candidate, is_selected, cx)
                        }))
                        .into_any_element()
                })
                .child(
                    div()
                        .pt_1()
                        .text_xs()
                        .text_color(muted_foreground)
                        .child(i18n::t(&lang, "server_dialog.jump_host_hint")),
                )
        }))
}

/// 渲染一个可选的跳板机服务器（单选）
fn render_candidate(
    state: Entity<ServerDialogState>,
    candidate: JumpHostCandidate,
    is_selected: bool,
    cx: &App,
) -> impl IntoElement {
    let primary = cx.theme().primary;
    let border = cx.theme().border;
    let hover_bg = cx.theme().muted;
    let foreground = cx.theme().foreground;
    let muted_foreground = cx.theme().muted_foreground;
    let candidate_id = candidate.id.clone();

    div()
        .id(SharedString::from(format!("jump-host-{}", candidate.id)))
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(if is_selected { primary } else { border })
        .when(is_selected, |row| row.bg(primary.opacity(0.08)))
        .cursor_pointer()
        .hover(move |row| if is_selected { row } else { row.bg(hover_bg) })
        .on_click(move |_, _, cx| {
            state.update(cx, |s, cx| {
                s.jump_host_id = Some(candidate_id.clone());
                s.jump_host_unresolved = None;
                cx.notify();
            });
        })
        // 单选圆点
        .child(
            div()
                .size(px(14.))
                .flex_shrink_0()
                .rounded_full()
                .border_1()
                .border_color(if is_selected { primary } else { muted_foreground })
                .flex()
                .items_center()
                .justify_center()
                .when(is_selected, |dot| {
                    dot.child(div().size(px(6.)).rounded_full().bg(primary))
                }),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .text_sm()
                        .text_color(foreground)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(candidate.label),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted_foreground)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(candidate.address),
                ),
        )
}
