// AI 对话相关方法：维护每个 tab 的输入框、发送消息

use super::{AiChatMessage, SessionState};
use crate::models::ProviderRef;
use crate::services::ai::{self, ChatMessage, ChatRole};
use gpui::{AppContext, Entity};
use gpui_component::input::InputState;

impl SessionState {
    /// 确保指定 tab 的 AI 对话输入框已创建
    pub fn ensure_ai_chat_input(
        &mut self,
        tab_id: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<InputState> {
        if !self.ai_chat_inputs.contains_key(tab_id) {
            let lang = crate::services::storage::load_settings()
                .map(|s| s.theme.language)
                .unwrap_or_default();
            let placeholder = crate::i18n::t(&lang, "ai_chat.placeholder");
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(1, 6)
            });
            self.ai_chat_inputs.insert(tab_id.to_string(), input);
        }
        self.ai_chat_inputs.get(tab_id).cloned().unwrap()
    }

    pub fn get_ai_chat_input(&self, tab_id: &str) -> Option<Entity<InputState>> {
        self.ai_chat_inputs.get(tab_id).cloned()
    }

    /// 切换 chat panel 头部里选择的供应商
    pub fn set_ai_chat_provider(&mut self, tab_id: &str, provider: ProviderRef) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.ai_chat.selected_provider = Some(provider);
            // 供应商变了，已选模型可能不再属于新供应商，重置为默认
            tab.ai_chat.selected_model = None;
        }
    }

    /// 设置底部选择的模型
    pub fn set_ai_chat_model(&mut self, tab_id: &str, model: String) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.ai_chat.selected_model = Some(model);
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
        // 用户已选且仍在该供应商列表内 → 用之
        if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
            if let Some(sel) = &tab.ai_chat.selected_model {
                if list.iter().any(|m| m == sel) {
                    return sel.clone();
                }
            }
        }
        // 否则用供应商默认模型
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
        // 用户选择优先：能解析且已验证可用则直接用
        if let Some(sel) = &tab.ai_chat.selected_provider {
            if let Some(r) = settings.ai.resolve(sel) {
                if r.verified && !r.api_key.is_empty() {
                    return Some(sel.clone());
                }
            }
        }
        // 回退：默认内置（若可用）→ 任一已验证供应商
        let verified = settings.ai.verified_provider_refs();
        let default_ref = ProviderRef::Builtin(settings.ai.default_provider);
        if verified.contains(&default_ref) {
            return Some(default_ref);
        }
        verified.into_iter().next()
    }

    /// 切换某条消息的思考过程折叠状态
    pub fn toggle_ai_reasoning(
        &mut self,
        tab_id: &str,
        msg_idx: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if let Some(msg) = tab.ai_chat.messages.get_mut(msg_idx) {
                msg.reasoning_collapsed = !msg.reasoning_collapsed;
                cx.notify();
            }
        }
    }

    /// 清空当前 tab 的 AI 对话
    pub fn clear_ai_chat(&mut self, tab_id: &str, cx: &mut gpui::Context<Self>) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.ai_chat.messages.clear();
            tab.ai_chat.pending = false;
            cx.notify();
        }
    }

    /// 构建终端上下文（连接信息 / 操作系统 / 终端最近输出），用于注入到对话。
    /// 关闭总开关或无任何可携带内容时返回 None。
    fn build_terminal_context(&self, tab_id: &str, cx: &gpui::App) -> Option<String> {
        let settings = crate::services::storage::load_settings().ok()?;
        let ctx = &settings.ai.context;
        if !ctx.enabled {
            return None;
        }
        let tab = self.tabs.iter().find(|t| t.id == tab_id)?;

        let mut sections: Vec<String> = Vec::new();

        // 连接信息
        if ctx.server_info {
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

        // 操作系统信息（复用 Monitor 采集，无则跳过）
        if ctx.os_info {
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

        // 终端最近输出
        if ctx.terminal_output {
            let term = tab
                .active_terminal_id
                .as_ref()
                .and_then(|id| tab.terminals.iter().find(|t| &t.id == id))
                .and_then(|inst| inst.terminal.as_ref());
            if let Some(term) = term {
                let text = term.read(cx).visible_text(ctx.output_lines as usize);
                if !text.trim().is_empty() {
                    sections.push(format!("终端最近输出:\n```\n{}\n```", text));
                }
            }
        }

        if sections.is_empty() {
            return None;
        }
        Some(format!(
            "以下是用户当前终端会话的环境上下文，供你参考以给出更贴合的回答（这些信息由系统自动附加，并非用户输入）：\n\n{}",
            sections.join("\n\n")
        ))
    }

    /// 发送当前 tab 的 AI 对话消息
    pub fn send_ai_chat_message(
        &mut self,
        tab_id: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // 取输入框内容
        let input = match self.ai_chat_inputs.get(tab_id).cloned() {
            Some(i) => i,
            None => return,
        };
        let text = input.read(cx).value().to_string();
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }

        // 解析供应商
        let provider_ref = match self.resolve_ai_provider_ref(tab_id) {
            Some(p) => p,
            None => {
                // 没有可用供应商：把错误塞到消息列表里
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    let lang = crate::services::storage::load_settings()
                        .map(|s| s.theme.language)
                        .unwrap_or_default();
                    tab.ai_chat.messages.push(AiChatMessage {
                        role: ChatRole::Assistant,
                        content: crate::i18n::t(&lang, "ai_chat.no_provider").to_string(),
                        error: true,
                        reasoning: None,
                        reasoning_collapsed: false,
                    });
                    cx.notify();
                }
                return;
            }
        };

        // 解析供应商统一视图
        let settings = match crate::services::storage::load_settings() {
            Ok(s) => s,
            Err(_) => return,
        };
        let Some(mut resolved) = settings.ai.resolve(&provider_ref) else {
            return;
        };
        // 用底部下拉所选模型覆盖
        resolved.model = self.resolve_ai_model(tab_id, &provider_ref);
        let system_prompt = settings.ai.system_prompt.clone();

        // 在可变借用 tabs 之前，先算好终端上下文（需要不可变读取 self 与 cx）
        let terminal_context = self.build_terminal_context(tab_id, cx);

        // 追加用户消息、标记 pending、清空输入
        let history: Vec<ChatMessage> = {
            let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            tab.ai_chat.messages.push(AiChatMessage {
                role: ChatRole::User,
                content: text.clone(),
                error: false,
                reasoning: None,
                        reasoning_collapsed: false,
            });
            tab.ai_chat.pending = true;
            let mut msgs = Vec::new();
            // 注入系统提示词（如果用户配置了）
            let trimmed = system_prompt.trim();
            if !trimmed.is_empty() {
                msgs.push(ChatMessage {
                    role: ChatRole::System,
                    content: system_prompt.clone(),
                });
            }
            // 注入终端上下文（如果开启且有内容），作为额外的 system 消息
            if let Some(ctx) = terminal_context {
                msgs.push(ChatMessage {
                    role: ChatRole::System,
                    content: ctx,
                });
            }
            msgs.extend(
                tab.ai_chat
                    .messages
                    .iter()
                    .filter(|m| !m.error)
                    .map(|m| ChatMessage {
                        role: m.role,
                        content: m.content.clone(),
                    }),
            );
            msgs
        };
        input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        cx.notify();

        // 先在消息列表里插入一条空的助手消息，后续增量追加到它的 content/reasoning
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.ai_chat.messages.push(AiChatMessage {
                role: ChatRole::Assistant,
                content: String::new(),
                error: false,
                reasoning: None,
                        reasoning_collapsed: false,
            });
        }
        cx.notify();

        // 启动后台流式请求
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ai::StreamEvent>();
        let (err_tx, err_rx) = tokio::sync::oneshot::channel::<Option<String>>();
        let format = resolved.format;
        let api_key = resolved.api_key.clone();
        let base_url = resolved.base_url.clone();
        let model = resolved.model.clone();
        let runtime = crate::ssh::manager::SshManager::global().runtime();
        runtime.spawn(async move {
            let result = ai::chat_completion_stream(
                format, &api_key, &base_url, &model, &history, tx,
            )
            .await;
            let _ = err_tx.send(result.err().map(|e| e.to_string()));
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
                            let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_for_update) else {
                                return;
                            };
                            let Some(msg) = tab.ai_chat.messages.last_mut() else {
                                return;
                            };
                            match ev {
                                ai::StreamEvent::ReasoningDelta(d) => {
                                    msg.reasoning.get_or_insert_with(String::new).push_str(&d);
                                }
                                ai::StreamEvent::ContentDelta(d) => {
                                    // 正文开始 → 自动折叠思考过程
                                    if msg.reasoning.is_some() {
                                        msg.reasoning_collapsed = true;
                                    }
                                    msg.content.push_str(&d);
                                }
                                ai::StreamEvent::Done => {}
                            }
                            cx.notify();
                        });
                    });
                }

                // 检查最终结果，若有错误则替换/追加错误内容
                let err = err_rx.await.ok().flatten();
                let _ = async_cx.update(|cx| {
                    session_entity.update(cx, |state, cx| {
                        if let Some(tab) =
                            state.tabs.iter_mut().find(|t| t.id == tab_id_owned)
                        {
                            tab.ai_chat.pending = false;
                            // 完成时若有思考过程也折叠（兜底：没有正文也折叠）
                            if let Some(last) = tab.ai_chat.messages.last_mut() {
                                if last.reasoning.is_some() {
                                    last.reasoning_collapsed = true;
                                }
                            }
                            if let Some(err_msg) = err {
                                // 若当前助手消息还没写内容，直接替换；否则单独追加一条错误
                                if let Some(last) = tab.ai_chat.messages.last_mut() {
                                    if last.role == ChatRole::Assistant
                                        && last.content.is_empty()
                                        && last.reasoning.is_none()
                                    {
                                        last.content = err_msg;
                                        last.error = true;
                                    } else {
                                        tab.ai_chat.messages.push(AiChatMessage {
                                            role: ChatRole::Assistant,
                                            content: err_msg,
                                            error: true,
                                            reasoning: None,
                        reasoning_collapsed: false,
                                        });
                                    }
                                }
                            }
                            cx.notify();
                        }
                    });
                });
                Some(())
            })
            .detach();
    }
}
