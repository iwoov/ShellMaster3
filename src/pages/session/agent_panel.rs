// AI Agent 侧边栏面板：支持多步工具调用卡片、危险命令内联审批、流式思考/正文与中断控制

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::text::{TextView, TextViewStyle};
use gpui_kit::component::{ActiveTheme, Disableable, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::components::common::button::icon_button_sized;
use crate::components::common::icon::{icon_sized, render_letter_avatar};
use crate::components::common::panel::{panel_header, PANEL_PADDING};
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::{AgentApprovalMode, AgentReasoningEffort, ProviderRef};
use crate::services::agent::{ApprovalDecision, ToolRisk};
use crate::services::ai::ChatRole;
use crate::state::{AgentMessageBlock, AiChatMessage, SessionState, SessionTab, ToolCallStatus};

/// 渲染右侧栏 AI Agent 面板
pub fn render_agent_panel(
    tab: &SessionTab,
    width: Pixels,
    session_state: Entity<SessionState>,
    lang: &Language,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let tab_id = tab.id.clone();
    let chat = &tab.ai_chat;
    let foreground = cx.theme().foreground;
    let muted_foreground = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let p = crate::theme::palette(cx);

    let settings = crate::services::storage::cached_settings();
    let verified_refs = settings.ai.verified_provider_refs();
    let has_any = !verified_refs.is_empty();
    let active_ref = session_state.read(cx).resolve_ai_provider_ref(&tab_id);
    let active_resolved = active_ref.as_ref().and_then(|r| settings.ai.resolve(r));
    let has_messages = !chat.messages.is_empty() || chat.pending;
    let input_entity = session_state.read(cx).get_ai_chat_input(&tab_id);

    // ===== 顶部 36px 标题条 =====
    let header = {
        let session_for_clear = session_state.clone();
        let tab_id_for_clear = tab_id.clone();
        let new_thread_tooltip = i18n::t(lang, "ai_chat.new_thread");
        let header_title: SharedString = if has_messages {
            SharedString::from(tab.server_label.clone())
        } else {
            i18n::t(lang, "ai_chat.new_thread").into()
        };
        let provider_icon: AnyElement = match (&active_ref, &active_resolved) {
            (Some(ProviderRef::Builtin(id)), _) => img(id.icon_path())
                .w(px(15.))
                .h(px(15.))
                .flex_shrink_0()
                .into_any_element(),
            (Some(ProviderRef::Custom(_)), Some(provider)) => {
                render_letter_avatar(ProviderRef::avatar_char(&provider.name), 15.)
                    .into_any_element()
            }
            _ => icon_sized(icons::SPARKLES, 14., p.text_muted).into_any_element(),
        };

        panel_header(&p)
            .bg(p.sidebar)
            .pl(PANEL_PADDING)
            .pr(px(6.))
            .gap(px(8.))
            .child(provider_icon)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text)
                    .child(header_title),
            )
            .child(
                icon_button_sized(
                    "ai-chat-new-thread",
                    icons::PLUS,
                    px(28.),
                    14.,
                    new_thread_tooltip,
                    &p,
                )
                .on_click(move |_, _, cx| {
                    session_for_clear.update(cx, |s, cx| {
                        s.clear_ai_chat(&tab_id_for_clear, cx);
                    });
                }),
            )
    };

    // ===== 中部消息区 =====
    let show_thinking = chat.pending
        && chat
            .messages
            .last()
            .map(|m| {
                if m.role != ChatRole::Assistant {
                    return true;
                }
                if !m.has_visible_content() {
                    return true;
                }
                // 上一个工具刚执行完、正在等模型下一步推理时，也展示轻量思考提示
                matches!(
                    m.blocks.last(),
                    Some(AgentMessageBlock::ToolCall {
                        status: ToolCallStatus::Succeeded
                            | ToolCallStatus::Failed
                            | ToolCallStatus::Denied,
                        ..
                    })
                )
            })
            .unwrap_or(true);

    let scroll_handle = chat.scroll_handle.clone();
    let show_scroll_to_bottom = chat.show_scroll_to_bottom_button();
    let session_for_wheel = session_state.clone();
    let tab_id_for_wheel = tab_id.clone();
    let session_for_jump = session_state.clone();
    let tab_id_for_jump = tab_id.clone();

    let messages_view = has_messages.then(|| {
        div()
            .flex_1()
            .min_h_0()
            .relative()
            .child(
                div()
                    .id(SharedString::from(format!("ai-chat-messages-{tab_id}")))
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&scroll_handle)
                    .on_scroll(move |event, window, cx| {
                        let delta_y = event.delta.pixel_delta(window.line_height()).y;
                        session_for_wheel.update(cx, |s, _| {
                            s.on_ai_chat_scroll_wheel(&tab_id_for_wheel, delta_y);
                        });
                    })
                    .child(
                        div()
                            .w_full()
                            .py_2()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .children(
                                chat.messages
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, msg)| msg.has_visible_content())
                                    .map(|(idx, msg)| {
                                        let is_streaming = chat.pending
                                            && idx + 1 == chat.messages.len()
                                            && msg.role == ChatRole::Assistant;
                                        render_agent_message(
                                            idx,
                                            msg,
                                            is_streaming,
                                            session_state.clone(),
                                            tab_id.clone(),
                                            lang,
                                            window,
                                            cx,
                                        )
                                    }),
                            )
                            .when(show_thinking, |this| {
                                this.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(7.))
                                        .px_5()
                                        .py_1()
                                        .child(
                                            svg()
                                                .path(icons::SPARKLES)
                                                .size(px(13.))
                                                .text_color(muted_foreground),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(muted_foreground)
                                                .child(i18n::t(lang, "ai_chat.thinking")),
                                        ),
                                )
                            }),
                    ),
            )
            .vertical_scrollbar(&scroll_handle)
            .when(show_scroll_to_bottom, |this| {
                this.child(
                    div()
                        .absolute()
                        .right(px(14.))
                        .bottom(px(10.))
                        .child(
                            Button::new("ai-chat-scroll-to-bottom")
                                .outline()
                                .xsmall()
                                .compact()
                                .rounded_full()
                                .bg(cx.theme().background)
                                .shadow_sm()
                                .tooltip(i18n::t(lang, "ai_chat.scroll_to_bottom"))
                                .child(
                                    svg()
                                        .path(icons::CHEVRON_DOWN)
                                        .size(px(14.))
                                        .text_color(foreground),
                                )
                                .on_click(move |_, _, cx| {
                                    session_for_jump.update(cx, |s, cx| {
                                        s.scroll_ai_chat_to_bottom(&tab_id_for_jump, cx);
                                    });
                                }),
                        ),
                )
            })
            .into_any_element()
    });

    // ===== 底部编辑器 + 工具栏 =====
    let input_area = {
        let session_for_send = session_state.clone();
        let tab_id_for_send = tab_id.clone();
        let session_for_stop = session_state.clone();
        let tab_id_for_stop = tab_id.clone();
        let pending = chat.pending;
        let has_input = input_entity.is_some();

        let model_list: Vec<String> = active_resolved
            .as_ref()
            .map(|r| r.models.clone())
            .unwrap_or_default();
        let current_model: SharedString = {
            let sel = chat
                .selected_model
                .as_ref()
                .filter(|m| model_list.iter().any(|x| x == *m));
            match sel {
                Some(m) => SharedString::from(m.clone()),
                None => model_list
                    .first()
                    .cloned()
                    .or_else(|| active_resolved.as_ref().map(|r| r.model.clone()))
                    .filter(|m| !m.is_empty())
                    .map(SharedString::from)
                    .unwrap_or_else(|| SharedString::from("—")),
            }
        };

        // 审批模式快捷切换下拉（与设置页同步）
        let approval_dropdown = {
            let current_mode = settings.ai.agent.approval_mode;
            let session_for_mode = session_state.clone();
            let lang_clone = lang.clone();
            let mode_short_label = approval_mode_short_label(&lang_clone, current_mode);
            let mode_tooltip = i18n::t(lang, current_mode.desc_key());

            Button::new("ai-agent-approval-mode")
                .ghost()
                .xsmall()
                .compact()
                .tooltip(mode_tooltip)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .min_w_0()
                        .gap(px(3.))
                        .child(
                            svg()
                                .path(icons::LOCK)
                                .size(px(12.))
                                .flex_shrink_0()
                                .text_color(muted_foreground),
                        )
                        .child(
                            div()
                                .max_w(px(60.))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_xs()
                                .text_color(muted_foreground)
                                .child(mode_short_label),
                        )
                        .child(
                            svg()
                                .path(icons::CHEVRON_DOWN)
                                .size(px(9.))
                                .flex_shrink_0()
                                .text_color(muted_foreground),
                        ),
                )
                .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                    let mut menu = menu.min_w(px(210.));
                    for mode in AgentApprovalMode::ALL {
                        let label = i18n::t(&lang_clone, mode.label_key());
                        let is_selected = mode == current_mode;
                        let session = session_for_mode.clone();
                        menu = menu.item(
                            PopupMenuItem::element(move |_window, cx| {
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().foreground)
                                            .child(label),
                                    )
                                    .when(is_selected, |this| {
                                        this.child(
                                            svg()
                                                .path(icons::CHECK)
                                                .size(px(12.))
                                                .text_color(cx.theme().primary),
                                        )
                                    })
                            })
                            .on_click(move |_, _, cx| {
                                session.update(cx, |s, cx| {
                                    s.set_ai_approval_mode(mode, cx);
                                });
                            }),
                        );
                    }
                    menu
                })
        };

        // 思考深度快捷切换下拉（与设置页同步）
        let reasoning_dropdown = {
            let current_effort = settings.ai.agent.reasoning_effort;
            let session_for_effort = session_state.clone();
            let lang_clone = lang.clone();
            let effort_short_label = i18n::t(lang, current_effort.short_key());
            let effort_tooltip = SharedString::from(format!(
                "{}: {}",
                i18n::t(lang, "settings.ai.agent.reasoning_effort"),
                i18n::t(lang, current_effort.desc_key())
            ));
            let icon_color = match current_effort {
                AgentReasoningEffort::Low
                | AgentReasoningEffort::Medium
                | AgentReasoningEffort::High => p.primary,
                AgentReasoningEffort::Auto | AgentReasoningEffort::Off => muted_foreground,
            };

            Button::new("ai-agent-reasoning-effort")
                .ghost()
                .xsmall()
                .compact()
                .tooltip(effort_tooltip)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .min_w_0()
                        .gap(px(2.))
                        .child(
                            svg()
                                .path(icons::SPARKLES)
                                .size(px(12.))
                                .flex_shrink_0()
                                .text_color(icon_color),
                        )
                        .child(
                            div()
                                .max_w(px(34.))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_xs()
                                .text_color(muted_foreground)
                                .child(effort_short_label),
                        )
                        .child(
                            svg()
                                .path(icons::CHEVRON_DOWN)
                                .size(px(9.))
                                .flex_shrink_0()
                                .text_color(muted_foreground),
                        ),
                )
                .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                    let mut menu = menu.min_w(px(196.));
                    for effort in AgentReasoningEffort::ALL {
                        let label = i18n::t(&lang_clone, effort.label_key());
                        let is_selected = effort == current_effort;
                        let session = session_for_effort.clone();
                        menu = menu.item(
                            PopupMenuItem::element(move |_window, cx| {
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().foreground)
                                            .child(label),
                                    )
                                    .when(is_selected, |this| {
                                        this.child(
                                            svg()
                                                .path(icons::CHECK)
                                                .size(px(12.))
                                                .text_color(cx.theme().primary),
                                        )
                                    })
                            })
                            .on_click(move |_, _, cx| {
                                session.update(cx, |s, cx| {
                                    s.set_ai_reasoning_effort(effort, cx);
                                });
                            }),
                        );
                    }
                    menu
                })
        };

        // 供应商选择器
        let provider_dropdown = {
            let tab_id_for_pick = tab_id.clone();
            let session_for_pick = session_state.clone();
            let menu_items: Vec<(ProviderRef, String, Option<&'static str>)> = verified_refs
                .iter()
                .map(|provider_ref| match provider_ref {
                    ProviderRef::Builtin(id) => (
                        provider_ref.clone(),
                        id.label().to_string(),
                        Some(id.icon_path()),
                    ),
                    ProviderRef::Custom(_) => (
                        provider_ref.clone(),
                        settings
                            .ai
                            .resolve(provider_ref)
                            .map(|provider| provider.name)
                            .unwrap_or_default(),
                        None,
                    ),
                })
                .collect();
            let trigger_label: SharedString = active_resolved
                .as_ref()
                .map(|provider| SharedString::from(provider.name.clone()))
                .unwrap_or_else(|| i18n::t(lang, "ai_chat.no_provider_short").into());
            let trigger_icon: Option<AnyElement> = match (&active_ref, &active_resolved) {
                (Some(ProviderRef::Builtin(id)), _) => Some(
                    img(id.icon_path())
                        .w(px(14.))
                        .h(px(14.))
                        .flex_shrink_0()
                        .into_any_element(),
                ),
                (Some(ProviderRef::Custom(_)), Some(provider)) => Some(
                    render_letter_avatar(ProviderRef::avatar_char(&provider.name), 14.)
                        .into_any_element(),
                ),
                _ => None,
            };

            Button::new("ai-chat-provider-btn")
                .ghost()
                .xsmall()
                .compact()
                .disabled(!has_any)
                .tooltip(trigger_label)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .min_w_0()
                        .gap(px(2.))
                        .children(trigger_icon)
                        .child(
                            svg()
                                .path(icons::CHEVRON_DOWN)
                                .size(px(9.))
                                .flex_shrink_0()
                                .text_color(muted_foreground),
                        ),
                )
                .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                    let mut menu = menu.min_w(px(180.));
                    for (provider_ref, name, builtin_icon) in &menu_items {
                        let provider_ref = provider_ref.clone();
                        let label: SharedString = SharedString::from(name.clone());
                        let builtin_icon = *builtin_icon;
                        let avatar_char = ProviderRef::avatar_char(name);
                        let session = session_for_pick.clone();
                        let tab_id = tab_id_for_pick.clone();
                        menu = menu.item(
                            PopupMenuItem::element(move |_window, cx| {
                                let icon: AnyElement = match builtin_icon {
                                    Some(path) => {
                                        img(path).w(px(16.)).h(px(16.)).into_any_element()
                                    }
                                    None => {
                                        render_letter_avatar(avatar_char, 16.).into_any_element()
                                    }
                                };
                                div().flex().items_center().gap(px(8.)).child(icon).child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().foreground)
                                        .child(label.clone()),
                                )
                            })
                            .on_click(move |_, _, cx| {
                                session.update(cx, |state, cx| {
                                    state.set_ai_chat_provider(&tab_id, provider_ref.clone());
                                    cx.notify();
                                });
                            }),
                        );
                    }
                    menu
                })
        };

        // 模型下拉触发器：按当前侧栏宽度动态计算模型名最大宽度，确保单行布局永不挤出发送按钮
        let model_label_max_w = (width - px(252.)).max(px(44.));
        let model_dropdown = {
            let models = model_list.clone();
            let session_for_model = session_state.clone();
            let tab_id_for_model = tab_id.clone();
            let label_color = if has_any {
                foreground
            } else {
                muted_foreground
            };
            Button::new("ai-chat-model-btn")
                .ghost()
                .xsmall()
                .compact()
                .disabled(!has_any || model_list.is_empty())
                .tooltip(current_model.clone())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .min_w_0()
                        .child(
                            div()
                                .max_w(model_label_max_w)
                                .text_xs()
                                .text_color(label_color)
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(current_model.clone()),
                        )
                        .child(
                            svg()
                                .path(icons::CHEVRON_DOWN)
                                .size(px(9.))
                                .flex_shrink_0()
                                .text_color(muted_foreground),
                        ),
                )
                .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                    let mut menu = menu.min_w(px(180.));
                    for m in &models {
                        let model_owned = m.clone();
                        let label: SharedString = SharedString::from(m.clone());
                        let session = session_for_model.clone();
                        let tab_id_owned = tab_id_for_model.clone();
                        menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                            session.update(cx, |s, cx| {
                                s.set_ai_chat_model(&tab_id_owned, model_owned.clone());
                                cx.notify();
                            });
                        }));
                    }
                    menu
                })
        };

        // 主动作按钮：运行中显示「停止」，空闲时显示「发送」
        let action_btn: AnyElement = if pending {
            Button::new("ai-agent-stop")
                .danger()
                .xsmall()
                .compact()
                .tooltip(i18n::t(lang, "ai_agent.stop"))
                .child(
                    svg()
                        .path(icons::X)
                        .size(px(14.))
                        .flex_shrink_0()
                        .text_color(cx.theme().danger_foreground),
                )
                .on_click(move |_, _, cx| {
                    session_for_stop.update(cx, |s, cx| {
                        s.stop_ai_agent(&tab_id_for_stop, cx);
                    });
                })
                .into_any_element()
        } else {
            let send_disabled = !has_any || !has_input;
            let send_icon_color = if send_disabled {
                muted_foreground
            } else {
                cx.theme().primary_foreground
            };
            Button::new("ai-chat-send")
                .primary()
                .xsmall()
                .compact()
                .disabled(send_disabled)
                .child(
                    svg()
                        .path(icons::SEND)
                        .size(px(16.))
                        .flex_shrink_0()
                        .text_color(send_icon_color),
                )
                .on_click(move |_, window, cx| {
                    session_for_send.update(cx, |s, cx| {
                        s.send_ai_chat_message(&tab_id_for_send, window, cx);
                    });
                })
                .into_any_element()
        };

        let input_control = input_entity.as_ref().map(|input| {
            let input = Textarea::new(input).appearance(false);
            if has_messages {
                input
            } else {
                input.h_full()
            }
        });

        div()
            .bg(cx.theme().background)
            .when(has_messages, |this| {
                this.flex_shrink_0().border_t_1().border_color(border)
            })
            .when(!has_messages, |this| this.flex_1().min_h_0())
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .min_h(px(54.))
                            .px_3()
                            .pt_3()
                            .pb_2()
                            .when(!has_messages, |this| this.flex_1().min_h_0())
                            .children(input_control),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(4.))
                            .px_2()
                            .pb_2()
                            // 左侧：审批模式 + 思考深度
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(2.))
                                    .child(approval_dropdown)
                                    .child(reasoning_dropdown),
                            )
                            // 右侧：供应商 + 模型（按侧栏宽度动态截断）+ 固定发送/停止按钮
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .gap(px(2.))
                                    .child(div().flex_shrink_0().child(provider_dropdown))
                                    .child(model_dropdown)
                                    .child(div().flex_shrink_0().child(action_btn)),
                            ),
                    ),
            )
    };

    div()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(header)
        .children(messages_view)
        .child(input_area)
}

