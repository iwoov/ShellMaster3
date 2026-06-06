// AI 配置面板：每个供应商一行（API Key + Base URL + Model + 测试按钮 + 状态）

use gpui::prelude::*;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::input::{Input, InputState};
use gpui_component::tooltip::Tooltip;
use gpui_component::{ActiveTheme, Disableable};

use crate::i18n;
use crate::models::settings::AiProviderId;

use super::super::helpers::render_section_title;
use super::super::{AiTestStatus, SettingsDialogState};

pub fn render_ai_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let read = state.read(cx);
    let lang = &read.settings.theme.language;

    let mut container = div().flex().flex_col().gap_6();

    // 顶部说明
    container = container.child(
        div()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(i18n::t(lang, "settings.ai.intro")),
    );

    // 保存校验错误（如果有）
    if let Some(err) = &read.ai_save_error {
        container = container.child(
            div()
                .py_2()
                .px_3()
                .rounded_md()
                .bg(cx.theme().danger.opacity(0.15))
                .text_sm()
                .text_color(cx.theme().danger)
                .child(SharedString::from(err.clone())),
        );
    }

    // 横向供应商图标切换
    let active = read.ai_active_provider;
    container = container.child(render_provider_tabs(active, state.clone(), cx));

    // 仅渲染当前选中的供应商配置
    container = container.child(render_provider_block(active, state.clone(), cx));

    // 系统提示词（全局）
    if let Some(input) = read.ai_system_prompt_input.clone() {
        let border = cx.theme().border;
        container = container.child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .border_1()
                .border_color(border)
                .rounded_lg()
                .child(render_section_title("系统提示词", cx))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("每次对话会以 system 消息注入到上下文首部，留空则不注入。"),
                )
                .child(Input::new(&input).appearance(true)),
        );
    }

    container
}

fn render_provider_tabs(
    active: AiProviderId,
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    let primary = cx.theme().primary;
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let default_provider = state.read(cx).settings.ai.default_provider;

    div()
        .flex()
        .items_center()
        .gap_2()
        .pb_2()
        .border_b_1()
        .border_color(border)
        .children(AiProviderId::ALL.into_iter().map(|id| {
            let is_active = id == active;
            let is_default = id == default_provider;
            let label_color = if is_active { primary } else { muted };
            let bg = if is_active { primary.opacity(0.12) } else { gpui::transparent_black() };
            let label: SharedString = if is_default {
                SharedString::from(format!("{} ★", id.label()))
            } else {
                id.label().into()
            };
            let state_for_click = state.clone();
            let tip: SharedString = if is_default {
                SharedString::from(format!("{}（默认）", id.label()))
            } else {
                id.label().into()
            };

            div()
                .id(SharedString::from(format!("ai-tab-{}", id.key())))
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(bg)
                .cursor_pointer()
                .hover(move |s| s.bg(primary.opacity(0.08)))
                .child(crate::components::common::icon::render_colored_icon(id.icon_path(), 24.))
                .child(div().text_xs().text_color(label_color).child(label))
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    state_for_click.update(cx, |s, cx| {
                        s.ai_active_provider = id;
                        cx.notify();
                    });
                })
        }))
}

fn render_provider_block(
    id: AiProviderId,
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    let read = state.read(cx);
    let lang = &read.settings.theme.language;
    let inputs = read.ai_inputs.get(&id);
    let api_key_input = inputs.and_then(|i| i.api_key.clone());
    let base_url_input = inputs.and_then(|i| i.base_url.clone());
    let model_input = inputs.and_then(|i| i.model.clone());
    let status = read
        .ai_test_statuses
        .get(&id)
        .cloned()
        .unwrap_or(AiTestStatus::Untested);
    let is_testing = matches!(status, AiTestStatus::Testing);
    let is_default = read.settings.ai.default_provider == id;

    let border = cx.theme().border;
    let primary = cx.theme().primary;
    let muted = cx.theme().muted_foreground;

    let default_badge = {
        let state_for_set = state.clone();
        if is_default {
            div()
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py_1()
                .rounded(px(4.))
                .bg(primary.opacity(0.15))
                .child(
                    div()
                        .text_xs()
                        .text_color(primary)
                        .child(SharedString::from("★ 默认模型")),
                )
                .into_any_element()
        } else {
            div()
                .id(SharedString::from(format!("ai-set-default-{}", id.key())))
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py_1()
                .rounded(px(4.))
                .cursor_pointer()
                .border_1()
                .border_color(border)
                .hover(move |s| s.bg(muted.opacity(0.12)))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from("设为默认")),
                )
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    state_for_set.update(cx, |s, cx| {
                        s.settings.ai.default_provider = id;
                        s.mark_changed();
                        cx.notify();
                    });
                })
                .into_any_element()
        }
    };

    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .border_1()
        .border_color(border)
        .rounded_lg()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(render_section_title(id.label(), cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(default_badge)
                        .child(render_status_pill(&status, lang, cx)),
                ),
        )
        .children(api_key_input.as_ref().map(|input| {
            render_kv_row(i18n::t(lang, "settings.ai.api_key"), input, cx)
        }))
        .children(base_url_input.as_ref().map(|input| {
            render_kv_row(i18n::t(lang, "settings.ai.base_url"), input, cx)
        }))
        .children(model_input.as_ref().map(|input| {
            render_models_field(lang, input, cx)
        }))
        .child(
            div().flex().justify_end().mt_1().child(
                Button::new(SharedString::from(format!("ai-test-{}", id.key())))
                    .outline()
                    .disabled(is_testing)
                    .child(if is_testing {
                        i18n::t(lang, "settings.ai.testing")
                    } else {
                        i18n::t(lang, "settings.ai.test")
                    })
                    .on_click({
                        let state = state.clone();
                        move |_, _, cx| run_test(id, state.clone(), cx)
                    }),
            ),
        )
}

