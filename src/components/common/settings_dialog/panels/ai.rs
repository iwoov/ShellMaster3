// AI 配置面板：内置供应商 + 自定义供应商（可选协议格式、字母头像）

use gpui::prelude::*;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::input::{Input, InputState};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::tooltip::Tooltip;
use gpui_component::{ActiveTheme, Disableable};

use crate::components::common::icon::{render_colored_icon, render_letter_avatar};
use crate::i18n;
use crate::models::settings::{AiProviderId, ApiFormat, ProviderRef};

use super::super::helpers::{render_number_row, render_section_title, render_switch_row};
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

    // 横向供应商图标切换（内置 + 自定义 + 添加）
    let active = read.ai_active_provider.clone();
    container = container.child(render_provider_tabs(active.clone(), state.clone(), cx));

    // 仅渲染当前选中的供应商配置
    container = container.child(render_provider_block(active, state.clone(), cx));

    // 终端上下文（全局）
    {
        let border = cx.theme().border;
        let ctx = read.settings.ai.context.clone();
        let mut block = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .border_1()
            .border_color(border)
            .rounded_lg()
            .child(render_section_title(i18n::t(lang, "settings.ai.context.title"), cx))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .mb_2()
                    .child(i18n::t(lang, "settings.ai.context.hint")),
            )
            .child(render_switch_row(
                "ai-ctx-enabled",
                i18n::t(lang, "settings.ai.context.enabled"),
                ctx.enabled,
                state.clone(),
                |s, v| s.settings.ai.context.enabled = v,
                cx,
            ));

        if ctx.enabled {
            block = block
                .child(render_switch_row(
                    "ai-ctx-server",
                    i18n::t(lang, "settings.ai.context.server_info"),
                    ctx.server_info,
                    state.clone(),
                    |s, v| s.settings.ai.context.server_info = v,
                    cx,
                ))
                .child(render_switch_row(
                    "ai-ctx-os",
                    i18n::t(lang, "settings.ai.context.os_info"),
                    ctx.os_info,
                    state.clone(),
                    |s, v| s.settings.ai.context.os_info = v,
                    cx,
                ))
                .child(render_switch_row(
                    "ai-ctx-output",
                    i18n::t(lang, "settings.ai.context.terminal_output"),
                    ctx.terminal_output,
                    state.clone(),
                    |s, v| s.settings.ai.context.terminal_output = v,
                    cx,
                ));
            if ctx.terminal_output {
                if let Some(input) = read.ai_context_lines_input.clone() {
                    block = block.child(render_number_row(
                        i18n::t(lang, "settings.ai.context.output_lines"),
                        &input,
                        cx,
                    ));
                }
            }
        }

        container = container.child(block);
    }

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
    active: ProviderRef,
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    let primary = cx.theme().primary;
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let foreground = cx.theme().foreground;
    let read = state.read(cx);
    let default_provider = read.settings.ai.default_provider;
    let lang = read.settings.theme.language.clone();

    let mut row = div()
        .flex()
        .items_center()
        .flex_wrap()
        .gap_2()
        .pb_2()
        .border_b_1()
        .border_color(border);

    // 内置供应商
    for id in AiProviderId::ALL {
        let r = ProviderRef::Builtin(id);
        let is_active = active == r;
        let is_default = id == default_provider;
        let label_color = if is_active { primary } else { muted };
        let bg = if is_active {
            primary.opacity(0.12)
        } else {
            gpui::transparent_black()
        };
        let label: SharedString = if is_default {
            SharedString::from(format!("{} ★", id.label()))
        } else {
            id.label().into()
        };
        let state_for_click = state.clone();
        row = row.child(
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
                .child(render_colored_icon(id.icon_path(), 24.))
                .child(div().text_xs().text_color(label_color).child(label))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    state_for_click.update(cx, |s, cx| {
                        s.ai_active_provider = ProviderRef::Builtin(id);
                        cx.notify();
                    });
                }),
        );
    }

    // 自定义供应商
    let customs: Vec<(String, String)> = read
        .settings
        .ai
        .custom_providers
        .iter()
        .map(|c| (c.id.clone(), c.name.clone()))
        .collect();
    for (cid, name) in customs {
        let r = ProviderRef::Custom(cid.clone());
        let is_active = active == r;
        let label_color = if is_active { primary } else { muted };
        let bg = if is_active {
            primary.opacity(0.12)
        } else {
            gpui::transparent_black()
        };
        let display: SharedString = if name.trim().is_empty() {
            SharedString::from("未命名")
        } else {
            SharedString::from(name.clone())
        };
        let avatar = ProviderRef::avatar_char(&name);
        let state_for_click = state.clone();
        let cid_for_click = cid.clone();
        row = row.child(
            div()
                .id(SharedString::from(format!("ai-tab-custom-{}", cid)))
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
                .child(render_letter_avatar(avatar, 24.))
                .child(div().text_xs().text_color(label_color).child(display))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    let cid = cid_for_click.clone();
                    state_for_click.update(cx, |s, cx| {
                        s.ai_active_provider = ProviderRef::Custom(cid);
                        cx.notify();
                    });
                }),
        );
    }

    // 添加自定义供应商
    let state_for_add = state.clone();
    let add_tip: SharedString = i18n::t(&lang, "settings.ai.custom_add").into();
    row = row.child(
        div()
            .id("ai-tab-add")
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_1()
            .px_3()
            .py_2()
            .rounded_md()
            .cursor_pointer()
            .border_1()
            .border_color(border)
            .hover(move |s| s.bg(primary.opacity(0.08)))
            .child(
                svg()
                    .path(crate::constants::icons::PLUS)
                    .size(px(20.))
                    .text_color(foreground),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(i18n::t(&lang, "settings.ai.custom_add")),
            )
            .tooltip(move |window, cx| Tooltip::new(add_tip.clone()).build(window, cx))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                state_for_add.update(cx, |s, cx| {
                    let r = s.add_custom_provider();
                    s.ai_active_provider = r;
                    s.mark_changed();
                    cx.notify();
                });
            }),
    );

    row
}

