// AI Agent 行为与安全配置模型

use serde::{Deserialize, Serialize};

/// Agent 执行终端/SSH 工具时的审批模式。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum AgentApprovalMode {
    /// 除读取终端屏幕外，所有执行/输入类工具均需人工确认
    AskAll,
    /// 默认：只读排查自动放行；操作可见终端或可能修改系统的命令需人工确认
    #[default]
    AutoReadOnly,
    /// 非高危命令自动执行；高危破坏性命令始终强制确认
    AutoSafe,
}

impl AgentApprovalMode {
    pub const ALL: [AgentApprovalMode; 3] = [
        AgentApprovalMode::AskAll,
        AgentApprovalMode::AutoReadOnly,
        AgentApprovalMode::AutoSafe,
    ];

    pub fn label_key(&self) -> &'static str {
        match self {
            AgentApprovalMode::AskAll => "settings.ai.agent.approval.ask_all",
            AgentApprovalMode::AutoReadOnly => "settings.ai.agent.approval.auto_read_only",
            AgentApprovalMode::AutoSafe => "settings.ai.agent.approval.auto_safe",
        }
    }

    pub fn desc_key(&self) -> &'static str {
        match self {
            AgentApprovalMode::AskAll => "settings.ai.agent.approval.ask_all_desc",
            AgentApprovalMode::AutoReadOnly => "settings.ai.agent.approval.auto_read_only_desc",
            AgentApprovalMode::AutoSafe => "settings.ai.agent.approval.auto_safe_desc",
        }
    }
}

/// Agent 调用推理模型时的思考深度（对不支持该参数的模型自动忽略）。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum AgentReasoningEffort {
    /// 默认：不下发思考深度参数，遵循模型/服务端自身默认行为
    #[default]
    Auto,
    /// 关闭思考（模型支持时生效）
    Off,
    /// 低深度：更快响应，适合简单排查
    Low,
    /// 中深度：平衡速度与推理质量
    Medium,
    /// 高深度：更充分推理，适合复杂排障与脚本分析
    High,
}

impl AgentReasoningEffort {
    pub const ALL: [AgentReasoningEffort; 5] = [
        AgentReasoningEffort::Auto,
        AgentReasoningEffort::Off,
        AgentReasoningEffort::Low,
        AgentReasoningEffort::Medium,
        AgentReasoningEffort::High,
    ];

    pub fn label_key(&self) -> &'static str {
        match self {
            AgentReasoningEffort::Auto => "settings.ai.agent.reasoning.auto",
            AgentReasoningEffort::Off => "settings.ai.agent.reasoning.off",
            AgentReasoningEffort::Low => "settings.ai.agent.reasoning.low",
            AgentReasoningEffort::Medium => "settings.ai.agent.reasoning.medium",
            AgentReasoningEffort::High => "settings.ai.agent.reasoning.high",
        }
    }

    pub fn desc_key(&self) -> &'static str {
        match self {
            AgentReasoningEffort::Auto => "settings.ai.agent.reasoning.auto_desc",
            AgentReasoningEffort::Off => "settings.ai.agent.reasoning.off_desc",
            AgentReasoningEffort::Low => "settings.ai.agent.reasoning.low_desc",
            AgentReasoningEffort::Medium => "settings.ai.agent.reasoning.medium_desc",
            AgentReasoningEffort::High => "settings.ai.agent.reasoning.high_desc",
        }
    }

    pub fn short_key(&self) -> &'static str {
        match self {
            AgentReasoningEffort::Auto => "ai_agent.reasoning.short.auto",
            AgentReasoningEffort::Off => "ai_agent.reasoning.short.off",
            AgentReasoningEffort::Low => "ai_agent.reasoning.short.low",
            AgentReasoningEffort::Medium => "ai_agent.reasoning.short.medium",
            AgentReasoningEffort::High => "ai_agent.reasoning.short.high",
        }
    }
}

/// Agent 运行参数与工具权限配置。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AgentSettings {
    /// 工具调用审批策略（默认 AutoReadOnly）
    pub approval_mode: AgentApprovalMode,
    /// 推理模型思考深度（默认 Auto，跟随模型默认行为）
    pub reasoning_effort: AgentReasoningEffort,
    /// 单次对话最多允许的 Agent 工具调用轮数（防止死循环）
    pub max_turns: u32,
    /// 单条命令执行超时时间（秒）
    pub command_timeout_secs: u32,
    /// 允许操作当前已打开的可见终端（run_in_terminal / send_terminal_keys）
    pub enable_pty_tools: bool,
    /// 允许通过 SSH 后台静默执行命令（ssh_exec）
    pub enable_exec_tool: bool,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            approval_mode: AgentApprovalMode::AutoReadOnly,
            reasoning_effort: AgentReasoningEffort::Auto,
            max_turns: 12,
            command_timeout_secs: 15,
            enable_pty_tools: true,
            enable_exec_tool: true,
        }
    }
}
