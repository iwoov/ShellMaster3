// AI Agent 服务层：基于 rig-core + rig-agent，完全隔离对外接口

pub mod policy;
pub mod prompt;
pub mod provider;
pub mod tools;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use rig_agent::agent::MultiTurnStreamItem;
use rig_agent::AgentBuilder;
use rig_core::message::{Message, ToolResultContent};
use rig_core::streaming::{Item, StreamEvent};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::models::settings::ResolvedProvider;
use crate::models::AgentSettings;
use crate::services::ai::{ChatMessage, ChatRole};

use self::policy::AgentPolicyHook;
use self::prompt::build_agent_system_prompt;
use self::provider::{build_generation_options, build_model};
pub use self::tools::AgentSessionContext;
use self::tools::read_screen::ReadTerminalScreenTool;
use self::tools::run_terminal::RunInTerminalTool;
use self::tools::send_keys::SendTerminalKeysTool;
use self::tools::ssh_exec::SshExecTool;

/// 工具风险等级
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolRisk {
    /// 只读（如查看终端屏幕、白名单诊断命令）
    ReadOnly,
    /// 普通执行（写 PTY 或非常规命令）
    Execute,
    /// 高危/破坏性命令（强制确认 + 红色警示）
    Dangerous,
}

/// 用户对工具审批请求的决定
#[derive(Debug)]
pub enum ApprovalDecision {
    /// 仅允许本次调用
    AllowOnce,
    /// 本次会话允许该工具（不含高危命令）
    AllowSession,
    /// 拒绝执行，可附带原因
    Deny(String),
}

/// Agent 执行期间向 GPUI 推送的结构化流式事件
pub enum AgentEvent {
    /// 思考过程增量（DeepSeek-R1 等推理模型）
    ReasoningDelta(String),
    /// 正文增量
    TextDelta(String),
    /// 需要用户确认是否执行该工具
    ToolApprovalRequired {
        call_id: String,
        tool_name: String,
        summary: String,
        risk: ToolRisk,
        responder: oneshot::Sender<ApprovalDecision>,
    },
    /// 工具已放行（自动放行或用户点击允许），开始执行
    ToolRunning {
        call_id: String,
        tool_name: String,
        summary: String,
        risk: ToolRisk,
    },
    /// 用户拒绝了该工具调用
    ToolDenied { call_id: String, reason: String },
    /// 工具执行完成（成功或失败）
    ToolFinished {
        call_id: String,
        tool_name: String,
        is_error: bool,
        output: String,
    },
    /// 整轮 Agent 任务正常结束
    Done,
    /// 任务被用户主动取消
    Cancelled,
    /// 运行期错误
    Error(String),
}

/// 发起一次 Agent 运行的完整请求
pub struct AgentRunRequest {
    pub provider: ResolvedProvider,
    pub system_prompt: String,
    pub terminal_context: Option<String>,
    /// 历史对话（不含本轮 user_prompt）
    pub history: Vec<ChatMessage>,
    /// 本轮用户输入
    pub user_prompt: String,
    pub agent_settings: AgentSettings,
    pub session_ctx: AgentSessionContext,
    pub session_allowed_tools: Arc<Mutex<HashSet<String>>>,
    pub cancel_token: CancellationToken,
}