#[allow(clippy::too_many_arguments)]
fn render_agent_message(
    msg_idx: usize,
    msg: &AiChatMessage,
    is_streaming: bool,
    session_state: Entity<SessionState>,
    tab_id: String,
    lang: &Language,
    _window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let is_user = msg.role == ChatRole::User;
    let border = cx.theme().border;

    let inner: Vec<AnyElement> = if is_user || msg.error || msg.blocks.is_empty() {
        let text_color = if msg.error {
            cx.theme().danger
        } else {
            cx.theme().foreground
        };
        vec![div()
            .text_sm()
            .line_height(relative(1.45))
            .text_color(text_color)
            .whitespace_normal()
            .child(SharedString::from(msg.content.clone()))
            .into_any_element()]
    } else {
        let block_count = msg.blocks.len();
        msg.blocks
            .iter()
            .enumerate()
            .filter_map(|(block_idx, block)| {
                let is_last_block = block_idx + 1 == block_count;
                match block {
                    AgentMessageBlock::Reasoning { text, collapsed } => {
                        let trimmed = text.trim();
                        if trimmed.is_empty() {
                            return None;
                        }
                        Some(render_reasoning_block(
                            msg_idx,
                            block_idx,
                            trimmed.to_string(),
                            *collapsed,
                            is_streaming && is_last_block,
                            session_state.clone(),
                            tab_id.clone(),
                            lang,
                            cx,
                        ))
                    }
                    AgentMessageBlock::Text(text) => {
                        if text.is_empty() {
                            return None;
                        }
                        Some(render_text_block(
                            msg_idx,
                            block_idx,
                            text,
                            is_streaming && is_last_block,
                            cx,
                        ))
                    }
                    AgentMessageBlock::ToolCall {
                        call_id,
                        tool_name,
                        summary,
                        risk,
                        status,
                        output,
                        collapsed,
                    } => Some(render_tool_block(
                        msg_idx,
                        block_idx,
                        call_id,
                        tool_name,
                        summary,
                        *risk,
                        *status,
                        output,
                        *collapsed,
                        session_state.clone(),
                        tab_id.clone(),
                        lang,
                        cx,
                    )),
                }
            })
            .collect()
    };

    let message = div()
        .id(SharedString::from(format!("ai-msg-{msg_idx}")))
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .overflow_hidden()
        .when(is_user, |this| {
            this.px_2()
                .py_3()
                .rounded(px(8.))
                .bg(cx.theme().background)
                .border_1()
                .border_color(border)
                .shadow_sm()
        })
        .when(msg.error, |this| {
            this.w_full()
                .px_3()
                .py_2()
                .rounded(px(6.))
                .bg(cx.theme().danger.opacity(0.1))
                .border_1()
                .border_color(cx.theme().danger.opacity(0.3))
        })
        .when(!is_user && !msg.error, |this| this.w_full().px_3().py_2())
        .children(inner);

    div()
        .w_full()
        .overflow_hidden()
        .px_2()
        .child(message)
}