fn render_kv_row(
    label: &'static str,
    input: &Entity<InputState>,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py_2()
        .px_3()
        .bg(cx.theme().muted)
        .rounded_md()
        .child(
            div()
                .w(px(110.))
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(label),
        )
        .child(div().flex_1().child(Input::new(input).appearance(true)))
}

/// 模型列表编辑（多行文本框，每行一个模型，首行为默认）
fn render_models_field(
    lang: &crate::models::settings::Language,
    input: &Entity<InputState>,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .py_2()
        .px_3()
        .bg(cx.theme().muted)
        .rounded_md()
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(i18n::t(lang, "settings.ai.models")),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(i18n::t(lang, "settings.ai.models_hint")),
        )
        .child(Input::new(input).appearance(true))
}

fn render_status_pill(
    status: &AiTestStatus,
    lang: &crate::models::settings::Language,
    cx: &App,
) -> impl IntoElement {
    let (text, color): (SharedString, gpui::Hsla) = match status {
        AiTestStatus::Untested => (
            i18n::t(lang, "settings.ai.status_untested").into(),
            cx.theme().muted_foreground,
        ),
        AiTestStatus::Testing => (
            i18n::t(lang, "settings.ai.status_testing").into(),
            cx.theme().muted_foreground,
        ),
        AiTestStatus::Pass => (
            i18n::t(lang, "settings.ai.status_pass").into(),
            cx.theme().success,
        ),
        AiTestStatus::Fail(msg) => (
            SharedString::from(format!(
                "{}: {}",
                i18n::t(lang, "settings.ai.status_fail"),
                truncate(msg, 80)
            )),
            cx.theme().danger,
        ),
    };
    div().text_xs().text_color(color).child(text)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

fn run_test(id: AiProviderId, state: Entity<SettingsDialogState>, cx: &mut App) {
    // 1. 从当前输入构造测试配置
    let cfg = {
        let app: &App = cx;
        state.read(app).build_ai_config_from_inputs(id, app)
    };
    if cfg.api_key.trim().is_empty() {
        state.update(cx, |s, cx| {
            s.ai_test_statuses.insert(
                id,
                AiTestStatus::Fail(i18n::t(&s.settings.theme.language, "settings.ai.empty_key").to_string()),
            );
            cx.notify();
        });
        return;
    }

    // 2. 标记测试中
    state.update(cx, |s, cx| {
        s.ai_test_statuses.insert(id, AiTestStatus::Testing);
        cx.notify();
    });

    // 3. 在 tokio 运行时中执行 HTTP 请求；通过 oneshot 把结果送回 GPUI 异步
    let (tx, rx) = tokio::sync::oneshot::channel();
    let cfg_for_task = cfg.clone();
    let runtime = crate::ssh::manager::SshManager::global().runtime();
    runtime.spawn(async move {
        let result = crate::services::ai::test_connection(id, &cfg_for_task).await;
        let _ = tx.send(result);
    });

    cx.spawn({
        let state = state.clone();
        async move |async_cx| {
            let result = rx.await;
            let _ = async_cx.update(|cx| {
                state.update(cx, |s, cx| {
                    let new_status = match result {
                        Ok(Ok(())) => {
                            s.ai_tested_snapshots.insert(
                                id,
                                (cfg.api_key.clone(), cfg.base_url.clone(), cfg.model.clone()),
                            );
                            // 测试成功也清除全局保存错误
                            s.ai_save_error = None;
                            AiTestStatus::Pass
                        }
                        Ok(Err(e)) => AiTestStatus::Fail(e.to_string()),
                        Err(_) => AiTestStatus::Fail("internal channel error".into()),
                    };
                    s.ai_test_statuses.insert(id, new_status);
                    cx.notify();
                });
            });
        }
    })
    .detach();
}
