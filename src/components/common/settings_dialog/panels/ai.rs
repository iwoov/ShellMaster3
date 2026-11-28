// AI 配置面板：内置供应商 + 自定义供应商（可选协议格式、字母头像）

use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

use crate::components::common::button::{button, icon_button_colored, text_tooltip, ButtonKind};
use crate::components::common::icon::{icon_sized, render_colored_icon, render_letter_avatar};
use crate::components::common::input::{select_trigger, text_area, text_field};
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::{AiProviderId, ApiFormat, Language, ProviderRef};
use crate::models::{AgentApprovalMode, AgentReasoningEffort};
use crate::theme::Palette;

use super::super::helpers::{
    render_block_row, render_card, render_hint, render_number_row, render_row,
    render_section_title, render_select_row, render_switch_row, CONTROL_WIDTH,
};
use super::super::{AiTestStatus, SettingsDialogState};

/// API Key、Base URL 输入框的宽度
const WIDE_FIELD_WIDTH: Pixels = px(320.);
/// 供应商标签里名称的最大字符数
const PROVIDER_NAME_MAX_CHARS: usize = 18;

pub fn render_ai_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let read = state.read(cx);
    let lang = read.settings.theme.language.clone();
    let active = read.ai_active_provider.clone();

    let mut container = div().flex().flex_col().gap_6();

    // 顶部说明，以及保存校验错误（如果有）
    container = container.child(
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(i18n::t(&lang, "settings.ai.intro")),
            )
            .children(read.ai_save_error.clone().map(|err| {
                div()
                    .px(px(12.))
                    .py(px(9.))
                    .rounded(px(8.))
                    .bg(p.danger.opacity(0.1))
                    .border_1()
                    .border_color(p.danger.opacity(0.3))
                    .text_size(px(12.))
                    .text_color(p.danger)
                    .child(err)
            })),
    );

    // 供应商：横向标签切换（内置 + 自定义 + 添加），下方是当前供应商的配置
    container = container.child(
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(render_section_title(
                i18n::t(&lang, "settings.ai.providers"),
                cx,
            ))
            .child(render_provider_tabs(active.clone(), state.clone(), &p, cx))
            .child(render_provider_card(active, state.clone(), &p, cx)),
    );

    // Agent 行为与安全（全局）
    {
        let agent = read.settings.ai.agent.clone();
        let approval_options: Vec<(AgentApprovalMode, &'static str)> = AgentApprovalMode::ALL
            .iter()
            .map(|m| (*m, i18n::t(&lang, m.label_key())))
            .collect();
        let reasoning_options: Vec<(AgentReasoningEffort, &'static str)> =
            AgentReasoningEffort::ALL
                .iter()
                .map(|r| (*r, i18n::t(&lang, r.label_key())))
                .collect();

        let mut card = render_card(cx)
            .child(render_select_row(
                "ai-agent-approval-mode",
                i18n::t(&lang, "settings.ai.agent.approval_mode"),
                &agent.approval_mode,
                approval_options,
                state.clone(),
                |s, v| s.settings.ai.agent.approval_mode = v,
                cx,
            ))
            .child(render_hint(i18n::t(&lang, agent.approval_mode.desc_key()), cx))
            .child(render_select_row(
                "ai-agent-reasoning-effort",
                i18n::t(&lang, "settings.ai.agent.reasoning_effort"),
                &agent.reasoning_effort,
                reasoning_options,
                state.clone(),
                |s, v| s.settings.ai.agent.reasoning_effort = v,
                cx,
            ))
            .child(render_hint(
                i18n::t(&lang, agent.reasoning_effort.desc_key()),
                cx,
            ))
            .child(render_switch_row(
                "ai-agent-pty-tools",
                i18n::t(&lang, "settings.ai.agent.enable_pty_tools"),
                agent.enable_pty_tools,
                state.clone(),
                |s, v| s.settings.ai.agent.enable_pty_tools = v,
                cx,
            ))
            .child(render_switch_row(
                "ai-agent-exec-tool",
                i18n::t(&lang, "settings.ai.agent.enable_exec_tool"),
                agent.enable_exec_tool,
                state.clone(),
                |s, v| s.settings.ai.agent.enable_exec_tool = v,
                cx,
            ));

        if let Some(input) = read.ai_agent_max_turns_input.clone() {
            card = card.child(render_number_row(
                i18n::t(&lang, "settings.ai.agent.max_turns"),
                &input,
                cx,
            ));
        }
        if let Some(input) = read.ai_agent_timeout_input.clone() {
            card = card.child(render_number_row(
                i18n::t(&lang, "settings.ai.agent.command_timeout"),
                &input,
                cx,
            ));
        }

        container = container.child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(&lang, "settings.ai.agent.title"),
                    cx,
                ))
                .child(
                    div()
                        .px(px(2.))
                        .text_size(px(12.))
                        .text_color(p.text_muted)
                        .child(i18n::t(&lang, "settings.ai.agent.hint")),
                )
                .child(card),
        );
    }

    // 终端上下文（全局）
    {
        let ctx = read.settings.ai.context.clone();
        let mut card = render_card(cx).child(render_switch_row(
            "ai-ctx-enabled",
            i18n::t(&lang, "settings.ai.context.enabled"),
            ctx.enabled,
            state.clone(),
            |s, v| s.settings.ai.context.enabled = v,
            cx,
        ));

        if ctx.enabled {
            card = card
                .child(render_switch_row(
                    "ai-ctx-server",
                    i18n::t(&lang, "settings.ai.context.server_info"),
                    ctx.server_info,
                    state.clone(),
                    |s, v| s.settings.ai.context.server_info = v,
                    cx,
                ))
                .child(render_switch_row(
                    "ai-ctx-os",
                    i18n::t(&lang, "settings.ai.context.os_info"),
                    ctx.os_info,
                    state.clone(),
                    |s, v| s.settings.ai.context.os_info = v,
                    cx,
                ))
                .child(render_switch_row(
                    "ai-ctx-output",
                    i18n::t(&lang, "settings.ai.context.terminal_output"),
                    ctx.terminal_output,
                    state.clone(),
                    |s, v| s.settings.ai.context.terminal_output = v,
                    cx,
                ));
            if ctx.terminal_output {
                if let Some(input) = read.ai_context_lines_input.clone() {
                    card = card.child(render_number_row(
                        i18n::t(&lang, "settings.ai.context.output_lines"),
                        &input,
                        cx,
                    ));
                }
            }
        }

        container = container.child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(&lang, "settings.ai.context.title"),
                    cx,
                ))
                .child(
                    div()
                        .px(px(2.))
                        .text_size(px(12.))
                        .text_color(p.text_muted)
                        .child(i18n::t(&lang, "settings.ai.context.hint")),
                )
                .child(card),
        );
    }

    // 系统提示词（全局）
    if let Some(input) = read.ai_system_prompt_input.clone() {
        container = container.child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(&lang, "settings.ai.system_prompt"),
                    cx,
                ))
                .child(
                    render_card(cx).child(render_block_row(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(p.text_muted)
                                    .child(i18n::t(&lang, "settings.ai.system_prompt_hint")),
                            )
                            .child(text_area(&input)),
                        cx,
                    )),
                ),
        );
    }

    container
}

