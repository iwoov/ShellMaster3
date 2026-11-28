// 工具 1：read_terminal_screen —— 读取当前已打开终端的屏幕内容与最近历史

use rig_core::completion::ToolAnnotations;
use rig_core::tool::{Tool, ToolContext};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::terminal::read_term_snapshot;

use super::{truncate_output, AgentSessionContext, AgentToolError};

const MAX_OUTPUT_BYTES: usize = 16 * 1024;

#[derive(Clone)]
pub struct ReadTerminalScreenTool {
    ctx: AgentSessionContext,
}

impl ReadTerminalScreenTool {
    pub fn new(ctx: AgentSessionContext) -> Self {
        Self { ctx }
    }
}

#[derive(Debug, Deserialize)]
pub struct ReadTerminalScreenArgs {
    /// 读取末尾多少行（默认 80，最大 300）
    #[serde(default)]
    pub lines: Option<usize>,
    /// 是否包含 scrollback 滚动历史（默认 true）
    #[serde(default)]
    pub include_scrollback: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ReadTerminalScreenOutput {
    pub text: String,
    pub alt_screen: bool,
    pub screen_lines: usize,
    pub total_lines: usize,
    pub truncated: bool,
}

impl Tool for ReadTerminalScreenTool {
    const NAME: &'static str = "read_terminal_screen";

    type Args = ReadTerminalScreenArgs;
    type Output = ReadTerminalScreenOutput;
    type Error = AgentToolError;

    fn description(&self) -> String {
        "Read the current text displayed in the user's active SSH terminal (and optional scrollback history). Also reports whether the terminal is currently inside a full-screen alt-screen TUI app such as vim, top, or less.".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "lines": {
                    "type": "integer",
                    "description": "Number of recent lines to return (default 80, max 300).",
                    "minimum": 1,
                    "maximum": 300
                },
                "include_scrollback": {
                    "type": "boolean",
                    "description": "Whether to include scrollback history above the visible viewport (default true)."
                }
            },
            "additionalProperties": false
        })
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations {
            title: Some("Read Terminal Screen".to_string()),
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        }
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let Some(term) = &self.ctx.active_term else {
            return Err(AgentToolError::NoActiveTerminal);
        };
        let max_lines = args.lines.unwrap_or(80).clamp(1, 300);
        let include_scrollback = args.include_scrollback.unwrap_or(true);
        let snap = read_term_snapshot(term, max_lines, include_scrollback);
        let (text, truncated) = truncate_output(&snap.text, MAX_OUTPUT_BYTES);
        Ok(ReadTerminalScreenOutput {
            text,
            alt_screen: snap.is_alt_screen,
            screen_lines: snap.screen_lines,
            total_lines: snap.total_lines,
            truncated,
        })
    }
}
