// AI Agent 对话与终端操控状态管理：维护每个 tab 的输入框、流式事件、工具审批与中断控制

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use gpui_kit::component::input::TextareaState;
use gpui_kit::{AppContext, Entity};
use tokio_util::sync::CancellationToken;

use super::{AgentMessageBlock, AiChatMessage, SessionState, ToolCallStatus};
use crate::models::{AgentApprovalMode, AgentReasoningEffort, ProviderRef};
use crate::services::agent::{
    self, AgentEvent, AgentRunRequest, AgentSessionContext, ApprovalDecision,
};
use crate::services::ai::{ChatMessage, ChatRole};

impl SessionState {
    /// 确保指定 tab 的 AI Agent 输入框已创建
    pub fn ensure_ai_chat_input(
        &mut self,
        tab_id: &str,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> Entity<TextareaState> {
        if !self.ai_chat_inputs.contains_key(tab_id) {
            let lang = crate::services::storage::load_settings()
                .map(|s| s.theme.language)
                .unwrap_or_default();
            let placeholder = crate::i18n::t(&lang, "ai_chat.placeholder");
            let input = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(1, 6)
            });
            self.ai_chat_inputs.insert(tab_id.to_string(), input);
        }
        self.ai_chat_inputs.get(tab_id).cloned().unwrap()
    }

    pub fn get_ai_chat_input(&self, tab_id: &str) -> Option<Entity<TextareaState>> {
        self.ai_chat_inputs.get(tab_id).cloned()
    }

    /// 切换 Agent 面板底部选择的供应商
    pub fn set_ai_chat_provider(&mut self, tab_id: &str, provider: ProviderRef) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.ai_chat.selected_provider = Some(provider);
            tab.ai_chat.selected_model = None;
        }
    }

    /// 设置底部选择的模型
    pub fn set_ai_chat_model(&mut self, tab_id: &str, model: String) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.ai_chat.selected_model = Some(model);
        }
    }

    /// 快捷切换全局 Agent 审批模式并持久化到设置（与设置弹窗保持同步）
    pub fn set_ai_approval_mode(
        &mut self,
        mode: AgentApprovalMode,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Ok(mut settings) = crate::services::storage::load_settings() {
            settings.ai.agent.approval_mode = mode;
            let _ = crate::services::storage::save_settings(&settings);
            cx.notify();
        }
    }

    /// 快捷切换全局 Agent 思考深度并持久化到设置（与设置弹窗保持同步）
    pub fn set_ai_reasoning_effort(
        &mut self,
        effort: AgentReasoningEffort,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Ok(mut settings) = crate::services::storage::load_settings() {
            settings.ai.agent.reasoning_effort = effort;
            let _ = crate::services::storage::save_settings(&settings);
            cx.notify();
        }
    }

    /// 解析当前 tab 实际使用的模型名
    pub fn resolve_ai_model(&self, tab_id: &str, provider: &ProviderRef) -> String {
        let settings = crate::services::storage::load_settings().unwrap_or_default();
        let resolved = settings.ai.resolve(provider);
        let list = resolved
            .as_ref()
            .map(|r| r.models.clone())
            .unwrap_or_default();
        if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
            if let Some(sel) = &tab.ai_chat.selected_model {
                if list.iter().any(|m| m == sel) {
                    return sel.clone();
                }
            }
        }
        resolved
            .map(|r| r.model)
            .filter(|m| !m.is_empty())
            .or_else(|| list.into_iter().next())
            .unwrap_or_default()
    }

    /// 解析当前 tab 实际使用的供应商引用（内置或自定义）
    pub fn resolve_ai_provider_ref(&self, tab_id: &str) -> Option<ProviderRef> {
        let settings = crate::services::storage::load_settings().ok()?;
        let tab = self.tabs.iter().find(|t| t.id == tab_id)?;
        if let Some(sel) = &tab.ai_chat.selected_provider {
            if let Some(r) = settings.ai.resolve(sel) {
                if r.verified && !r.api_key.is_empty() {
                    return Some(sel.clone());
                }
            }
        }
        let verified = settings.ai.verified_provider_refs();
        let default_ref = ProviderRef::Builtin(settings.ai.default_provider);
        if verified.contains(&default_ref) {
            return Some(default_ref);
        }
        verified.into_iter().next()
    }

    /// 切换某条助手消息里单个折叠块（思考过程 / 工具调用输出）的展开状态
    pub fn toggle_ai_block_collapse(
        &mut self,
        tab_id: &str,
        msg_idx: usize,
        block_idx: usize,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if let Some(msg) = tab.ai_chat.messages.get_mut(msg_idx) {
                if let Some(block) = msg.blocks.get_mut(block_idx) {
                    match block {
                        AgentMessageBlock::Reasoning { collapsed, .. }
                        | AgentMessageBlock::ToolCall { collapsed, .. } => {
                            *collapsed = !*collapsed;
                            cx.notify();
                        }
                        AgentMessageBlock::Text(_) => {}
                    }
                }
            }
        }
    }

    /// 响应一个待审批的工具调用（允许本次 / 本次会话允许 / 拒绝）
    pub fn respond_ai_tool_approval(
        &mut self,
        _tab_id: &str,
        call_id: &str,
        decision: ApprovalDecision,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Some(tx) = self.ai_pending_approvals.remove(call_id) {
            let _ = tx.send(decision);
            cx.notify();
        }
    }

    /// 停止当前 tab 正在运行的 Agent 任务（中止流式请求、等待中的审批及命令执行）
    pub fn stop_ai_agent(&mut self, tab_id: &str, cx: &mut gpui_kit::Context<Self>) {
        if let Some(token) = self.ai_cancel_tokens.remove(tab_id) {
            token.cancel();
        }
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            // 清理该 tab 中仍处于待审批的通道，避免悬空卡片
            for msg in &mut tab.ai_chat.messages {
                for block in &mut msg.blocks {
                    if let AgentMessageBlock::ToolCall {
                        call_id, status, ..
                    } = block
                    {
                        if *status == ToolCallStatus::WaitingApproval {
                            self.ai_pending_approvals.remove(call_id);
                            *status = ToolCallStatus::Denied;
                        }
                    }
                }
            }
            tab.ai_chat.pending = false;
            cx.notify();
        }
    }

    /// 清空当前 tab 的 Agent 对话并重置会话级允许白名单
    pub fn clear_ai_chat(&mut self, tab_id: &str, cx: &mut gpui_kit::Context<Self>) {
        if let Some(token) = self.ai_cancel_tokens.remove(tab_id) {
            token.cancel();
        }
        self.ai_session_allowed_tools.remove(tab_id);
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            for msg in &tab.ai_chat.messages {
                for block in &msg.blocks {
                    if let AgentMessageBlock::ToolCall { call_id, .. } = block {
                        self.ai_pending_approvals.remove(call_id);
                    }
                }
            }
            tab.ai_chat.messages.clear();
            tab.ai_chat.pending = false;
            tab.ai_chat.reset_scroll();
            cx.notify();
        }
    }

    /// 处理 AI 对话消息列表的滚轮滚动，用于维护智能贴底状态
    pub fn on_ai_chat_scroll_wheel(&mut self, tab_id: &str, delta_y: gpui_kit::Pixels) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.ai_chat.on_scroll_wheel(delta_y);
        }
    }

    /// 立即滚到 AI 对话底部并恢复智能贴底跟随
    pub fn scroll_ai_chat_to_bottom(&mut self, tab_id: &str, cx: &mut gpui_kit::Context<Self>) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.ai_chat.pin_and_scroll_to_bottom();
            cx.notify();
        }
    }

    /// 关闭 tab 时清理该 tab 关联的所有 Agent 运行期资源
    pub fn cleanup_ai_agent_for_tab(&mut self, tab_id: &str) {
        if let Some(token) = self.ai_cancel_tokens.remove(tab_id) {
            token.cancel();
        }
        self.ai_session_allowed_tools.remove(tab_id);
        self.ai_chat_inputs.remove(tab_id);
    }

    /// 构建静态/轻量终端上下文（连接信息 / 操作系统 / 可选首屏输出），注入到 Agent 系统提示词中。
    fn build_terminal_context(&self, tab_id: &str, cx: &gpui_kit::App) -> Option<String> {
        let settings = crate::services::storage::load_settings().ok()?;
        let ctx = &settings.ai.context;
        let tab = self.tabs.iter().find(|t| t.id == tab_id)?;

        let mut sections: Vec<String> = Vec::new();

        // 连接信息（即使关闭附带终端输出，Agent 也默认需要知道连到了哪台机器）
        if !ctx.enabled || ctx.server_info {
            if let Some(sd) = &tab.server_data {
                let mut lines = Vec::new();
                if !sd.label.trim().is_empty() {
                    lines.push(format!("- 名称: {}", sd.label));
                }
                if !sd.host.trim().is_empty() {
                    lines.push(format!("- 主机: {}:{}", sd.host, sd.port));
                }
                if !sd.username.trim().is_empty() {
                    lines.push(format!("- 用户名: {}", sd.username));
                }
                if !lines.is_empty() {
                    sections.push(format!("连接信息:\n{}", lines.join("\n")));
                }
            }
        }

        // 操作系统信息（复用 Monitor 采集）
        if !ctx.enabled || ctx.os_info {
            if let Some(info) = &tab.monitor_state.system_info {
                let mut lines = Vec::new();
                if !info.host.hostname.trim().is_empty() {
                    lines.push(format!("- 主机名: {}", info.host.hostname));
                }
                if !info.host.os.trim().is_empty() {
                    lines.push(format!("- 操作系统: {}", info.host.os));
                }
                if !info.host.kernel.trim().is_empty() {
                    lines.push(format!("- 内核: {}", info.host.kernel));
                }
                if !info.cpu.architecture.trim().is_empty() {
                    lines.push(format!("- 架构: {}", info.cpu.architecture));
                }
                if !lines.is_empty() {
                    sections.push(format!("系统信息:\n{}", lines.join("\n")));
                }
            }
        }

        // 仅当用户显式开启“自动携带终端最近输出”时附带首屏快照；否则 Agent 可按需调用 read_terminal_screen
        if ctx.enabled && ctx.terminal_output {
            let term = tab
                .active_terminal_id
                .as_ref()
                .and_then(|id| tab.terminals.iter().find(|t| &t.id == id))
                .and_then(|inst| inst.terminal.as_ref());
            if let Some(term) = term {
                let text = term.read(cx).visible_text(ctx.output_lines as usize);
                if !text.trim().is_empty() {
                    sections.push(format!("当前终端最近输出:\n```\n{}\n```", text));
                }
            }
        }

        if sections.is_empty() {
            return None;
        }
        Some(sections.join("\n\n"))
    }

    /// 向当前 tab 的 AI Agent 发送消息并启动多步工具执行流
    pub fn send_ai_chat_message(
        &mut self,
        tab_id: &str,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        // 如果已有运行中的任务，避免并发提交
        if self
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.ai_chat.pending)
            .unwrap_or(false)
        {
            return;
        }

        let input = match self.ai_chat_inputs.get(tab_id).cloned() {
            Some(i) => i,
            None => return,
        };
        let text = input.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }

        let provider_ref = match self.resolve_ai_provider_ref(tab_id) {
            Some(p) => p,
            None => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    let lang = crate::services::storage::load_settings()
                        .map(|s| s.theme.language)
                        .unwrap_or_default();
                    tab.ai_chat.messages.push(AiChatMessage {
                        role: ChatRole::Assistant,
                        content: crate::i18n::t(&lang, "ai_chat.no_provider").to_string(),
                        blocks: Vec::new(),
                        error: true,
                    });
                    tab.ai_chat.pin_and_scroll_to_bottom();
                    cx.notify();
                }
                return;
            }
        };

        let settings = match crate::services::storage::load_settings() {
            Ok(s) => s,
            Err(_) => return,
        };
        let Some(mut resolved) = settings.ai.resolve(&provider_ref) else {
            return;
        };
        resolved.model = self.resolve_ai_model(tab_id, &provider_ref);
        let system_prompt = settings.ai.system_prompt.clone();
        let agent_settings = settings.ai.agent.clone();

        // 收集当前 tab 的线程安全终端与 SSH 句柄（在可变借用 self.tabs 前读取）
        let terminal_context = self.build_terminal_context(tab_id, cx);
        let (active_pty, active_term) = self
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|tab| {
                tab.active_terminal_id
                    .as_ref()
                    .and_then(|id| tab.terminals.iter().find(|t| &t.id == id))
                    .or_else(|| tab.terminals.first())
            })
            .map(|inst| {
                let pty = inst.pty_channel.clone();
                let term = inst.terminal.as_ref().map(|t| t.read(cx).term().clone());
                (pty, term)
            })
            .unwrap_or((None, None));
        let ssh_session = crate::ssh::manager::SshManager::global().get_session(tab_id);

        // 组装历史（不含本次用户输入）并追加用户消息与空助手占位消息
        let history: Vec<ChatMessage> = {
            let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            let prev: Vec<ChatMessage> = tab
                .ai_chat
                .messages
                .iter()
                .filter(|m| !m.error)
                .map(|m| ChatMessage {
                    role: m.role,
                    content: m.history_text(),
                })
                .filter(|m| !m.content.trim().is_empty())
                .collect();

            tab.ai_chat.messages.push(AiChatMessage {
                role: ChatRole::User,
                content: text.clone(),
                blocks: Vec::new(),
                error: false,
            });
            tab.ai_chat.messages.push(AiChatMessage {
                role: ChatRole::Assistant,
                content: String::new(),
                blocks: Vec::new(),
                error: false,
            });
            tab.ai_chat.pending = true;
            tab.ai_chat.pin_and_scroll_to_bottom();
            prev
        };

        input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });

        let cancel_token = CancellationToken::new();
        self.ai_cancel_tokens
            .insert(tab_id.to_string(), cancel_token.clone());

        let session_allowed = self
            .ai_session_allowed_tools
            .entry(tab_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(HashSet::new())))
            .clone();

        cx.notify();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();
        let req = AgentRunRequest {
            provider: resolved,
            system_prompt,
            terminal_context,
            history,
            user_prompt: text,
            agent_settings: agent_settings.clone(),
            session_ctx: AgentSessionContext {
                ssh_session,
                active_pty,
                active_term,
                command_timeout_secs: agent_settings.command_timeout_secs.max(1) as u64,
                cancel_token: cancel_token.clone(),
            },
            session_allowed_tools: session_allowed,
            cancel_token,
        };

        let runtime = crate::ssh::manager::SshManager::global().runtime();
        runtime.spawn(async move {
            agent::run_agent(req, tx).await;
        });

        let session_entity = cx.entity().clone();
        let tab_id_owned = tab_id.to_string();
        cx.to_async()
            .spawn(async move |async_cx| {
                while let Some(ev) = rx.recv().await {
                    let session = session_entity.clone();
                    let tab_id_for_update = tab_id_owned.clone();
                    let _ = async_cx.update(|cx| {
                        session.update(cx, |state, cx| {
                            state.apply_agent_event(&tab_id_for_update, ev, cx);
                        });
                    });
                }

                // 兜底：若任务通道意外提前关闭，确保 pending 状态复位
                let _ = async_cx.update(|cx| {
                    session_entity.update(cx, |state, cx| {
                        state.ai_cancel_tokens.remove(&tab_id_owned);
                        if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_owned) {
                            if tab.ai_chat.pending {
                                tab.ai_chat.pending = false;
                                cx.notify();
                            }
                        }
                    });
                });
                Some(())
            })
            .detach();
    }

    fn apply_agent_event(
        &mut self,
        tab_id: &str,
        ev: AgentEvent,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        // 先处理需要存入 SessionState（非 SessionTab）的审批通道
        let ev = match ev {
            AgentEvent::ToolApprovalRequired {
                call_id,
                tool_name,
                summary,
                risk,
                responder,
            } => {
                self.ai_pending_approvals.insert(call_id.clone(), responder);
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    if let Some(msg) = tab.ai_chat.messages.last_mut() {
                        upsert_tool_block(
                            msg,
                            call_id,
                            tool_name,
                            summary,
                            risk,
                            ToolCallStatus::WaitingApproval,
                            false,
                        );
                    }
                    tab.ai_chat.scroll_to_bottom_if_pinned();
                }
                cx.notify();
                return;
            }
            other => other,
        };

        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };

        match ev {
            AgentEvent::ReasoningDelta(d) => {
                if let Some(msg) = tab.ai_chat.messages.last_mut() {
                    if let Some(AgentMessageBlock::Reasoning { text, .. }) = msg.blocks.last_mut() {
                        text.push_str(&d);
                    } else {
                        msg.blocks.push(AgentMessageBlock::Reasoning {
                            text: d,
                            // 默认折叠，侧边栏以单行滚动条展示思考过程，避免占用大量纵向空间
                            collapsed: true,
                        });
                    }
                }
            }
            AgentEvent::TextDelta(d) => {
                if let Some(msg) = tab.ai_chat.messages.last_mut() {
                    msg.content.push_str(&d);
                    if let Some(AgentMessageBlock::Text(text)) = msg.blocks.last_mut() {
                        text.push_str(&d);
                    } else {
                        msg.blocks.push(AgentMessageBlock::Text(d));
                    }
                }
            }
            AgentEvent::ToolRunning {
                call_id,
                tool_name,
                summary,
                risk,
            } => {
                if let Some(msg) = tab.ai_chat.messages.last_mut() {
                    upsert_tool_block(
                        msg,
                        call_id,
                        tool_name,
                        summary,
                        risk,
                        ToolCallStatus::Running,
                        true,
                    );
                }
            }
            AgentEvent::ToolDenied { call_id, reason } => {
                if let Some(msg) = tab.ai_chat.messages.last_mut() {
                    for block in &mut msg.blocks {
                        if let AgentMessageBlock::ToolCall {
                            call_id: id,
                            status,
                            output,
                            collapsed,
                            ..
                        } = block
                        {
                            if id == &call_id {
                                *status = ToolCallStatus::Denied;
                                *output = reason.clone();
                                *collapsed = true;
                                break;
                            }
                        }
                    }
                }
            }
            AgentEvent::ToolFinished {
                call_id,
                tool_name,
                is_error,
                output: tool_output,
            } => {
                if let Some(msg) = tab.ai_chat.messages.last_mut() {
                    let mut updated = false;
                    for block in &mut msg.blocks {
                        if let AgentMessageBlock::ToolCall {
                            call_id: id,
                            status,
                            output,
                            collapsed,
                            ..
                        } = block
                        {
                            if id == &call_id {
                                if *status != ToolCallStatus::Denied {
                                    *status = if is_error {
                                        ToolCallStatus::Failed
                                    } else {
                                        ToolCallStatus::Succeeded
                                    };
                                    *output = tool_output.clone();
                                    // 出错时自动展开便于排查，正常输出默认折叠保持界面清爽
                                    *collapsed = !is_error;
                                }
                                updated = true;
                                break;
                            }
                        }
                    }
                    if !updated {
                        msg.blocks.push(AgentMessageBlock::ToolCall {
                            call_id,
                            tool_name: tool_name.clone(),
                            summary: tool_name,
                            risk: crate::services::agent::ToolRisk::Execute,
                            status: if is_error {
                                ToolCallStatus::Failed
                            } else {
                                ToolCallStatus::Succeeded
                            },
                            output: tool_output,
                            collapsed: !is_error,
                        });
                    }
                }
            }
            AgentEvent::Done | AgentEvent::Cancelled => {
                tab.ai_chat.pending = false;
                self.ai_cancel_tokens.remove(tab_id);
            }
            AgentEvent::Error(err_msg) => {
                tab.ai_chat.pending = false;
                self.ai_cancel_tokens.remove(tab_id);
                if let Some(last) = tab.ai_chat.messages.last_mut() {
                    if last.role == ChatRole::Assistant
                        && last.content.is_empty()
                        && last.blocks.is_empty()
                    {
                        last.content = err_msg;
                        last.error = true;
                    } else {
                        tab.ai_chat.messages.push(AiChatMessage {
                            role: ChatRole::Assistant,
                            content: err_msg,
                            blocks: Vec::new(),
                            error: true,
                        });
                    }
                }
            }
            AgentEvent::ToolApprovalRequired { .. } => unreachable!(),
        }

        tab.ai_chat.scroll_to_bottom_if_pinned();
        cx.notify();
    }
}