/// 供应商标签（一个可点击的小卡片）
fn provider_tab(
    id: SharedString,
    icon: AnyElement,
    label: SharedString,
    selected: bool,
    is_default: bool,
    default_label: &'static str,
    p: &Palette,
) -> Stateful<Div> {
    let hover_bg = p.hover;
    div()
        .id(id)
        .flex_none()
        .h(px(32.))
        .pl(px(8.))
        .pr(px(10.))
        .rounded(px(8.))
        .border_1()
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(px(7.))
        .when(selected, |tab| {
            tab.bg(p.selected)
                .border_color(crate::theme::mix(p.border, p.primary, 0.6))
        })
        .when(!selected, |tab| {
            tab.bg(p.surface)
                .border_color(p.border)
                .hover(move |s| s.bg(hover_bg))
        })
        .child(icon)
        .child(
            div()
                .whitespace_nowrap()
                .text_size(px(13.))
                .text_color(if selected { p.text } else { p.text_muted })
                .when(selected, |label| label.font_weight(FontWeight::MEDIUM))
                .child(label),
        )
        .when(is_default, |tab| {
            tab.child(status_tag(default_label.into(), p.primary))
        })
}

/// 小标签：状态、默认等
fn status_tag(text: SharedString, color: Hsla) -> Div {
    div()
        .flex_none()
        .h(px(18.))
        .px(px(6.))
        .rounded(px(5.))
        .bg(color.opacity(0.14))
        .flex()
        .items_center()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .whitespace_nowrap()
        .child(text)
}

