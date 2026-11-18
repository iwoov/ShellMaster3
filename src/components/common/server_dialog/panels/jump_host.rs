// 跳板机：从已保存的服务器中选择一台作为跳板机

use gpui::prelude::*;
use gpui::*;
use gpui_component::switch::Switch;

use crate::components::common::input::{form_field, settings_card, settings_row};
use crate::components::common::text::TextFitter;
use crate::i18n;
use crate::theme::Palette;

use super::super::helpers::{error_line, error_text, hint, notice, radio};
use super::super::{Field, FormErrors, JumpHostCandidate, ServerDialogState};

/// 候选行里除文字外占用的宽度：卡片边框 2 + 左右内边距 28 + 圆点 16 + 间距 12
const CANDIDATE_CHROME_WIDTH: Pixels = px(58.);

/// 渲染跳板机表单
pub fn render_jump_host_form(
    state: Entity<ServerDialogState>,
    errors: &FormErrors,
    form_width: Pixels,
    fit: &TextFitter,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let s = state.read(cx);
    let lang = s.lang.clone();
    let t = |key| i18n::t(&lang, key);
    let enabled = s.enable_jump_host;

    let state_for_toggle = state.clone();
    let toggle = Switch::new("server-jump-host-switch")
        .checked(enabled)
        .on_click(move |checked, _, cx| {
            state_for_toggle.update(cx, |s, cx| {
                s.enable_jump_host = *checked;
                cx.notify();
            });
        });

    let form = div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(settings_card(&p).child(settings_row(
            t("server_dialog.enable_jump_host"),
            Some(t("server_dialog.jump_host_desc").into()),
            toggle,
            &p,
        )));
    if !enabled {
        return form;
    }

    let selected_id = s.jump_host_id.clone();
    let text_width = (form_width - CANDIDATE_CHROME_WIDTH).max(px(0.));
    let candidates = if s.jump_host_candidates.is_empty() {
        notice(t("server_dialog.jump_host_empty"), p.text_muted).into_any_element()
    } else {
        settings_card(&p)
            .children(s.jump_host_candidates.iter().map(|candidate| {
                let selected = selected_id.as_deref() == Some(candidate.id.as_str());
                render_candidate(state.clone(), candidate, selected, text_width, fit, &p)
            }))
            .into_any_element()
    };

    form.child(form_field(
        t("server_dialog.jump_host_select"),
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            // 旧版本保存的跳板机地址匹配不到服务器时提示重新选择
            .children(s.jump_host_unresolved.as_ref().map(|reference| {
                notice(
                    t("server_dialog.jump_host_unresolved").replace("{}", reference),
                    p.warning,
                )
            }))
            .child(candidates)
            .children(error_text(errors, Field::JumpHost, &lang).map(|e| error_line(e, &p))),
        None,
        &p,
    ))
    .child(hint(t("server_dialog.jump_host_hint"), &p))
}

/// 一台可选作跳板机的服务器（单选）：名称 + user@host:port，过长时截断
fn render_candidate(
    state: Entity<ServerDialogState>,
    candidate: &JumpHostCandidate,
    selected: bool,
    text_width: Pixels,
    fit: &TextFitter,
    p: &Palette,
) -> Stateful<Div> {
    let hover_bg = p.hover;
    let candidate_id = candidate.id.clone();

    div()
        .id(SharedString::from(format!("jump-host-{}", candidate.id)))
        // 上移 1px，让分隔线与上一行重叠；第一行的分隔线会被卡片边框裁掉
        .mt(px(-1.))
        .border_t_1()
        .border_color(p.line)
        .min_h(px(52.))
        .px(px(14.))
        .py(px(8.))
        .flex()
        .items_center()
        .gap(px(12.))
        .cursor_pointer()
        .when(selected, |row| row.bg(p.primary.opacity(0.08)))
        .when(!selected, |row| row.hover(move |row| row.bg(hover_bg)))
        .on_click(move |_, _, cx| {
            state.update(cx, |s, cx| {
                s.jump_host_id = Some(candidate_id.clone());
                s.jump_host_unresolved = None;
                cx.notify();
            });
        })
        .child(radio(selected, p))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    fit.label_with_tooltip(
                        SharedString::from(format!("jump-host-label-{}", candidate.id)),
                        &candidate.label,
                        text_width,
                        13.,
                        FontWeight::NORMAL,
                    )
                    .text_color(p.text),
                )
                .child(
                    fit.mono_label_with_tooltip(
                        SharedString::from(format!("jump-host-address-{}", candidate.id)),
                        &candidate.address,
                        text_width,
                        12.,
                    )
                    .text_color(p.text_muted),
                ),
        )
}