/// 在 Tokio 运行时中执行一轮多步 Agent 任务，并持续通过 `tx` 推送 `AgentEvent`。
pub async fn run_agent(req: AgentRunRequest, tx: mpsc::UnboundedSender<AgentEvent>) {
    let model = match build_model(&req.provider) {
        Ok(m) => m,
        Err(err) => {
            let _ = tx.send(AgentEvent::Error(err));
            return;
        }
    };

    let has_pty_tools =
        req.agent_settings.enable_pty_tools && req.session_ctx.active_pty.is_some();
    let has_exec_tool =
        req.agent_settings.enable_exec_tool && req.session_ctx.ssh_session.is_some();

    let preamble = build_agent_system_prompt(
        &req.system_prompt,
        req.terminal_context.as_deref(),
        has_pty_tools,
        has_exec_tool,
    );

    let mut builder = AgentBuilder::new(model)
        .name("shellmaster-agent")
        .preamble(preamble);

    if let Some(opts) = build_generation_options(req.agent_settings.reasoning_effort, &req.provider) {
        builder = builder.options(opts);
    }

    if req.session_ctx.active_term.is_some() {
        builder = builder.tool(ReadTerminalScreenTool::new(req.session_ctx.clone()));
    }
    if has_pty_tools {
        builder = builder
            .tool(RunInTerminalTool::new(req.session_ctx.clone()))
            .tool(SendTerminalKeysTool::new(req.session_ctx.clone()));
    }
    if has_exec_tool {
        builder = builder.tool(SshExecTool::new(req.session_ctx.clone()));
    }

    let hook = AgentPolicyHook {
        session_allowed_tools: req.session_allowed_tools.clone(),
        event_tx: tx.clone(),
        cancel_token: req.cancel_token.clone(),
    };
    let agent = builder.add_hook(hook).build();

    let rig_history: Vec<Message> = req
        .history
        .into_iter()
        .filter_map(|m| match m.role {
            ChatRole::User => Some(Message::user(m.content)),
            ChatRole::Assistant if !m.content.trim().is_empty() => {
                Some(Message::assistant(m.content))
            }
            _ => None,
        })
        .collect();

    let max_turns = (req.agent_settings.max_turns as usize).clamp(1, 30);
    let mut stream = agent
        .prompt(req.user_prompt)
        .history(rig_history)
        .max_turns(max_turns)
        .tool_concurrency(1)
        .stream();

    let cancel = req.cancel_token;
    let mut seen_tool_results: HashSet<String> = HashSet::new();

    loop {
        let next_item = tokio::select! {
            _ = cancel.cancelled() => {
                let _ = tx.send(AgentEvent::Cancelled);
                return;
            }
            item = stream.next() => item,
        };

        let Some(item) = next_item else {
            break;
        };

        match item {
            Ok(MultiTurnStreamItem::StreamAssistantItem(Item::Event(ev))) => match ev {
                StreamEvent::Text { text, .. } if !text.is_empty() => {
                    let _ = tx.send(AgentEvent::TextDelta(text));
                }
                StreamEvent::Reasoning { text, .. } if !text.is_empty() => {
                    let _ = tx.send(AgentEvent::ReasoningDelta(text));
                }
                _ => {}
            },
            Ok(MultiTurnStreamItem::ToolResultItem { tool_result, .. }) => {
                let call_id = tool_result.call.to_string();
                seen_tool_results.insert(call_id.clone());
                let output = format_tool_result_content(&tool_result.content);
                let _ = tx.send(AgentEvent::ToolFinished {
                    call_id,
                    tool_name: tool_result.name.to_string(),
                    is_error: tool_result.is_error,
                    output,
                });
            }
            Ok(MultiTurnStreamItem::ToolResult { tool_result }) => {
                let call_id = tool_result.call.to_string();
                if seen_tool_results.insert(call_id.clone()) {
                    let output = format_tool_result_content(&tool_result.content);
                    let _ = tx.send(AgentEvent::ToolFinished {
                        call_id,
                        tool_name: tool_result.name.to_string(),
                        is_error: tool_result.is_error,
                        output,
                    });
                }
            }
            Ok(MultiTurnStreamItem::FinalResponse(_)) => {
                let _ = tx.send(AgentEvent::Done);
                return;
            }
            Ok(_) => {}
            Err(err) => {
                if cancel.is_cancelled() {
                    let _ = tx.send(AgentEvent::Cancelled);
                } else {
                    let _ = tx.send(AgentEvent::Error(err.to_string()));
                }
                return;
            }
        }
    }

    if cancel.is_cancelled() {
        let _ = tx.send(AgentEvent::Cancelled);
    } else {
        let _ = tx.send(AgentEvent::Done);
    }
}

fn format_tool_result_content(items: &[ToolResultContent]) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(items.len());
    for item in items {
        match item {
            ToolResultContent::Text(t) => {
                if !t.text.trim().is_empty() {
                    parts.push(t.text.clone());
                }
            }
            ToolResultContent::Json { value } => {
                parts.push(format_tool_json_preview(value));
            }
            ToolResultContent::Image(_) => {
                parts.push("[image]".to_string());
            }
        }
    }
    parts.join("\n")
}

/// 把工具返回的结构化 JSON 转成更适合人类在侧边栏查看的预览文本。
fn format_tool_json_preview(value: &serde_json::Value) -> String {
    if let Some(obj) = value.as_object() {
        // ssh_exec 输出
        if obj.contains_key("stdout") && obj.contains_key("stderr") {
            let mut out = String::new();
            if let Some(code) = obj.get("exit_code").and_then(|v| v.as_u64()) {
                out.push_str(&format!("exit_code: {code}\n"));
            }
            if let Some(s) = obj.get("stdout").and_then(|v| v.as_str()) {
                if !s.trim().is_empty() {
                    out.push_str(s.trim_end());
                    out.push('\n');
                }
            }
            if let Some(e) = obj.get("stderr").and_then(|v| v.as_str()) {
                if !e.trim().is_empty() {
                    out.push_str("[stderr]\n");
                    out.push_str(e.trim_end());
                    out.push('\n');
                }
            }
            return out.trim_end().to_string();
        }
        // run_in_terminal 输出
        if let Some(s) = obj.get("output").and_then(|v| v.as_str()) {
            return s.to_string();
        }
        // read_terminal_screen 输出
        if let Some(s) = obj.get("text").and_then(|v| v.as_str()) {
            return s.to_string();
        }
        // send_terminal_keys 输出
        if let Some(s) = obj.get("screen_tail").and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}