/// 将多行思考过程压平为单行预览文本：
/// - `tail = true`（思考流式进行中）：取末尾最新片段，用于单行横向滚动展示；
/// - `tail = false`（已折叠静态摘要）：取开头片段，配合省略号展示。
fn reasoning_single_line_preview(text: &str, tail: bool, max_chars: usize) -> String {
    let mut normalized = String::with_capacity(text.len().min(512));
    let mut last_was_space = true;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                normalized.push(' ');
                last_was_space = true;
            }
        } else {
            normalized.push(ch);
            last_was_space = false;
        }
    }
    let normalized = normalized.trim_end();
    let total_chars = normalized.chars().count();
    if total_chars <= max_chars {
        return normalized.to_string();
    }
    if tail {
        normalized
            .chars()
            .skip(total_chars - max_chars)
            .collect()
    } else {
        normalized.chars().take(max_chars).collect()
    }
}

#[allow(clippy::too_many_arguments)]
fn render_reasoning_block(
    msg_idx: usize,
    block_idx: usize,
    text: String,
    collapsed: bool,
    is_streaming: bool,
    session_state: Entity<SessionState>,
    tab_id: String,
    lang: &Language,
    cx: &App,
) -> AnyElement {
    let p = crate::theme::palette(cx);
    let muted_fg = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let icon_color = if is_streaming { p.primary } else { muted_fg };
    let title = if is_streaming {
        i18n::t(lang, "ai_chat.thinking")
    } else {
        i18n::t(lang, "ai_chat.reasoning")
    };

    // 默认折叠态采用单行滚动显示：流式时靠右跟随最新思考内容，完成后以单行省略展示
    let preview_el: Option<AnyElement> = if collapsed {
        let preview = reasoning_single_line_preview(&text, is_streaming, 240);
        if preview.is_empty() {
            None
        } else if is_streaming {
            Some(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .justify_end()
                    .child(
                        div()
                            .min_w_full()
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .text_xs()
                            .text_color(p.text_faint)
                            .child(SharedString::from(preview)),
                    )
                    .into_any_element(),
            )
        } else {
            Some(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(p.text_faint)
                    .child(SharedString::from(preview))
                    .into_any_element(),
            )
        }
    } else {
        None
    };

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .id(SharedString::from(format!(
                    "ai-reason-toggle-{msg_idx}-{block_idx}"
                )))
                .w_full()
                .px(px(6.))
                .py(px(4.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .hover(move |s| s.bg(p.hover))
                .child(
                    svg()
                        .path(icons::SPARKLES)
                        .size(px(12.))
                        .flex_shrink_0()
                        .text_color(icon_color),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if is_streaming { p.text_muted } else { muted_fg })
                        .child(title),
                )
                .when_else(
                    preview_el.is_some(),
                    |this| this.children(preview_el),
                    |this| this.child(div().flex_1()),
                )
                .child(
                    svg()
                        .path(if collapsed {
                            icons::CHEVRON_RIGHT
                        } else {
                            icons::CHEVRON_DOWN
                        })
                        .size(px(12.))
                        .flex_shrink_0()
                        .text_color(muted_fg),
                )
                .on_click(move |_, _, cx| {
                    session_state.update(cx, |s, cx| {
                        s.toggle_ai_block_collapse(&tab_id, msg_idx, block_idx, cx);
                    });
                }),
        )
        .when(!collapsed, |this| {
            this.child(
                div()
                    .id(SharedString::from(format!(
                        "ai-reason-body-{msg_idx}-{block_idx}"
                    )))
                    .ml(px(11.))
                    .pl_3()
                    .pr_2()
                    .py_1()
                    .max_h(px(220.))
                    .overflow_y_scrollbar()
                    .border_l_1()
                    .border_color(border)
                    .text_xs()
                    .line_height(relative(1.45))
                    .text_color(muted_fg)
                    .whitespace_normal()
                    .child(SharedString::from(text)),
            )
        })
        .into_any_element()
}