fn upsert_tool_block(
    msg: &mut AiChatMessage,
    call_id: String,
    tool_name: String,
    summary: String,
    risk: crate::services::agent::ToolRisk,
    new_status: ToolCallStatus,
    new_collapsed: bool,
) {
    for block in &mut msg.blocks {
        if let AgentMessageBlock::ToolCall {
            call_id: id,
            status,
            collapsed,
            ..
        } = block
        {
            if id == &call_id {
                *status = new_status;
                *collapsed = new_collapsed;
                return;
            }
        }
    }
    msg.blocks.push(AgentMessageBlock::ToolCall {
        call_id,
        tool_name,
        summary,
        risk,
        status: new_status,
        output: String::new(),
        collapsed: new_collapsed,
    });
}

#[cfg(test)]
mod tests {
    use super::super::AiChatState;
    use gpui_kit::px;

    #[test]
    fn sticky_scroll_stays_pinned_when_unscrollable_or_near_bottom() {
        assert!(AiChatState::next_auto_scroll_from_position(
            true,
            px(0.),
            px(0.)
        ));
        assert!(AiChatState::next_auto_scroll_from_position(
            true,
            px(400.),
            px(-400.)
        ));
        assert!(AiChatState::next_auto_scroll_from_position(
            true,
            px(400.),
            px(-380.)
        ));
    }