fn render_provider_block(
    r: ProviderRef,
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    let read = state.read(cx);
    let lang = &read.settings.theme.language;
    let inputs = read.ai_inputs.get(&r);
    let api_key_input = inputs.and_then(|i| i.api_key.clone());
    let base_url_input = inputs.and_then(|i| i.base_url.clone());
    let model_input = inputs.and_then(|i| i.model.clone());
    let name_input = inputs.and_then(|i| i.name.clone());
    let status = read
        .ai_test_statuses
        .get(&r)
        .cloned()
        .unwrap_or(AiTestStatus::Untested);
    let is_testing = matches!(status, AiTestStatus::Testing);

    let border = cx.theme().border;
    let primary = cx.theme().primary;
    let muted = cx.theme().muted_foreground;

    let is_custom = matches!(r, ProviderRef::Custom(_));
    // 标题：内置用品牌名；自定义用名称输入框的当前值或占位
    let title: String = match &r {
        ProviderRef::Builtin(id) => id.label().to_string(),
        ProviderRef::Custom(_) => i18n::t(lang, "settings.ai.custom_name").to_string(),
    };

    // 头部右侧：内置可设默认；自定义可删除
    let head_action = match &r {
        ProviderRef::Builtin(id) => {
            let id = *id;
            let is_default = read.settings.ai.default_provider == id;
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
        }
        ProviderRef::Custom(cid) => {
            let cid = cid.clone();
            let state_for_del = state.clone();
            let danger = cx.theme().danger;
            div()
                .id(SharedString::from(format!("ai-del-{}", cid)))
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py_1()
                .rounded(px(4.))
                .cursor_pointer()
                .border_1()
                .border_color(border)
                .hover(move |s| s.bg(danger.opacity(0.12)))
                .child(
                    svg()
                        .path(crate::constants::icons::TRASH)
                        .size(px(14.))
                        .text_color(danger),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(danger)
                        .child(i18n::t(lang, "settings.ai.delete")),
                )
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    let cid = cid.clone();
                    state_for_del.update(cx, |s, cx| {
                        // 删除后回到第一个内置供应商
                        s.remove_custom_provider(&cid);
                        s.ai_active_provider = ProviderRef::Builtin(AiProviderId::OpenAi);
                        s.mark_changed();
                        cx.notify();
                    });
                })
                .into_any_element()
        }
    };

    let mut block = div()
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
                .child(render_section_title(&title, cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(head_action)
                        .child(render_status_pill(&status, lang, cx)),
                ),
        );

    // 自定义：名称 + 协议格式
    if is_custom {
        if let Some(input) = name_input.as_ref() {
            block = block.child(render_kv_row(i18n::t(lang, "settings.ai.custom_name"), input, cx));
        }
        let current_format = read
            .ai_inputs
            .get(&r)
            .and_then(|i| i.format)
            .unwrap_or_default();
        block = block.child(render_format_row(r.clone(), current_format, state.clone(), lang, cx));
    }

    block
        .children(
            api_key_input
                .as_ref()
                .map(|input| render_kv_row(i18n::t(lang, "settings.ai.api_key"), input, cx)),
        )
        .children(
            base_url_input
                .as_ref()
                .map(|input| render_kv_row(i18n::t(lang, "settings.ai.base_url"), input, cx)),
        )
        .children(model_input.as_ref().map(|input| render_models_field(lang, input, cx)))
        .child(
            div().flex().justify_end().mt_1().child(
                Button::new(SharedString::from(format!("ai-test-{:?}", r)))
                    .outline()
                    .disabled(is_testing)
                    .child(if is_testing {
                        i18n::t(lang, "settings.ai.testing")
                    } else {
                        i18n::t(lang, "settings.ai.test")
                    })
                    .on_click({
                        let state = state.clone();
                        let r = r.clone();
                        move |_, _, cx| run_test(r.clone(), state.clone(), cx)
                    }),
            ),
        )
}