fn render_text_block(
    msg_idx: usize,
    block_idx: usize,
    text: &str,
    is_streaming: bool,
    cx: &App,
) -> AnyElement {
    if is_streaming {
        return div()
            .text_sm()
            .line_height(relative(1.45))
            .text_color(cx.theme().foreground)
            .whitespace_normal()
            .child(SharedString::from(text.to_string()))
            .into_any_element();
    }

    let code_bg = if cx.theme().mode.is_dark() {
        gpui_kit::black().opacity(0.36)
    } else {
        cx.theme().secondary
    };
    let code_style = StyleRefinement::default()
        .bg(code_bg)
        .rounded(px(5.))
        .p_2();

    div()
        .text_sm()
        .line_height(relative(1.45))
        .child(
            TextView::markdown(
                SharedString::from(format!("ai-md-{msg_idx}-{block_idx}")),
                text.to_string(),
            )
            .style(
                TextViewStyle::default()
                    .paragraph_gap(rems(0.65))
                    .code_block(code_style),
            )
            .selectable(true)
            .code_block_actions(move |cb, _window, cx| {
                let code = cb.code();
                let copy_color = cx.theme().muted_foreground;
                div()
                    .id(SharedString::from(format!(
                        "md-copy-{msg_idx}-{block_idx}-{}",
                        code.len()
                    )))
                    .cursor_pointer()
                    .px_1()
                    .py(px(2.))
                    .rounded(px(3.))
                    .hover(move |s| s.bg(copy_color.opacity(0.15)))
                    .child(
                        svg()
                            .path(icons::COPY)
                            .size(px(12.))
                            .text_color(copy_color),
                    )
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(code.to_string()));
                    })
                    .into_any_element()
            }),
        )
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_tool_block(
    msg_idx: usize,
    block_idx: usize,
    call_id: &str,
    tool_name: &str,
    summary: &str,
    risk: ToolRisk,
    status: ToolCallStatus,
    output: &str,
    collapsed: bool,
    session_state: Entity<SessionState>,
    tab_id: String,
    lang: &Language,
    cx: &App,
) -> AnyElement {
    let p = crate::theme::palette(cx);
    let code_bg = if cx.theme().mode.is_dark() {
        gpui_kit::black().opacity(0.32)
    } else {
        cx.theme().secondary
    };
    let tool_label = tool_display_name(lang, tool_name);

    // ===== 等待审批状态：渲染高亮内联审批卡片 =====
    if status == ToolCallStatus::WaitingApproval {
        let is_dangerous = risk == ToolRisk::Dangerous;
        let accent = if is_dangerous { p.danger } else { p.primary };
        let card_bg = accent.opacity(0.08);
        let card_border = accent.opacity(0.42);

        let call_id_allow = call_id.to_string();
        let tab_id_allow = tab_id.clone();
        let session_allow = session_state.clone();

        let call_id_session = call_id.to_string();
        let tab_id_session = tab_id.clone();
        let session_always = session_state.clone();

        let call_id_deny = call_id.to_string();
        let tab_id_deny = tab_id.clone();
        let session_deny = session_state.clone();

        let allow_btn = if is_dangerous {
            Button::new(SharedString::from(format!(
                "ai-approve-once-{msg_idx}-{block_idx}"
            )))
            .danger()
            .xsmall()
            .label(i18n::t(lang, "ai_agent.approval.allow"))
        } else {
            Button::new(SharedString::from(format!(
                "ai-approve-once-{msg_idx}-{block_idx}"
            )))
            .primary()
            .xsmall()
            .label(i18n::t(lang, "ai_agent.approval.allow"))
        }
        .on_click(move |_, _, cx| {
            session_allow.update(cx, |s, cx| {
                s.respond_ai_tool_approval(
                    &tab_id_allow,
                    &call_id_allow,
                    ApprovalDecision::AllowOnce,
                    cx,
                );
            });
        });

        let deny_btn = Button::new(SharedString::from(format!(
            "ai-approve-deny-{msg_idx}-{block_idx}"
        )))
        .ghost()
        .xsmall()
        .label(i18n::t(lang, "ai_agent.approval.deny"))
        .on_click(move |_, _, cx| {
            session_deny.update(cx, |s, cx| {
                s.respond_ai_tool_approval(
                    &tab_id_deny,
                    &call_id_deny,
                    ApprovalDecision::Deny(String::new()),
                    cx,
                );
            });
        });

        return div()
            .w_full()
            .rounded(px(8.))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                svg()
                                    .path(if is_dangerous {
                                        icons::INFO
                                    } else {
                                        icons::TERMINAL
                                    })
                                    .size(px(13.))
                                    .text_color(accent),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child(format!(
                                        "{} · {}",
                                        i18n::t(lang, "ai_agent.approval.title"),
                                        tool_label
                                    )),
                            ),
                    )
                    .when(is_dangerous, |this| {
                        this.child(
                            div()
                                .px(px(6.))
                                .py(px(1.))
                                .rounded(px(4.))
                                .bg(p.danger.opacity(0.18))
                                .text_size(px(10.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.danger)
                                .child(i18n::t(lang, "ai_agent.approval.dangerous_badge")),
                        )
                    }),
            )
            .child(
                div()
                    .w_full()
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(5.))
                    .bg(code_bg)
                    .font_family("JetBrains Mono")
                    .text_xs()
                    .text_color(p.text)
                    .whitespace_normal()
                    .child(SharedString::from(summary.to_string())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(6.))
                    .child(allow_btn)
                    .when(!is_dangerous, |this| {
                        this.child(
                            Button::new(SharedString::from(format!(
                                "ai-approve-session-{msg_idx}-{block_idx}"
                            )))
                            .outline()
                            .xsmall()
                            .label(i18n::t(lang, "ai_agent.approval.allow_session"))
                            .on_click(move |_, _, cx| {
                                session_always.update(cx, |s, cx| {
                                    s.respond_ai_tool_approval(
                                        &tab_id_session,
                                        &call_id_session,
                                        ApprovalDecision::AllowSession,
                                        cx,
                                    );
                                });
                            }),
                        )
                    })
                    .child(deny_btn),
            )
            .into_any_element();
    }

    // ===== 运行中 / 已完成 / 失败 / 已拒绝：紧凑可折叠工具卡片 =====
    let (status_icon, status_color, status_text) = match status {
        ToolCallStatus::Running => (icons::LOADER, p.primary, i18n::t(lang, "ai_agent.tool.running")),
        ToolCallStatus::Succeeded => (icons::CHECK, p.success, i18n::t(lang, "ai_agent.tool.done")),
        ToolCallStatus::Failed => (icons::X, p.danger, i18n::t(lang, "ai_agent.tool.failed")),
        ToolCallStatus::Denied => (icons::X, p.warning, i18n::t(lang, "ai_agent.tool.denied")),
        ToolCallStatus::WaitingApproval => unreachable!(),
    };
    let has_output = !output.trim().is_empty();
    let border_color = match status {
        ToolCallStatus::Failed => p.danger.opacity(0.35),
        ToolCallStatus::Denied => p.warning.opacity(0.35),
        _ => p.border,
    };

    div()
        .w_full()
        .rounded(px(6.))
        .bg(p.surface)
        .border_1()
        .border_color(border_color)
        .overflow_hidden()
        .child(
            div()
                .id(SharedString::from(format!(
                    "ai-tool-toggle-{msg_idx}-{block_idx}"
                )))
                .w_full()
                .px(px(8.))
                .py(px(6.))
                .flex()
                .items_center()
                .gap(px(6.))
                .when(has_output, |this| {
                    this.cursor_pointer().hover(move |s| s.bg(p.hover))
                })
                .child(
                    svg()
                        .path(status_icon)
                        .size(px(12.))
                        .flex_shrink_0()
                        .text_color(status_color),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.text_muted)
                        .child(tool_label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .font_family("JetBrains Mono")
                        .text_size(px(11.))
                        .text_color(p.text)
                        .child(SharedString::from(summary.to_string())),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(10.))
                        .text_color(status_color)
                        .child(status_text),
                )
                .when(has_output, |this| {
                    this.child(
                        svg()
                            .path(if collapsed {
                                icons::CHEVRON_RIGHT
                            } else {
                                icons::CHEVRON_DOWN
                            })
                            .size(px(11.))
                            .flex_shrink_0()
                            .text_color(p.text_muted),
                    )
                })
                .when(has_output, |this| {
                    this.on_click(move |_, _, cx| {
                        session_state.update(cx, |s, cx| {
                            s.toggle_ai_block_collapse(&tab_id, msg_idx, block_idx, cx);
                        });
                    })
                }),
        )
        .when(has_output && !collapsed, |this| {
            this.child(
                div()
                    .id(SharedString::from(format!(
                        "ai-tool-out-{msg_idx}-{block_idx}"
                    )))
                    .w_full()
                    .max_h(px(220.))
                    .overflow_y_scrollbar()
                    .border_t_1()
                    .border_color(p.border)
                    .bg(code_bg)
                    .px(px(8.))
                    .py(px(6.))
                    .font_family("JetBrains Mono")
                    .text_size(px(11.))
                    .line_height(relative(1.4))
                    .text_color(if status == ToolCallStatus::Failed {
                        p.danger
                    } else {
                        p.text_muted
                    })
                    .whitespace_normal()
                    .child(SharedString::from(output.to_string())),
            )
        })
        .into_any_element()
}

