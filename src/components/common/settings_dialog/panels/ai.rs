// AI 配置面板：内置供应商 + 自定义供应商（可选协议格式、字母头像）

use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

use crate::components::common::button::{button, icon_button_colored, text_tooltip, ButtonKind};
use crate::components::common::icon::{icon_sized, render_letter_avatar, render_provider_icon};
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
    verified: bool,
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
        .when(verified, |tab| {
            tab.child(div().flex_none().size(px(6.)).rounded_full().bg(p.success))
        })
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

fn provider_group_label(text: &'static str, p: &Palette) -> Div {
    div()
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.text_muted)
        .px(px(2.))
        .child(text)
}

fn render_builtin_group_row(
    ids: &[AiProviderId],
    active: &ProviderRef,
    default_provider: AiProviderId,
    default_label: &'static str,
    state: &Entity<SettingsDialogState>,
    p: &Palette,
    cx: &App,
) -> Div {
    let read = state.read(cx);
    let mut row = div().flex().items_center().flex_wrap().gap(px(6.));
    for &id in ids {
        let r = ProviderRef::Builtin(id);
        let verified = matches!(read.ai_test_statuses.get(&r), Some(AiTestStatus::Pass));
        let state_for_click = state.clone();
        row = row.child(
            provider_tab(
                SharedString::from(format!("ai-tab-{}", id.key())),
                render_provider_icon(id, 16., p.text),
                id.label().into(),
                *active == r,
                verified,
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
    row
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

    // 1. Coding 订阅分组
    let subscription_section = div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(provider_group_label(
            i18n::t(&lang, "settings.ai.group.subscription"),
            p,
        ))
        .child(render_builtin_group_row(
            &AiProviderId::SUBSCRIPTIONS,
            &active,
            default_provider,
            default_label,
            &state,
            p,
            cx,
        ));

    // 2. 官方与聚合 API 分组
    let api_section = div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(provider_group_label(
            i18n::t(&lang, "settings.ai.group.api"),
            p,
        ))
        .child(render_builtin_group_row(
            &AiProviderId::APIS,
            &active,
            default_provider,
            default_label,
            &state,
            p,
            cx,
        ));

    // 3. 本地模型与自定义分组
    let mut local_and_custom_row = render_builtin_group_row(
        &AiProviderId::LOCALS,
        &active,
        default_provider,
        default_label,
        &state,
        p,
        cx,
    );

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
        let r = ProviderRef::Custom(cid.clone());
        let verified = matches!(read.ai_test_statuses.get(&r), Some(AiTestStatus::Pass));
        let state_for_click = state.clone();
        let cid_for_click = cid.clone();
        local_and_custom_row = local_and_custom_row.child(
            provider_tab(
                SharedString::from(format!("ai-tab-custom-{}", cid)),
                render_letter_avatar(ProviderRef::avatar_char(&name), 16.).into_any_element(),
                display,
                active == r,
                verified,
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

    let state_for_add = state.clone();
    local_and_custom_row = local_and_custom_row.child(
        provider_tab(
            "ai-tab-add".into(),
            icon_sized(icons::PLUS, 14., p.text_muted).into_any_element(),
            i18n::t(&lang, "settings.ai.custom_add").into(),
            false,
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
    );

    let local_custom_section = div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(provider_group_label(
            i18n::t(&lang, "settings.ai.group.local_custom"),
            p,
        ))
        .child(local_and_custom_row);

    div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(subscription_section)
        .child(api_section)
        .child(local_custom_section)
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
    let new_model_input = inputs.and_then(|i| i.new_model_input.clone());
    let selected_models = read.current_ai_models(&r);
    let candidate_models = read.candidate_ai_models(&r);
    let name_input = inputs.and_then(|i| i.name.clone());
    let status = read
        .ai_test_statuses
        .get(&r)
        .cloned()
        .unwrap_or(AiTestStatus::Untested);
    let is_testing = matches!(status, AiTestStatus::Testing);
    let is_fetching = read.ai_fetching_models.contains(&r);
    let fetch_notice = read.ai_fetch_notices.get(&r).cloned();

    let current_format = read
        .ai_inputs
        .get(&r)
        .and_then(|i| i.format)
        .unwrap_or_else(|| match &r {
            ProviderRef::Builtin(id) => read.settings.ai.get(*id).effective_format(*id),
            ProviderRef::Custom(_) => ApiFormat::default(),
        });
    let current_base_url = base_url_input
        .as_ref()
        .map(|i| i.read(cx).value().trim().trim_end_matches('/').to_string())
        .unwrap_or_default();
    let requires_key = SettingsDialogState::ai_requires_api_key(&r, &current_base_url);

    // 头部：图标 + 名称 + 测试状态；右侧是设为默认（内置）或删除（自定义）
    let (icon, title): (AnyElement, SharedString) = match &r {
        ProviderRef::Builtin(id) => (render_provider_icon(*id, 20., p.text), id.label().into()),
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

    // 自定义供应商：名称
    if matches!(r, ProviderRef::Custom(_)) {
        if let Some(input) = name_input.as_ref() {
            card = card.child(render_row(
                i18n::t(&lang, "settings.ai.custom_name"),
                None,
                div().w(WIDE_FIELD_WIDTH).child(text_field(input)),
                cx,
            ));
        }
    }

    // 协议格式下拉（自定义供应商，或支持多协议切换的内置供应商如 OpenAI / 智谱 / 百炼 / Kimi / MiniMax / 火山方舟）
    let show_format_row = match &r {
        ProviderRef::Custom(_) => true,
        ProviderRef::Builtin(id) => id.supports_format_override(),
    };
    if show_format_row {
        card = card.child(render_format_row(
            r.clone(),
            current_format,
            state.clone(),
            &lang,
            cx,
        ));
    }

    card = card.children(api_key_input.as_ref().map(|input| {
        render_row(
            i18n::t(&lang, "settings.ai.api_key"),
            None,
            div()
                .w(WIDE_FIELD_WIDTH)
                .child(text_field(input).mask_toggle()),
            cx,
        )
    }));
    if !requires_key {
        card = card.child(render_hint(i18n::t(&lang, "settings.ai.local_key_hint"), cx));
    }

    card = card.children(base_url_input.as_ref().map(|input| {
        render_row(
            i18n::t(&lang, "settings.ai.base_url"),
            None,
            div().w(WIDE_FIELD_WIDTH).child(text_field(input)),
            cx,
        )
    }));

    // 快捷端点预设（按量 API / 国际站 / Coding Plan 订阅等）
    if let ProviderRef::Builtin(id) = &r {
        let presets = id.endpoint_presets();
        if !presets.is_empty() {
            let mut chips = div().flex().items_center().flex_wrap().gap(px(6.));
            for (idx, preset) in presets.iter().enumerate() {
                let is_active =
                    current_base_url == preset.base_url && current_format == preset.format;
                let preset = *preset;
                let state_for_preset = state.clone();
                let r_for_preset = r.clone();
                let base_input_for_preset = base_url_input.clone();
                let hover_bg = p.hover;
                chips = chips.child(
                    div()
                        .id(SharedString::from(format!(
                            "ai-preset-{}-{}",
                            id.key(),
                            idx
                        )))
                        .px(px(8.))
                        .py(px(4.))
                        .rounded(px(6.))
                        .border_1()
                        .cursor_pointer()
                        .text_size(px(11.5))
                        .when(is_active, |el| {
                            el.bg(p.primary.opacity(0.14))
                                .border_color(p.primary.opacity(0.55))
                                .text_color(p.primary)
                                .font_weight(FontWeight::MEDIUM)
                        })
                        .when(!is_active, |el| {
                            el.bg(p.surface)
                                .border_color(p.border)
                                .text_color(p.text_muted)
                                .hover(move |s| s.bg(hover_bg))
                        })
                        .child(preset.label(&lang))
                        .on_click(move |_, window, cx| {
                            if let Some(input) = base_input_for_preset.as_ref() {
                                input.update(cx, |s, cx| {
                                    s.set_value(preset.base_url.to_string(), window, cx);
                                });
                            }
                            let r_clone = r_for_preset.clone();
                            state_for_preset.update(cx, |s, cx| {
                                s.ai_inputs.entry(r_clone).or_default().format =
                                    Some(preset.format);
                                s.mark_changed();
                                cx.notify();
                            });
                        }),
                );
            }
            card = card.child(render_block_row(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.text_muted)
                            .child(i18n::t(&lang, "settings.ai.endpoint_presets")),
                    )
                    .child(chips),
                cx,
            ));
        }
    }

    // 模型列表：已添加模型标签（点击设为默认 / × 移除）+ 下拉选择添加 + 手动输入添加
    card = card.child(render_models_section(
        &r,
        selected_models,
        candidate_models,
        new_model_input,
        is_fetching,
        fetch_notice,
        state.clone(),
        &lang,
        p,
        cx,
    ));

    card.child(render_block_row(
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

/// 模型管理区块：顶部拉取按钮 + 已选模型标签（点选默认 / 删除）+ 下拉选择添加 + 手动输入添加
#[allow(clippy::too_many_arguments)]
fn render_models_section(
    r: &ProviderRef,
    selected_models: Vec<String>,
    candidate_models: Vec<String>,
    new_model_input: Option<Entity<gpui_kit::component::input::InputState>>,
    is_fetching: bool,
    fetch_notice: Option<(bool, String)>,
    state: Entity<SettingsDialogState>,
    lang: &Language,
    p: &Palette,
    cx: &App,
) -> impl IntoElement {
    let default_tag = i18n::t(lang, "settings.ai.default");
    let set_default_tip = i18n::t(lang, "settings.ai.model_set_default_tip");
    let hover_bg = p.hover;

    // 1. 已选模型标签区
    let chips_el: AnyElement = if selected_models.is_empty() {
        div()
            .px(px(10.))
            .py(px(8.))
            .rounded(px(7.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .text_size(px(12.))
            .text_color(p.text_faint)
            .child(i18n::t(lang, "settings.ai.models_empty"))
            .into_any_element()
    } else {
        let mut chips = div().flex().items_center().flex_wrap().gap(px(6.));
        for (idx, model_name) in selected_models.iter().enumerate() {
            let is_default = idx == 0;
            let model_for_default = model_name.clone();
            let model_for_remove = model_name.clone();
            let state_for_default = state.clone();
            let state_for_remove = state.clone();
            let r_for_default = r.clone();
            let r_for_remove = r.clone();

            let mut chip = div()
                .flex_none()
                .h(px(28.))
                .pl(px(9.))
                .pr(px(5.))
                .rounded(px(7.))
                .border_1()
                .flex()
                .items_center()
                .gap(px(6.))
                .when(is_default, |el| {
                    el.bg(p.primary.opacity(0.1))
                        .border_color(p.primary.opacity(0.45))
                })
                .when(!is_default, |el| {
                    el.bg(p.surface).border_color(p.border)
                });

            let label_btn = div()
                .id(SharedString::from(format!(
                    "ai-model-chip-{:?}-{}",
                    r, idx
                )))
                .flex()
                .items_center()
                .gap(px(5.))
                .when(!is_default, |el| {
                    el.cursor_pointer()
                        .tooltip(text_tooltip(set_default_tip))
                        .on_click(move |_, _, cx| {
                            let r = r_for_default.clone();
                            let m = model_for_default.clone();
                            state_for_default.update(cx, |s, cx| {
                                s.set_default_ai_model(&r, &m);
                                cx.notify();
                            });
                        })
                })
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(if is_default { p.text } else { p.text_muted })
                        .when(is_default, |el| el.font_weight(FontWeight::MEDIUM))
                        .child(model_name.clone()),
                )
                .when(is_default, |el| {
                    el.child(status_tag(default_tag.into(), p.primary))
                });

            let remove_btn = div()
                .id(SharedString::from(format!(
                    "ai-model-del-{:?}-{}",
                    r, idx
                )))
                .size(px(18.))
                .rounded(px(4.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .child(icon_sized(icons::X, 11., p.text_muted))
                .on_click(move |_, _, cx| {
                    let r = r_for_remove.clone();
                    let m = model_for_remove.clone();
                    state_for_remove.update(cx, |s, cx| {
                        s.remove_ai_model(&r, &m);
                        cx.notify();
                    });
                });

            chip = chip.child(label_btn).child(remove_btn);
            chips = chips.child(chip);
        }
        chips.into_any_element()
    };

    // 2. 下拉选择添加 + 手动输入添加
    let picker_placeholder = if candidate_models.is_empty() {
        i18n::t(lang, "settings.ai.model_picker_empty")
    } else {
        i18n::t(lang, "settings.ai.model_picker_placeholder")
    };
    let candidate_count = candidate_models.len();
    let dropdown_el: AnyElement = if candidate_count > 0 {
        let r_for_menu = r.clone();
        let state_for_menu = state.clone();
        select_trigger(
            SharedString::from(format!("ai-model-picker-{:?}", r)),
            picker_placeholder,
            px(210.),
            cx,
        )
        .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu, _, _| {
            let mut menu = menu.min_w(px(240.)).max_h(px(260.)).scrollable(true);
            for model in &candidate_models {
                let model_clone = model.clone();
                let r_click = r_for_menu.clone();
                let state_click = state_for_menu.clone();
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(model.clone())).on_click(
                        move |_, _, cx| {
                            let r = r_click.clone();
                            let m = model_clone.clone();
                            state_click.update(cx, |s, cx| {
                                s.add_ai_model(&r, m);
                                cx.notify();
                            });
                        },
                    ),
                );
            }
            menu
        })
        .into_any_element()
    } else {
        div()
            .h(px(30.))
            .w(px(210.))
            .px(px(10.))
            .rounded(px(7.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .flex()
            .items_center()
            .text_size(px(12.))
            .text_color(p.text_faint)
            .child(picker_placeholder)
            .into_any_element()
    };

    let manual_add_el: Option<AnyElement> = new_model_input.map(|input| {
        let input_for_add = input.clone();
        let state_for_add = state.clone();
        let r_for_add = r.clone();
        div()
            .flex_1()
            .min_w(px(180.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(div().flex_1().min_w_0().child(text_field(&input)))
            .child(
                button(
                    SharedString::from(format!("ai-model-add-{:?}", r)),
                    ButtonKind::Secondary,
                    Some(icons::PLUS),
                    i18n::t(lang, "settings.ai.model_add_btn"),
                    true,
                    p,
                )
                .on_click(move |_, window, cx| {
                    let val = input_for_add.read(cx).value().trim().to_string();
                    if !val.is_empty() {
                        let r = r_for_add.clone();
                        state_for_add.update(cx, |s, cx| {
                            s.add_ai_model(&r, val);
                            cx.notify();
                        });
                        input_for_add.update(cx, |s, cx| {
                            s.set_value(String::new(), window, cx);
                        });
                    }
                }),
            )
            .into_any_element()
    });

    render_block_row(
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(p.text)
                            .child(i18n::t(lang, "settings.ai.models")),
                    )
                    .child({
                        let fetch_btn = button(
                            SharedString::from(format!("ai-fetch-models-{:?}", r)),
                            ButtonKind::Secondary,
                            Some(icons::REFRESH),
                            if is_fetching {
                                i18n::t(lang, "settings.ai.fetching_models")
                            } else {
                                i18n::t(lang, "settings.ai.fetch_models")
                            },
                            !is_fetching,
                            p,
                        );
                        if is_fetching {
                            fetch_btn
                        } else {
                            let state = state.clone();
                            let r = r.clone();
                            fetch_btn.on_click(move |_, _, cx| {
                                run_fetch_models(r.clone(), state.clone(), cx);
                            })
                        }
                    }),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.text_muted)
                    .child(i18n::t(lang, "settings.ai.models_hint")),
            )
            .children(fetch_notice.map(|(ok, msg)| {
                div()
                    .text_size(px(11.5))
                    .text_color(if ok { p.success } else { p.danger })
                    .child(msg)
            }))
            .child(chips_el)
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(dropdown_el)
                    .children(manual_add_el),
            ),
        cx,
    )
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

fn resolve_test_base_url(r: &ProviderRef, base_url: &str) -> String {
    if base_url.trim().is_empty() {
        match r {
            ProviderRef::Builtin(id) => id.default_base_url().to_string(),
            ProviderRef::Custom(_) => String::new(),
        }
    } else {
        base_url.trim().to_string()
    }
}

fn run_fetch_models(r: ProviderRef, state: Entity<SettingsDialogState>, cx: &mut App) {
    let values = {
        let app: &App = cx;
        state.read(app).collect_ai_inputs(&r, app)
    };
    let test_base = resolve_test_base_url(&r, &values.base_url);
    let requires_key = SettingsDialogState::ai_requires_api_key(&r, &test_base);
    if requires_key && values.api_key.trim().is_empty() {
        let r2 = r.clone();
        state.update(cx, |s, cx| {
            let msg = i18n::t(&s.settings.theme.language, "settings.ai.empty_key").to_string();
            s.ai_fetch_notices.insert(r2, (false, msg));
            cx.notify();
        });
        return;
    }

    let r_mark = r.clone();
    state.update(cx, |s, cx| {
        s.ai_fetching_models.insert(r_mark.clone());
        s.ai_fetch_notices.remove(&r_mark);
        cx.notify();
    });

    let (tx, rx) = tokio::sync::oneshot::channel();
    let format = values.format;
    let api_key = values.api_key.clone();
    let runtime = crate::ssh::manager::SshManager::global().runtime();
    runtime.spawn(async move {
        let result =
            crate::services::ai::fetch_models(format, &api_key, &test_base, requires_key).await;
        let _ = tx.send(result);
    });

    cx.spawn({
        let state = state.clone();
        async move |async_cx| {
            let result = rx.await;
            let _ = async_cx.update(|cx| {
                state.update(cx, |s, cx| {
                    s.ai_fetching_models.remove(&r);
                    let lang = s.settings.theme.language.clone();
                    match result {
                        Ok(Ok(models)) if !models.is_empty() => {
                            let count = models.len();
                            s.ai_remote_models.insert(r.clone(), models.clone());
                            // 若当前尚未配置模型（或仅有占位模型 local-model），自动添入首个发现的模型
                            let current = s.current_ai_models(&r);
                            if current.is_empty()
                                || (current.len() == 1 && current[0] == "local-model")
                            {
                                s.ai_inputs.entry(r.clone()).or_default().models =
                                    Some(vec![models[0].clone()]);
                                s.mark_changed();
                            }
                            s.ai_fetch_notices.insert(
                                r.clone(),
                                (
                                    true,
                                    format!(
                                        "{} ({})",
                                        i18n::t(&lang, "settings.ai.fetch_models_ok"),
                                        count
                                    ),
                                ),
                            );
                        }
                        Ok(Ok(_)) => {
                            s.ai_fetch_notices.insert(
                                r.clone(),
                                (
                                    false,
                                    i18n::t(&lang, "settings.ai.fetch_models_empty").to_string(),
                                ),
                            );
                        }
                        Ok(Err(e)) => {
                            s.ai_fetch_notices.insert(r.clone(), (false, e.to_string()));
                        }
                        Err(_) => {
                            s.ai_fetch_notices
                                .insert(r.clone(), (false, "internal channel error".into()));
                        }
                    }
                    cx.notify();
                });
            });
            Some(())
        }
    })
    .detach();
}

fn run_test(r: ProviderRef, state: Entity<SettingsDialogState>, cx: &mut App) {
    // 1. 从当前输入收集测试配置
    let values = {
        let app: &App = cx;
        state.read(app).collect_ai_inputs(&r, app)
    };
    let test_base = resolve_test_base_url(&r, &values.base_url);
    let requires_key = SettingsDialogState::ai_requires_api_key(&r, &test_base);
    if requires_key && values.api_key.trim().is_empty() {
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

    let probe_model = if !values.model.trim().is_empty() {
        values.model.clone()
    } else if let ProviderRef::Builtin(id) = &r {
        id.default_model().to_string()
    } else {
        String::new()
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
        let result = crate::services::ai::test_connection(
            format,
            &api_key,
            &test_base,
            &probe_model,
            requires_key,
        )
        .await;
        let _ = tx.send(result);
    });

    cx.spawn({
        let state = state.clone();
        async move |async_cx| {
            let result = rx.await;
            let _ = async_cx.update(|cx| {
                state.update(cx, |s, cx| {
                    let new_status = match result {
                        Ok(Ok(discovered_models)) => {
                            let mut snapshot_model = values.model.clone();
                            if !discovered_models.is_empty() {
                                s.ai_remote_models
                                    .insert(r.clone(), discovered_models.clone());
                                let should_autofill = values.models.is_empty()
                                    || (values.models.len() == 1
                                        && values.models[0] == "local-model");
                                if should_autofill {
                                    snapshot_model = discovered_models[0].clone();
                                    s.ai_inputs.entry(r.clone()).or_default().models =
                                        Some(vec![discovered_models[0].clone()]);
                                }
                            }
                            s.ai_tested_snapshots.insert(
                                r.clone(),
                                (
                                    values.api_key.clone(),
                                    values.base_url.clone(),
                                    snapshot_model,
                                    values.format.label().to_string(),
                                ),
                            );
                            s.ai_save_error = None;
                            s.mark_changed();
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