/// 协议格式下拉行
fn render_format_row(
    r: ProviderRef,
    current: ApiFormat,
    state: Entity<SettingsDialogState>,
    lang: &crate::models::settings::Language,
    cx: &App,
) -> impl IntoElement {
    let muted_fg = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
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
                .text_color(foreground)
                .child(i18n::t(lang, "settings.ai.format")),
        )
        .child(
            Button::new(SharedString::from(format!("ai-format-{:?}", r)))
                .outline()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(div().text_sm().text_color(foreground).child(current.label()))
                        .child(
                            svg()
                                .path(crate::constants::icons::CHEVRON_DOWN)
                                .size(px(14.))
                                .text_color(muted_fg),
                        ),
                )
                .dropdown_menu_with_anchor(gpui::Corner::TopRight, move |menu, _, _| {
                    let mut menu = menu.min_w(px(200.));
                    for fmt in ApiFormat::ALL {
                        let label: SharedString = fmt.label().into();
                        let state = state.clone();
                        let r = r.clone();
                        menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                            let r = r.clone();
                            state.update(cx, |s, cx| {
                                s.ai_inputs.entry(r).or_default().format = Some(fmt);
                                s.mark_changed();
                                cx.notify();
                            });
                        }));
                    }
                    menu
                }),
        )
}

fn render_kv_row(label: &str, input: &Entity<InputState>, cx: &App) -> impl IntoElement {
    let label = label.to_string();
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

fn run_test(r: ProviderRef, state: Entity<SettingsDialogState>, cx: &mut App) {
    // 1. 从当前输入收集测试配置
    let values = {
        let app: &App = cx;
        state.read(app).collect_ai_inputs(&r, app)
    };
    if values.api_key.trim().is_empty() {
        let r2 = r.clone();
        state.update(cx, |s, cx| {
            s.ai_test_statuses.insert(
                r2,
                AiTestStatus::Fail(
                    i18n::t(&s.settings.theme.language, "settings.ai.empty_key").to_string(),
                ),
            );
            cx.notify();
        });
        return;
    }

    // 测试用 base_url：内置为空时填默认
    let test_base = if values.base_url.trim().is_empty() {
        match &r {
            ProviderRef::Builtin(id) => id.default_base_url().to_string(),
            ProviderRef::Custom(_) => String::new(),
        }
    } else {
        values.base_url.clone()
    };

    // 2. 标记测试中
    let r_for_mark = r.clone();
    state.update(cx, |s, cx| {
        s.ai_test_statuses.insert(r_for_mark, AiTestStatus::Testing);
        cx.notify();
    });

    // 3. tokio 运行时执行 HTTP；oneshot 回传
    let (tx, rx) = tokio::sync::oneshot::channel();
    let format = values.format;
    let api_key = values.api_key.clone();
    let runtime = crate::ssh::manager::SshManager::global().runtime();
    runtime.spawn(async move {
        let result = crate::services::ai::test_connection(format, &api_key, &test_base).await;
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
                                r.clone(),
                                (
                                    values.api_key.clone(),
                                    values.base_url.clone(),
                                    values.model.clone(),
                                    values.format.label().to_string(),
                                ),
                            );
                            s.ai_save_error = None;
                            AiTestStatus::Pass
                        }
                        Ok(Err(e)) => AiTestStatus::Fail(e.to_string()),
                        Err(_) => AiTestStatus::Fail("internal channel error".into()),
                    };
                    s.ai_test_statuses.insert(r.clone(), new_status);
                    cx.notify();
                });
            });
        }
    })
    .detach();
}