    #[test]
    fn sticky_scroll_detaches_when_dragged_away_and_reengages_at_bottom() {
        // 拖动滚动条离开底部超过阈值后暂停跟随
        assert!(!AiChatState::next_auto_scroll_from_position(
            true,
            px(400.),
            px(-320.)
        ));
        // 停留在距离底部 12px 处时保持暂停，不误吸回
        assert!(!AiChatState::next_auto_scroll_from_position(
            false,
            px(400.),
            px(-388.)
        ));
        // 拖回到最底部附近后恢复贴底跟随
        assert!(AiChatState::next_auto_scroll_from_position(
            false,
            px(400.),
            px(-398.)
        ));
    }

    #[test]
    fn wheel_up_immediately_detaches_and_wheel_down_to_bottom_reengages() {
        // 向上滚轮（delta_y > 0）立即暂停自动贴底
        assert!(!AiChatState::next_auto_scroll_from_wheel(
            true,
            px(500.),
            px(-500.),
            px(12.)
        ));
        // 向下滚轮但仍远离底部时保持暂停
        assert!(!AiChatState::next_auto_scroll_from_wheel(
            false,
            px(500.),
            px(-200.),
            px(-40.)
        ));
        // 向下滚轮到达底部后恢复自动贴底
        assert!(AiChatState::next_auto_scroll_from_wheel(
            false,
            px(500.),
            px(-485.),
            px(-20.)
        ));
    }
}