fn render_provider_tabs(
    active: ProviderRef,
    state: Entity<SettingsDialogState>,
    p: &Palette,
    cx: &App,
) -> impl IntoElement {
    let read = state.read(cx);
    let default_provider = read.settings.ai.default_provider;
    let lang = read.settings.theme.language.clone();
    let default_label = i18n::t(&lang, "settings.ai.default");

    let mut row = div().flex().items_center().flex_wrap().gap(px(6.));

    // 内置供应商
    for id in AiProviderId::ALL {
        let state_for_click = state.clone();
        row = row.child(
            provider_tab(
                SharedString::from(format!("ai-tab-{}", id.key())),
                render_colored_icon(id.icon_path(), 16.).into_any_element(),
                id.label().into(),
                active == ProviderRef::Builtin(id),
                id == default_provider,
                default_label,
                p,
            )
            .on_click(move |_, _, cx| {
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
        let display: SharedString = if name.trim().is_empty() {
            i18n::t(&lang, "settings.ai.unnamed").into()
        } else {
            truncate(&name, PROVIDER_NAME_MAX_CHARS).into()
        };
        let state_for_click = state.clone();
        let cid_for_click = cid.clone();
        row = row.child(
            provider_tab(
                SharedString::from(format!("ai-tab-custom-{}", cid)),
                render_letter_avatar(ProviderRef::avatar_char(&name), 16.).into_any_element(),
                display,
                active == ProviderRef::Custom(cid.clone()),
                false,
                default_label,
                p,
            )
            .on_click(move |_, _, cx| {
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
    row.child(
        provider_tab(
            "ai-tab-add".into(),
            icon_sized(icons::PLUS, 14., p.text_muted).into_any_element(),
            i18n::t(&lang, "settings.ai.custom_add").into(),
            false,
            false,
            default_label,
            p,
        )
        .on_click(move |_, _, cx| {
            state_for_add.update(cx, |s, cx| {
                let r = s.add_custom_provider();
                s.ai_active_provider = r;
                s.mark_changed();
                cx.notify();
            });
        }),
    )
}

fn render_provider_card(
    r: ProviderRef,
    state: Entity<SettingsDialogState>,
    p: &Palette,
    cx: &App,
) -> impl IntoElement {
    let read = state.read(cx);
    let lang = read.settings.theme.language.clone();
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

    // 头部：图标 + 名称 + 测试状态；右侧是设为默认（内置）或删除（自定义）
    let (icon, title): (AnyElement, SharedString) = match &r {
        ProviderRef::Builtin(id) => (
            render_colored_icon(id.icon_path(), 20.).into_any_element(),
            id.label().into(),
        ),
        ProviderRef::Custom(cid) => {
            let name = read
                .settings
                .ai
                .custom_providers
                .iter()
                .find(|c| &c.id == cid)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let title = if name.trim().is_empty() {
                i18n::t(&lang, "settings.ai.unnamed").to_string()
            } else {
                truncate(&name, 40)
            };
            (
                render_letter_avatar(ProviderRef::avatar_char(&name), 20.).into_any_element(),
                title.into(),
            )
        }
    };

    let head_action: AnyElement = match &r {
        ProviderRef::Builtin(id) => {
            let id = *id;
            if read.settings.ai.default_provider == id {
                status_tag(i18n::t(&lang, "settings.ai.default").into(), p.primary)
                    .into_any_element()
            } else {
                let state_for_set = state.clone();
                button(
                    SharedString::from(format!("ai-set-default-{}", id.key())),
                    ButtonKind::Secondary,
                    None,
                    i18n::t(&lang, "settings.ai.set_default"),
                    true,
                    p,
                )
                .on_click(move |_, _, cx| {
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
            icon_button_colored(
                SharedString::from(format!("ai-del-{}", cid)),
                icons::TRASH,
                p.danger,
                p.danger,
                i18n::t(&lang, "settings.ai.delete"),
                p,
            )
            .on_click(move |_, _, cx| {
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

    let header = div()
        .min_h(px(52.))
        .px(px(14.))
        .py(px(10.))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(div().flex_none().size(px(20.)).child(icon))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child(title),
                )
                .child(render_status(&r, &status, &lang, p)),
        )
        .child(head_action);

    let mut card = render_card(cx).child(header);

    // 自定义：名称 + 协议格式
    if matches!(r, ProviderRef::Custom(_)) {
        if let Some(input) = name_input.as_ref() {
            card = card.child(render_row(
                i18n::t(&lang, "settings.ai.custom_name"),
                None,
                div().w(WIDE_FIELD_WIDTH).child(text_field(input)),
                cx,
            ));
        }
        let current_format = read
            .ai_inputs
            .get(&r)
            .and_then(|i| i.format)
            .unwrap_or_default();
        card = card.child(render_format_row(
            r.clone(),
            current_format,
            state.clone(),
            &lang,
            cx,
        ));
    }

    card.children(api_key_input.as_ref().map(|input| {
        render_row(
            i18n::t(&lang, "settings.ai.api_key"),
            None,
            div()
                .w(WIDE_FIELD_WIDTH)
                .child(text_field(input).mask_toggle()),
            cx,
        )
    }))
    .children(base_url_input.as_ref().map(|input| {
        render_row(
            i18n::t(&lang, "settings.ai.base_url"),
            None,
            div().w(WIDE_FIELD_WIDTH).child(text_field(input)),
            cx,
        )
    }))
    .children(model_input.as_ref().map(|input| {
        render_block_row(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(p.text)
                        .child(i18n::t(&lang, "settings.ai.models")),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.text_muted)
                        .child(i18n::t(&lang, "settings.ai.models_hint")),
                )
                .child(div().mt(px(2.)).child(text_area(input))),
            cx,
        )
    }))
    .child(render_block_row(
        div().flex().justify_end().child({
            let test = button(
                SharedString::from(format!("ai-test-{:?}", r)),
                ButtonKind::Secondary,
                Some(icons::PLAY),
                if is_testing {
                    i18n::t(&lang, "settings.ai.testing")
                } else {
                    i18n::t(&lang, "settings.ai.test")
                },
                !is_testing,
                p,
            );
            if is_testing {
                test
            } else {
                let state = state.clone();
                let r = r.clone();
                test.on_click(move |_, _, cx| run_test(r.clone(), state.clone(), cx))
            }
        }),
        cx,
    ))
}

/// 协议格式下拉行
fn render_format_row(
    r: ProviderRef,
    current: ApiFormat,
    state: Entity<SettingsDialogState>,
    lang: &Language,
    cx: &App,
) -> impl IntoElement {
    render_row(
        i18n::t(lang, "settings.ai.format"),
        None,
        select_trigger(
            SharedString::from(format!("ai-format-{:?}", r)),
            current.label(),
            CONTROL_WIDTH,
            cx,
        )
        .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu, _, _| {
            let mut menu = menu.min_w(CONTROL_WIDTH);
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
        cx,
    )
}

/// 连通性测试状态标签；失败时悬停显示完整原因
fn render_status(
    r: &ProviderRef,
    status: &AiTestStatus,
    lang: &Language,
    p: &Palette,
) -> AnyElement {
    match status {
        AiTestStatus::Untested => status_tag(
            i18n::t(lang, "settings.ai.status_untested").into(),
            p.text_faint,
        )
        .into_any_element(),
        AiTestStatus::Testing => status_tag(
            i18n::t(lang, "settings.ai.status_testing").into(),
            p.text_muted,
        )
        .into_any_element(),
        AiTestStatus::Pass => {
            status_tag(i18n::t(lang, "settings.ai.status_pass").into(), p.success)
                .into_any_element()
        }
        AiTestStatus::Fail(msg) => {
            let full = format!("{}: {}", i18n::t(lang, "settings.ai.status_fail"), msg);
            div()
                .id(SharedString::from(format!("ai-status-{:?}", r)))
                .min_w_0()
                .overflow_hidden()
                .tooltip(text_tooltip(full))
                .child(status_tag(
                    format!(
                        "{}: {}",
                        i18n::t(lang, "settings.ai.status_fail"),
                        truncate(msg, 32)
                    )
                    .into(),
                    p.danger,
                ))
                .into_any_element()
        }
    }
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