fn tool_display_name(lang: &Language, tool_name: &str) -> &'static str {
    match tool_name {
        "read_terminal_screen" => i18n::t(lang, "ai_agent.tool.read_screen"),
        "run_in_terminal" => i18n::t(lang, "ai_agent.tool.run_terminal"),
        "send_terminal_keys" => i18n::t(lang, "ai_agent.tool.send_keys"),
        "ssh_exec" => i18n::t(lang, "ai_agent.tool.ssh_exec"),
        _ => i18n::t(lang, "ai_agent.tool.generic"),
    }
}

fn approval_mode_short_label(lang: &Language, mode: AgentApprovalMode) -> &'static str {
    match mode {
        AgentApprovalMode::AskAll => i18n::t(lang, "ai_agent.approval.short.ask_all"),
        AgentApprovalMode::AutoReadOnly => i18n::t(lang, "ai_agent.approval.short.auto_read_only"),
        AgentApprovalMode::AutoSafe => i18n::t(lang, "ai_agent.approval.short.auto_safe"),
    }
}

#[cfg(test)]
mod tests {
    use super::reasoning_single_line_preview;

    #[test]
    fn reasoning_preview_flattens_and_slices_tail_or_head() {
        let raw = "  第一步：\n检查磁盘\t占用  \n\n第二步：检查内存  ";
        assert_eq!(
            reasoning_single_line_preview(raw, true, 100),
            "第一步： 检查磁盘 占用 第二步：检查内存"
        );
        assert_eq!(reasoning_single_line_preview(raw, true, 7), "第二步：检查内存".chars().skip(1).collect::<String>());
        assert_eq!(reasoning_single_line_preview(raw, false, 4), "第一步：");
    }
}
